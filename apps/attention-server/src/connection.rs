use crate::AppState;
use crate::mapping;
use crate::time::truncate_to_microseconds;
use attention_kernel as k;
use attention_protocol as p;
use axum::extract::ws::Message;
use axum::extract::ws::WebSocket;
use futures_util::SinkExt;
use futures_util::StreamExt;
use futures_util::sink::Sink;
use serde_json::Value;
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::OwnedSemaphorePermit;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;

pub async fn run(socket: WebSocket, state: Arc<AppState>, _permit: OwnedSemaphorePermit) {
    let (mut sink, mut source) = socket.split();
    let first = tokio::select! {
        () = state.shutdown.cancelled() => {
            let _ = send_message(
                &mut sink,
                close_message(1001, "server shutdown"),
                state.config.write_timeout,
                &state.shutdown,
            ).await;
            return;
        },
        () = tokio::time::sleep(state.config.hello_frame_timeout) => {
            let _ = send_message(
                &mut sink,
                close_message(1002, "hello frame timeout"),
                state.config.write_timeout,
                &state.shutdown,
            ).await;
            return;
        },
        frame = source.next() => frame,
    };
    let Some(Ok(Message::Text(text))) = first else {
        let _ = reject_hello(
            &mut sink,
            None,
            close_message(1002, "text hello required"),
            state.config.write_timeout,
            &state.shutdown,
        )
        .await;
        return;
    };
    let raw: Value = if let Ok(value) = serde_json::from_str(&text) {
        value
    } else {
        let _ = reject_hello(
            &mut sink,
            Some(error(Value::Null, p::PARSE_ERROR, "Parse error", None)),
            close_message(1002, "invalid hello"),
            state.config.write_timeout,
            &state.shutdown,
        )
        .await;
        return;
    };
    if !safe_json(
        &raw,
        state.config.max_json_depth,
        state.config.max_json_nodes,
        false,
    ) {
        let _ = reject_hello(
            &mut sink,
            Some(error(
                Value::Null,
                p::INVALID_REQUEST,
                "Invalid request",
                None,
            )),
            close_message(1002, "invalid hello"),
            state.config.write_timeout,
            &state.shutdown,
        )
        .await;
        return;
    }
    let recovered_id = recover_id(&raw);
    let request: p::RpcRequest<Value> = match serde_json::from_value::<p::RpcRequest<Value>>(raw) {
        Ok(request) if request.params.is_some() => request,
        _ => {
            let _ = reject_hello(
                &mut sink,
                Some(error(
                    recovered_id,
                    p::INVALID_REQUEST,
                    "Invalid request",
                    None,
                )),
                close_message(1002, "invalid hello"),
                state.config.write_timeout,
                &state.shutdown,
            )
            .await;
            return;
        }
    };
    let id = json!(request.id);
    if request.method != p::RPC_HELLO_METHOD {
        let _ = reject_hello(
            &mut sink,
            Some(error(id, p::HELLO_REQUIRED, "Hello required", None)),
            close_message(1002, "hello required"),
            state.config.write_timeout,
            &state.shutdown,
        )
        .await;
        return;
    }
    let hello: p::HelloRequest = if let Some(hello) = request
        .params
        .and_then(|value| serde_json::from_value(value).ok())
    {
        hello
    } else {
        let _ = reject_hello(
            &mut sink,
            Some(error(id, p::INVALID_PARAMS, "Invalid params", None)),
            close_message(1002, "invalid hello params"),
            state.config.write_timeout,
            &state.shutdown,
        )
        .await;
        return;
    };
    if hello.protocol_version != p::PROTOCOL_V1 {
        let e = p::RpcError::unsupported_protocol_version(&[p::PROTOCOL_V1]);
        let _ = reject_hello(
            &mut sink,
            Some(error(id, e.code, &e.message, e.data)),
            close_message(1002, "unsupported protocol version"),
            state.config.write_timeout,
            &state.shutdown,
        )
        .await;
        return;
    }
    let mut subscription = match prepare_subscription(&state, &hello.subscription).await {
        Ok(value) => value,
        Err(PrepareError::Gap(gap)) => {
            let _ = reject_hello(
                &mut sink,
                Some(error(
                    id,
                    p::CURSOR_GAP,
                    "Cursor gap",
                    serde_json::to_value(gap).ok(),
                )),
                close_message(1008, "cursor gap; snapshot required"),
                state.config.write_timeout,
                &state.shutdown,
            )
            .await;
            return;
        }
        Err(PrepareError::Internal) => {
            let _ = reject_hello(
                &mut sink,
                Some(error(id, p::INTERNAL_ERROR, "Internal error", None)),
                close_message(1011, "subscription preparation failed"),
                state.config.write_timeout,
                &state.shutdown,
            )
            .await;
            return;
        }
        Err(PrepareError::Shutdown) => {
            let _ = send_message(
                &mut sink,
                close_message(1001, "server shutdown"),
                state.config.write_timeout,
                &state.shutdown,
            )
            .await;
            return;
        }
    };
    let result = p::AttentionHelloResult {
        protocol_version: p::PROTOCOL_V1,
        server_id: state.identity.server_id.clone(),
        boot_id: state.identity.boot_id.clone(),
        stream_id: state.identity.stream_id.clone(),
        subscription_result: subscription.result,
        limits: p::HelloLimits {
            max_message_bytes: state.config.max_message_bytes as u32,
            max_in_flight: state.config.max_in_flight as u32,
        },
    };
    if send_json(
        &mut sink,
        &json!({"jsonrpc":"2.0","id":id,"result":result}),
        state.config.write_timeout,
        &state.shutdown,
    )
    .await
    .is_err()
    {
        return;
    }
    for event in subscription.replay.drain(..) {
        if send_event(
            &mut sink,
            &event,
            state.config.write_timeout,
            &state.shutdown,
        )
        .await
        .is_err()
        {
            return;
        }
    }

    let mut last_sent = subscription.last_sent;
    let (out_tx, mut out_rx) = mpsc::channel::<Value>(state.config.outbound_capacity);
    let response_overflow = tokio_util::sync::CancellationToken::new();
    let in_flight = Arc::new(Semaphore::new(state.config.max_in_flight));
    let mut requests = tokio::task::JoinSet::new();
    let close = loop {
        let (live_receiver, live_overflow) = match subscription.live.as_mut() {
            Some(live) => (Some(&mut live.receiver), Some(&mut live.overflow)),
            None => (None, None),
        };
        tokio::select! {
            () = state.shutdown.cancelled() => break close_message(1001, "server shutdown"),
            () = response_overflow.cancelled() => break close_message(1013, "outbound overflow; reconnect required"),
            Some(value) = out_rx.recv() => if send_json(&mut sink, &value, state.config.write_timeout, &state.shutdown).await.is_err() { return; },
            overflow = async { match live_overflow { Some(signal) => signal.await.ok(), None => std::future::pending().await } } => {
                let _ = overflow;
                break close_message(1013, "publication overflow; resume required");
            },
            event = async { match live_receiver { Some(receiver) => receiver.recv().await, None => std::future::pending().await } } => {
                let Some(event) = event else { break close_message(1001, "publication closed"); };
                let Some(previous) = last_sent else { break close_message(1011, "invalid subscription state"); };
                if event.cursor() <= previous { continue; }
                if previous.value().checked_add(1) != Some(event.cursor().value()) {
                    break close_message(1011, "cursor discontinuity; resume required");
                }
                last_sent = Some(event.cursor());
                let Ok(event) = mapping::event(&event) else { break close_message(1011, "event mapping failed"); };
                if out_tx.try_send(notification(event)).is_err() {
                    break close_message(1013, "outbound overflow; reconnect required");
                }
            },
            Some(joined) = requests.join_next(), if !requests.is_empty() => {
                if joined.is_err() { break close_message(1011, "request task failed"); }
            },
            message = source.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    if text.len() > state.config.max_message_bytes { break close_message(1009, "message too large"); }
                    let raw: Value = if let Ok(raw) = serde_json::from_str(&text) {
                        raw
                    } else {
                        if out_tx.try_send(error_value(Value::Null, p::PARSE_ERROR, "Parse error", None)).is_err() {
                            break close_message(1013, "outbound overflow; reconnect required");
                        }
                        continue;
                    };
                    let allows_lease_token = raw
                        .get("method")
                        .and_then(Value::as_str)
                        .is_some_and(is_delivery_worker_method);
                    if !safe_json(
                        &raw,
                        state.config.max_json_depth,
                        state.config.max_json_nodes,
                        allows_lease_token,
                    ) {
                        if out_tx.try_send(error_value(Value::Null, p::INVALID_REQUEST, "Invalid request", None)).is_err() {
                            break close_message(1013, "outbound overflow; reconnect required");
                        }
                        continue;
                    }
                    let recovered = recover_id(&raw);
                    let request: p::RpcRequest<Value> =
                        match serde_json::from_value::<p::RpcRequest<Value>>(raw) {
                            Ok(request) if request.params.is_some() => request,
                        _ => {
                            if out_tx.try_send(error_value(recovered, p::INVALID_REQUEST, "Invalid request", None)).is_err() {
                                break close_message(1013, "outbound overflow; reconnect required");
                            }
                            continue;
                        }
                    };
                    let request_id = json!(request.id);
                    let Ok(request_permit) = Arc::clone(&in_flight).try_acquire_owned() else {
                        if out_tx.try_send(error_value(request_id, p::INTERNAL_ERROR, "Too many in-flight requests", None)).is_err() {
                            break close_message(1013, "outbound overflow; reconnect required");
                        }
                        continue;
                    };
                    let tx = out_tx.clone();
                    let state = Arc::clone(&state);
                    let overflow = response_overflow.clone();
                    requests.spawn(async move {
                        let _request_permit = request_permit;
                        let response = dispatch_request(&state, request).await;
                        if tx.try_send(response).is_err() { overflow.cancel(); }
                    });
                },
                Some(Ok(Message::Ping(data))) => if send_message(&mut sink, Message::Pong(data), state.config.write_timeout, &state.shutdown).await.is_err() { return; },
                Some(Ok(Message::Pong(_))) => {},
                Some(Ok(Message::Binary(_))) => break close_message(1003, "binary messages unsupported"),
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
            }
        }
    };
    let _ = send_message(
        &mut sink,
        close,
        state.config.write_timeout,
        &state.shutdown,
    )
    .await;
    drop(out_tx);
    let drain = async { while requests.join_next().await.is_some() {} };
    if tokio::time::timeout(state.config.shutdown_grace, drain)
        .await
        .is_err()
    {
        requests.abort_all();
        while requests.join_next().await.is_some() {}
    }
}

fn safe_json(root: &Value, max_depth: usize, max_nodes: usize, allow_lease_token: bool) -> bool {
    const FORBIDDEN_FIELDS: [&str; 7] = [
        "password",
        "secret",
        "token",
        "authorization",
        "credential",
        "api_key",
        "apikey",
    ];
    const FORBIDDEN_SUFFIXES: [&str; 7] = [
        "_password",
        "_secret",
        "_token",
        "_authorization",
        "_credential",
        "_api_key",
        "_apikey",
    ];
    let mut stack = vec![(root, 1usize)];
    let mut nodes = 0usize;
    while let Some((value, depth)) = stack.pop() {
        nodes += 1;
        if depth > max_depth || nodes > max_nodes {
            return false;
        }
        match value {
            Value::Array(values) => stack.extend(values.iter().map(|value| (value, depth + 1))),
            Value::Object(values) => {
                for (key, value) in values {
                    let normalized = key.to_ascii_lowercase().replace(['-', ' '], "_");
                    if !(allow_lease_token && normalized == "lease_token")
                        && (FORBIDDEN_FIELDS.contains(&normalized.as_str())
                            || FORBIDDEN_SUFFIXES
                                .iter()
                                .any(|suffix| normalized.ends_with(suffix)))
                    {
                        return false;
                    }
                    stack.push((value, depth + 1));
                }
            }
            _ => {}
        }
    }
    true
}

fn is_delivery_worker_method(method: &str) -> bool {
    matches!(
        method,
        <p::DeliveryClaim as p::RpcMethod>::NAME
            | <p::DeliveryInspect as p::RpcMethod>::NAME
            | <p::DeliveryRenew as p::RpcMethod>::NAME
            | <p::DeliverySucceed as p::RpcMethod>::NAME
            | <p::DeliveryFailRetryable as p::RpcMethod>::NAME
            | <p::DeliveryFailTerminal as p::RpcMethod>::NAME
    )
}

fn safe_source_fields(root: &Value, max_component: usize, max_order: usize) -> bool {
    let mut stack = vec![root];
    while let Some(value) = stack.pop() {
        match value {
            Value::Array(values) => stack.extend(values),
            Value::Object(values) => {
                for (key, value) in values {
                    if matches!(
                        key.as_str(),
                        "source_kind"
                            | "source_instance"
                            | "external_entity_id"
                            | "occurrence_id"
                            | "domain"
                    ) && value
                        .as_str()
                        .is_some_and(|text| text.len() > max_component)
                    {
                        return false;
                    }
                    if key == "value"
                        && values.contains_key("domain")
                        && value
                            .as_str()
                            .is_some_and(|text| text.len().saturating_mul(3) / 4 > max_order)
                    {
                        return false;
                    }
                    stack.push(value);
                }
            }
            _ => {}
        }
    }
    true
}

fn recover_id(raw: &Value) -> Value {
    raw.get("id")
        .and_then(Value::as_str)
        .map_or(Value::Null, |id| json!(id))
}

fn close_message(code: u16, reason: &'static str) -> Message {
    Message::Close(Some(axum::extract::ws::CloseFrame {
        code,
        reason: reason.into(),
    }))
}

struct Prepared {
    result: p::SubscriptionResult<p::AttentionSnapshot>,
    replay: Vec<p::ChangeEvent>,
    live: Option<crate::PublicationSubscription>,
    last_sent: Option<k::CommitCursor>,
}
#[derive(Debug)]
enum PrepareError {
    Gap(p::CursorGap),
    Internal,
    Shutdown,
}

async fn prepare_subscription(
    state: &Arc<AppState>,
    request: &p::SubscriptionRequest,
) -> Result<Prepared, PrepareError> {
    match request {
        p::SubscriptionRequest::None => Ok(Prepared {
            result: p::SubscriptionResult::None,
            replay: vec![],
            live: None,
            last_sent: None,
        }),
        p::SubscriptionRequest::Snapshot => {
            let live = state.publications.subscribe();
            let snap = tokio::select! {
                () = state.shutdown.cancelled() => return Err(PrepareError::Shutdown),
                result = state.service.snapshot() => result.map_err(|_| PrepareError::Internal)?,
            };
            let state_wire = mapping::snapshot(&snap).map_err(|_| PrepareError::Internal)?;
            Ok(Prepared {
                result: p::SubscriptionResult::Snapshot {
                    state: state_wire,
                    after_cursor: mapping::cursor(snap.cursor()),
                },
                replay: vec![],
                live: Some(live),
                last_sent: Some(snap.cursor()),
            })
        }
        p::SubscriptionRequest::Resume {
            server_id,
            stream_id,
            after_cursor,
        } => {
            if server_id != &state.identity.server_id || stream_id != &state.identity.stream_id {
                return Err(PrepareError::Gap(invalid_gap(after_cursor.clone())));
            }
            let mut last = mapping::parse_cursor(after_cursor)
                .map_err(|_| PrepareError::Gap(invalid_gap(after_cursor.clone())))?;
            let mut live = state.publications.subscribe();
            let mut replay = Vec::new();
            loop {
                let limit = k::QueryLimit::try_from(state.config.replay_page_size)
                    .map_err(|_| PrepareError::Internal)?;
                let result = tokio::select! {
                    () = state.shutdown.cancelled() => return Err(PrepareError::Shutdown),
                    result = state.service.changes_after(k::ChangesAfterQuery::new(last, limit)) =>
                        result.map_err(|_| PrepareError::Internal)?,
                };
                match result {
                    k::ChangesResult::Gap(gap) => return Err(PrepareError::Gap(map_gap(gap))),
                    k::ChangesResult::Page(page) => {
                        let next = validate_replay_page(last, &page)?;
                        for event in page.events() {
                            replay.push(mapping::event(event).map_err(|_| PrepareError::Internal)?);
                        }
                        last = next;
                        if !page.has_more() {
                            break;
                        }
                    }
                }
            }
            while let Ok(event) = live.receiver.try_recv() {
                if event.cursor() <= last {
                    continue;
                }
                if last.value().checked_add(1) != Some(event.cursor().value()) {
                    return Err(PrepareError::Internal);
                }
                replay.push(mapping::event(&event).map_err(|_| PrepareError::Internal)?);
                last = event.cursor();
            }
            Ok(Prepared {
                result: p::SubscriptionResult::Resume {
                    after_cursor: mapping::cursor(last),
                },
                replay,
                live: Some(live),
                last_sent: Some(last),
            })
        }
    }
}

fn validate_replay_page(
    after: k::CommitCursor,
    page: &k::ChangePage,
) -> Result<k::CommitCursor, PrepareError> {
    let mut last = after;
    for event in page.events() {
        let expected = last.value().checked_add(1).ok_or(PrepareError::Internal)?;
        if event.cursor().value() != expected {
            return Err(PrepareError::Internal);
        }
        last = event.cursor();
    }
    if page.resume_after() != last || (page.has_more() && last == after) {
        return Err(PrepareError::Internal);
    }
    Ok(last)
}

fn invalid_gap(requested: p::Cursor) -> p::CursorGap {
    p::CursorGap {
        reason: p::CursorGapReason::Invalid,
        requested_after: requested,
        earliest_available: None,
        latest_available: None,
        snapshot_required: p::SnapshotRequired,
    }
}
fn map_gap(g: k::ChangeGap) -> p::CursorGap {
    match g {
        k::ChangeGap::Expired {
            requested_after,
            earliest_available,
            latest_available,
        } => p::CursorGap {
            reason: p::CursorGapReason::Expired,
            requested_after: mapping::cursor(requested_after),
            earliest_available: Some(mapping::cursor(earliest_available)),
            latest_available: Some(mapping::cursor(latest_available)),
            snapshot_required: p::SnapshotRequired,
        },
        k::ChangeGap::Future {
            requested_after,
            latest_available,
        } => p::CursorGap {
            reason: p::CursorGapReason::Future,
            requested_after: mapping::cursor(requested_after),
            earliest_available: None,
            latest_available: Some(mapping::cursor(latest_available)),
            snapshot_required: p::SnapshotRequired,
        },
    }
}
#[expect(
    clippy::needless_pass_by_value,
    reason = "JSON ownership is consumed by serialization"
)]
fn notification(event: p::ChangeEvent) -> Value {
    json!({"jsonrpc":"2.0","method":"attention.change","params":{"event":event}})
}
async fn send_event(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    event: &p::ChangeEvent,
    write_timeout: Duration,
    shutdown: &tokio_util::sync::CancellationToken,
) -> Result<(), SendError> {
    send_json(sink, &notification(event.clone()), write_timeout, shutdown).await
}
async fn send_json(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    value: &Value,
    write_timeout: Duration,
    shutdown: &tokio_util::sync::CancellationToken,
) -> Result<(), SendError> {
    let text = serde_json::to_string(value).map_err(|_| SendError::Serialization)?;
    send_message(sink, Message::Text(text), write_timeout, shutdown).await
}

#[derive(Debug, PartialEq, Eq)]
enum SendError {
    Serialization,
    Transport,
    Timeout,
    Cancelled,
}

async fn send_message<S>(
    sink: &mut S,
    message: Message,
    write_timeout: Duration,
    shutdown: &tokio_util::sync::CancellationToken,
) -> Result<(), SendError>
where
    S: Sink<Message> + Unpin,
{
    let send = tokio::time::timeout(write_timeout, sink.send(message));
    tokio::pin!(send);
    tokio::select! {
        biased;
        result = &mut send => match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) => Err(SendError::Transport),
            Err(_) => Err(SendError::Timeout),
        },
        () = shutdown.cancelled() => Err(SendError::Cancelled),
    }
}

async fn reject_hello<S>(
    sink: &mut S,
    error: Option<Message>,
    close: Message,
    write_timeout: Duration,
    shutdown: &tokio_util::sync::CancellationToken,
) -> Result<(), SendError>
where
    S: Sink<Message> + Unpin,
{
    if let Some(error) = error {
        send_message(sink, error, write_timeout, shutdown).await?;
    }
    send_message(sink, close, write_timeout, shutdown).await
}
#[expect(
    clippy::needless_pass_by_value,
    reason = "JSON ownership is consumed by serialization"
)]
fn error(id: Value, code: p::ErrorCode, message: &str, data: Option<Value>) -> Message {
    Message::Text(
        serde_json::to_string(
            &json!({"jsonrpc":"2.0","id":id,"error":p::RpcError{code,message:message.into(),data}}),
        )
        .unwrap_or_default(),
    )
}
async fn dispatch_request(state: &Arc<AppState>, request: p::RpcRequest<Value>) -> Value {
    let id = json!(request.id);
    let method = request.method.as_str();
    let Some(params) = request.params else {
        return error_value(id, p::INVALID_REQUEST, "Invalid request", None);
    };
    if !safe_source_fields(
        &params,
        state.config.max_source_component_bytes,
        state.config.max_source_order_bytes,
    ) {
        return error_value(id, p::INVALID_PARAMS, "Invalid params", None);
    }
    let source_bounds = state.config.source_bounds();
    macro_rules! read {($params:ty,$parse:expr,$call:expr,$map:expr,$resource:expr)=>{{let Ok(v)=serde_json::from_value::<$params>(params)else{return error_value(id,p::INVALID_PARAMS,"Invalid params",None)};let Ok(native_id)=$parse(&v)else{return error_value(id,p::INVALID_PARAMS,"Invalid params",None)};match $call(native_id).await{Ok(Some(x))=>match $map(&x){Ok(x)=>json!({"jsonrpc":"2.0","id":id,"result":x}),Err(_)=>internal(id)},Ok(None)=>error_value(id,p::RESOURCE_NOT_FOUND,"Resource not found",Some(json!({"resource":$resource(v)}))),Err(error)=>service_error(id,&error)}}};}
    macro_rules! mutate {($params:ty,$parse:expr,$method:ident,$map:path)=>{{
        let Some(service) = state.mutations.as_ref() else { return error_value(id,p::METHOD_NOT_FOUND,"Method not found",None) };
        let Ok(wire)=serde_json::from_value::<$params>(params) else { return error_value(id,p::INVALID_PARAMS,"Invalid params",None) };
        let Ok(command)=($parse)(&wire) else { return error_value(id,p::INVALID_PARAMS,"Invalid params",None) };
        match service.$method(command).await {
            Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":$map(&result)}),
            Err(error) => service_error(id,&error),
        }
    }};}
    match method {
        <p::WorkItemGet as p::RpcMethod>::NAME => read!(
            p::WorkItemGetParams,
            |v: &p::WorkItemGetParams| mapping::parse_work_item_id(&v.id),
            |native_id| state.service.work_item(native_id),
            mapping::work_item,
            |v: p::WorkItemGetParams| p::ResourceRef::WorkItem { id: v.id }
        ),
        <p::AttentionSignalGet as p::RpcMethod>::NAME => read!(
            p::AttentionSignalGetParams,
            |v: &p::AttentionSignalGetParams| mapping::parse_signal_id(&v.id),
            |native_id| state.service.attention_signal(native_id),
            mapping::signal,
            |v: p::AttentionSignalGetParams| p::ResourceRef::AttentionSignal { id: v.id }
        ),
        <p::ReminderGet as p::RpcMethod>::NAME => read!(
            p::ReminderGetParams,
            |v: &p::ReminderGetParams| mapping::parse_reminder_id(&v.id),
            |native_id| state.service.reminder(native_id),
            mapping::reminder,
            |v: p::ReminderGetParams| p::ResourceRef::Reminder { id: v.id }
        ),
        <p::SourceReceiptGet as p::RpcMethod>::NAME => read!(
            p::SourceReceiptGetParams,
            |v: &p::SourceReceiptGetParams| mapping::parse_receipt_id(&v.id),
            |native_id| state.service.source_receipt(native_id),
            mapping::receipt,
            |v: p::SourceReceiptGetParams| p::ResourceRef::SourceReceipt { id: v.id }
        ),
        <p::SourceEntityGet as p::RpcMethod>::NAME => {
            let Ok(v) = serde_json::from_value::<p::SourceEntityGetParams>(params) else {
                return error_value(id, p::INVALID_PARAMS, "Invalid params", None);
            };
            let Ok(key) = mapping::parse_source_key(&v.key, source_bounds) else {
                return error_value(id, p::INVALID_PARAMS, "Invalid params", None);
            };
            match state
                .service
                .source_entity(k::SourceAuthorityQuery::new(key))
                .await
            {
                Ok(Some(x)) => match mapping::entity(&x) {
                    Ok(x) => json!({"jsonrpc":"2.0","id":id,"result":x}),
                    Err(_) => internal(id),
                },
                Ok(None) => error_value(
                    id,
                    p::RESOURCE_NOT_FOUND,
                    "Resource not found",
                    Some(json!({"resource":p::ResourceRef::SourceEntity{key:v.key}})),
                ),
                Err(error) => service_error(id, &error),
            }
        }
        <p::SnapshotGet as p::RpcMethod>::NAME => match state.service.snapshot().await {
            Ok(x) => match mapping::snapshot(&x) {
                Ok(s) => {
                    json!({"jsonrpc":"2.0","id":id,"result":p::SnapshotResult{state:s,after_cursor:mapping::cursor(x.cursor())}})
                }
                Err(_) => internal(id),
            },
            Err(error) => service_error(id, &error),
        },
        <p::ChangesGet as p::RpcMethod>::NAME => {
            let Ok(v) = serde_json::from_value::<p::ChangesGetParams>(params) else {
                return error_value(id, p::INVALID_PARAMS, "Invalid params", None);
            };
            let Ok(after) = mapping::parse_cursor(&v.after_cursor) else {
                return error_value(id, p::INVALID_PARAMS, "Invalid params", None);
            };
            let Ok(limit) = k::QueryLimit::try_from(v.limit as usize) else {
                return error_value(id, p::INVALID_PARAMS, "Invalid params", None);
            };
            match state
                .service
                .changes_after(k::ChangesAfterQuery::new(after, limit))
                .await
            {
                Ok(k::ChangesResult::Page(page)) => {
                    let events = page
                        .events()
                        .iter()
                        .map(mapping::event)
                        .collect::<Result<Vec<_>, _>>();
                    match events {
                        Ok(events) => {
                            json!({"jsonrpc":"2.0","id":id,"result":p::ChangesResult::Page{events,resume_after:mapping::cursor(page.resume_after()),has_more:page.has_more()}})
                        }
                        Err(_) => internal(id),
                    }
                }
                Ok(k::ChangesResult::Gap(g)) => {
                    json!({"jsonrpc":"2.0","id":id,"result":p::ChangesResult::Gap{gap:map_gap(g)}})
                }
                Err(error) => service_error(id, &error),
            }
        }
        <p::WorkItemCreate as p::RpcMethod>::NAME => mutate!(
            p::CreateWorkItemParams,
            |wire| mapping::create_work_item_command(wire, source_bounds),
            create_work_item,
            mapping::work_item_result
        ),
        <p::WorkItemComplete as p::RpcMethod>::NAME => mutate!(
            p::CompleteWorkItemParams,
            mapping::complete_work_item_command,
            complete_work_item,
            mapping::work_item_result
        ),
        <p::WorkItemCancel as p::RpcMethod>::NAME => mutate!(
            p::CancelWorkItemParams,
            mapping::cancel_work_item_command,
            cancel_work_item,
            mapping::work_item_result
        ),
        <p::AttentionSignalAcknowledge as p::RpcMethod>::NAME => mutate!(
            p::AcknowledgeAttentionSignalParams,
            mapping::acknowledge_signal_command,
            acknowledge_attention_signal,
            mapping::signal_result
        ),
        <p::SourceOccurrenceIngest as p::RpcMethod>::NAME => mutate!(
            p::IngestSourceOccurrenceParams,
            |wire| mapping::ingest_source_command(
                wire,
                source_bounds,
                truncate_to_microseconds(state.clock.now()),
            ),
            ingest_source_occurrence,
            mapping::source_result
        ),
        <p::ReminderCreate as p::RpcMethod>::NAME => mutate!(
            p::CreateReminderParams,
            mapping::create_reminder_command,
            create_reminder,
            mapping::reminder_result
        ),
        <p::ReminderFireAcknowledge as p::RpcMethod>::NAME => mutate!(
            p::AcknowledgeReminderFireParams,
            mapping::acknowledge_reminder_command,
            acknowledge_reminder_fire,
            mapping::reminder_result
        ),
        <p::ReminderFireSnooze as p::RpcMethod>::NAME => mutate!(
            p::SnoozeReminderFireParams,
            mapping::snooze_reminder_command,
            snooze_reminder_fire,
            mapping::reminder_result
        ),
        <p::DeliveryClaim as p::RpcMethod>::NAME => {
            let Some(service) = state.delivery_workers.as_ref() else {
                return method_not_found(id);
            };
            let Ok(v) = serde_json::from_value::<p::DeliveryClaimParams>(params) else {
                return invalid_params(id);
            };
            let Ok(limit_value) = usize::try_from(v.limit) else {
                return invalid_params(id);
            };
            if limit_value > state.config.max_delivery_claims || v.lease_expires_at <= v.eligible_at
            {
                return invalid_params(id);
            }
            let Ok(limit) = k::ClaimLimit::try_from(limit_value) else {
                return invalid_params(id);
            };
            match service
                .claim(k::DeliveryClaimQuery::new(
                    mapping::parse_wire_time(&v.eligible_at),
                    mapping::parse_wire_time(&v.lease_expires_at),
                    limit,
                ))
                .await
            {
                Ok(claims) => {
                    let claims = claims
                        .into_iter()
                        .map(|claim| {
                            Ok(p::DeliveryClaimCapability {
                                intent_id: mapping::intent_id(claim.intent_id()),
                                lease_token: mapping::delivery_token(claim.token())?,
                                expires_at: mapping::wire_time(claim.expires_at())?,
                            })
                        })
                        .collect::<Result<Vec<_>, mapping::MappingError>>();
                    match claims {
                        Ok(claims) => {
                            json!({"jsonrpc":"2.0","id":id,"result":p::DeliveryClaimResult{claims}})
                        }
                        Err(_) => internal(id),
                    }
                }
                Err(error) => service_error(id, &error),
            }
        }
        <p::DeliveryInspect as p::RpcMethod>::NAME => {
            let Some(service) = state.delivery_workers.as_ref() else {
                return method_not_found(id);
            };
            let Ok(v) = serde_json::from_value::<p::DeliveryInspectParams>(params) else {
                return invalid_params(id);
            };
            let Ok(intent_id) = mapping::parse_intent_id(&v.intent_id) else {
                return invalid_params(id);
            };
            match service.inspect(intent_id).await {
                Ok(Some(authority)) => match mapping::delivery_authority(&authority) {
                    Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                    Err(_) => internal(id),
                },
                Ok(None) => error_value(
                    id,
                    p::DELIVERY_NOT_FOUND,
                    "Delivery not found",
                    Some(json!({"intent_id":v.intent_id})),
                ),
                Err(error) => service_error(id, &error),
            }
        }
        <p::DeliveryRenew as p::RpcMethod>::NAME => {
            let Some(service) = state.delivery_workers.as_ref() else {
                return method_not_found(id);
            };
            let Ok(v) = serde_json::from_value::<p::DeliveryRenewParams>(params) else {
                return invalid_params(id);
            };
            let (Ok(intent_id), Ok(token)) = (
                mapping::parse_intent_id(&v.intent_id),
                mapping::parse_delivery_token(&v.lease_token),
            ) else {
                return invalid_params(id);
            };
            match service
                .renew(intent_id, token, mapping::parse_wire_time(&v.expires_at))
                .await
            {
                Ok(outcome) => {
                    json!({"jsonrpc":"2.0","id":id,"result":p::DeliveryRenewResult{outcome:map_renew(outcome)}})
                }
                Err(error) => service_error(id, &error),
            }
        }
        <p::DeliverySucceed as p::RpcMethod>::NAME => {
            let Some(service) = state.delivery_workers.as_ref() else {
                return method_not_found(id);
            };
            let Ok(v) = serde_json::from_value::<p::DeliverySucceedParams>(params) else {
                return invalid_params(id);
            };
            let (Ok(intent_id), Ok(token), Ok(provider)) = (
                mapping::parse_intent_id(&v.intent_id),
                mapping::parse_delivery_token(&v.lease_token),
                k::ProviderMessageId::new(
                    v.provider_message_id.as_str(),
                    state.config.max_delivery_text_bytes,
                ),
            ) else {
                return invalid_params(id);
            };
            match service
                .succeed(
                    intent_id,
                    token,
                    provider,
                    mapping::parse_wire_time(&v.succeeded_at),
                )
                .await
            {
                Ok(outcome) => completion(id, outcome),
                Err(error) => service_error(id, &error),
            }
        }
        <p::DeliveryFailRetryable as p::RpcMethod>::NAME => {
            let Some(service) = state.delivery_workers.as_ref() else {
                return method_not_found(id);
            };
            let Ok(v) = serde_json::from_value::<p::DeliveryFailRetryableParams>(params) else {
                return invalid_params(id);
            };
            let (Ok(intent_id), Ok(token), Ok(error)) = (
                mapping::parse_intent_id(&v.intent_id),
                mapping::parse_delivery_token(&v.lease_token),
                k::BoundedDeliveryText::new(v.error, state.config.max_delivery_text_bytes),
            ) else {
                return invalid_params(id);
            };
            match service
                .fail_retryable(
                    intent_id,
                    token,
                    v.attempt,
                    error,
                    mapping::parse_wire_time(&v.next_retry_at),
                )
                .await
            {
                Ok(outcome) => completion(id, outcome),
                Err(error) => service_error(id, &error),
            }
        }
        <p::DeliveryFailTerminal as p::RpcMethod>::NAME => {
            let Some(service) = state.delivery_workers.as_ref() else {
                return method_not_found(id);
            };
            let Ok(v) = serde_json::from_value::<p::DeliveryFailTerminalParams>(params) else {
                return invalid_params(id);
            };
            let (Ok(intent_id), Ok(token), Ok(error)) = (
                mapping::parse_intent_id(&v.intent_id),
                mapping::parse_delivery_token(&v.lease_token),
                k::BoundedDeliveryText::new(v.error, state.config.max_delivery_text_bytes),
            ) else {
                return invalid_params(id);
            };
            match service
                .fail_terminal(
                    intent_id,
                    token,
                    v.attempt,
                    error,
                    mapping::parse_wire_time(&v.failed_at),
                )
                .await
            {
                Ok(outcome) => completion(id, outcome),
                Err(error) => service_error(id, &error),
            }
        }
        _ => method_not_found(id),
    }
}
fn map_renew(outcome: k::RenewOutcome) -> p::DeliveryRenewalOutcome {
    match outcome {
        k::RenewOutcome::Renewed => p::DeliveryRenewalOutcome::Renewed,
        k::RenewOutcome::Fenced => p::DeliveryRenewalOutcome::Fenced,
        k::RenewOutcome::NotLeased => p::DeliveryRenewalOutcome::NotLeased,
    }
}
#[expect(
    clippy::needless_pass_by_value,
    reason = "JSON ownership is consumed by serialization"
)]
fn completion(id: Value, outcome: k::DeliveryCompletionOutcome) -> Value {
    let outcome = match outcome {
        k::DeliveryCompletionOutcome::Applied => p::DeliveryCompletionOutcome::Applied,
        k::DeliveryCompletionOutcome::Repeated => p::DeliveryCompletionOutcome::Repeated,
        k::DeliveryCompletionOutcome::Fenced => p::DeliveryCompletionOutcome::Fenced,
        k::DeliveryCompletionOutcome::Conflict => p::DeliveryCompletionOutcome::Conflict,
    };
    json!({"jsonrpc":"2.0","id":id,"result":p::DeliveryCompletionResult{outcome}})
}
fn invalid_params(id: Value) -> Value {
    error_value(id, p::INVALID_PARAMS, "Invalid params", None)
}
fn method_not_found(id: Value) -> Value {
    error_value(id, p::METHOD_NOT_FOUND, "Method not found", None)
}
fn internal(id: Value) -> Value {
    error_value(id, p::INTERNAL_ERROR, "Internal error", None)
}
fn service_error(id: Value, error: &crate::ServiceError) -> Value {
    if matches!(error, crate::ServiceError::InvalidParams) {
        return error_value(id, p::INVALID_PARAMS, "Invalid params", None);
    }
    match crate::service::peer_error(error) {
        Some(error) => error_value(id, error.code, &error.message, error.data),
        None => internal(id),
    }
}
#[expect(
    clippy::needless_pass_by_value,
    reason = "JSON ownership is consumed by serialization"
)]
fn error_value(id: Value, code: p::ErrorCode, message: &str, data: Option<Value>) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":p::RpcError{code,message:message.into(),data}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::DateTime;
    use chrono::Utc;
    use std::pin::Pin;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::task::Context;
    use std::task::Poll;

    #[derive(Clone, Copy)]
    enum SinkBehavior {
        Ready,
        Failed,
        Pending,
    }

    struct MockSink {
        behavior: SinkBehavior,
        sent: Arc<AtomicUsize>,
    }

    impl MockSink {
        fn new(behavior: SinkBehavior) -> (Self, Arc<AtomicUsize>) {
            let sent = Arc::new(AtomicUsize::new(0));
            (
                Self {
                    behavior,
                    sent: Arc::clone(&sent),
                },
                sent,
            )
        }
    }

    impl Sink<Message> for MockSink {
        type Error = &'static str;

        fn poll_ready(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            match self.behavior {
                SinkBehavior::Ready => Poll::Ready(Ok(())),
                SinkBehavior::Failed => Poll::Ready(Err("send failed")),
                SinkBehavior::Pending => Poll::Pending,
            }
        }

        fn start_send(self: Pin<&mut Self>, _item: Message) -> Result<(), Self::Error> {
            self.sent.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _context: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    fn cursor(value: u64) -> k::CommitCursor {
        k::CommitCursor::try_from(value).expect("nonzero cursor")
    }
    fn event(value: u64) -> k::ChangeEvent {
        k::ChangeEventDraft::new(
            k::ChangeEventId::new(),
            DateTime::parse_from_rfc3339("2026-08-13T12:00:00Z")
                .expect("timestamp")
                .with_timezone(&Utc),
            k::ChangeKind::WorkItemCreated,
            vec![],
            k::InboxEffects::default(),
        )
        .commit(cursor(value))
    }

    #[tokio::test]
    async fn bounded_send_prefers_immediately_ready_success() {
        let (mut sink, sent) = MockSink::new(SinkBehavior::Ready);
        let shutdown = tokio_util::sync::CancellationToken::new();
        shutdown.cancel();
        assert_eq!(
            send_message(
                &mut sink,
                Message::Text("ready".into()),
                Duration::from_secs(1),
                &shutdown,
            )
            .await,
            Ok(())
        );
        assert_eq!(sent.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn bounded_send_reports_transport_failure() {
        let (mut sink, sent) = MockSink::new(SinkBehavior::Failed);
        assert_eq!(
            send_message(
                &mut sink,
                Message::Text("failed".into()),
                Duration::from_secs(1),
                &tokio_util::sync::CancellationToken::new(),
            )
            .await,
            Err(SendError::Transport)
        );
        assert_eq!(sent.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn bounded_send_times_out_without_starting_another_write() {
        let (mut sink, sent) = MockSink::new(SinkBehavior::Pending);
        assert_eq!(
            send_message(
                &mut sink,
                Message::Text("pending".into()),
                Duration::from_millis(10),
                &tokio_util::sync::CancellationToken::new(),
            )
            .await,
            Err(SendError::Timeout)
        );
        assert_eq!(sent.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn bounded_send_cancels_pending_write_without_waiting_for_timeout() {
        let (mut sink, sent) = MockSink::new(SinkBehavior::Pending);
        let shutdown = tokio_util::sync::CancellationToken::new();
        shutdown.cancel();
        let result = tokio::time::timeout(
            Duration::from_millis(100),
            send_message(
                &mut sink,
                Message::Text("pending".into()),
                Duration::from_mins(1),
                &shutdown,
            ),
        )
        .await;
        assert_eq!(result, Ok(Err(SendError::Cancelled)));
        assert_eq!(sent.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn gap_mapping_covers_expired_future_and_invalid() {
        let expired = map_gap(k::ChangeGap::Expired {
            requested_after: cursor(2),
            earliest_available: cursor(5),
            latest_available: cursor(9),
        });
        assert_eq!(expired.reason, p::CursorGapReason::Expired);
        assert_eq!(expired.requested_after.as_str(), "2");
        assert_eq!(expired.earliest_available.expect("earliest").as_str(), "5");
        assert_eq!(expired.latest_available.expect("latest").as_str(), "9");

        let future = map_gap(k::ChangeGap::Future {
            requested_after: cursor(10),
            latest_available: cursor(9),
        });
        assert_eq!(future.reason, p::CursorGapReason::Future);
        assert!(future.earliest_available.is_none());
        assert_eq!(future.latest_available.expect("latest").as_str(), "9");

        let invalid = invalid_gap(p::Cursor("not-a-cursor".into()));
        assert_eq!(invalid.reason, p::CursorGapReason::Invalid);
        assert_eq!(invalid.requested_after.as_str(), "not-a-cursor");
        assert!(invalid.earliest_available.is_none());
        assert!(invalid.latest_available.is_none());
    }

    #[test]
    fn replay_page_validation_accepts_only_contiguous_progress() {
        let valid = k::ChangePage::new(vec![event(2), event(3)], cursor(3), true);
        assert_eq!(
            validate_replay_page(cursor(1), &valid).expect("valid"),
            cursor(3)
        );
        let empty_final = k::ChangePage::new(vec![], cursor(3), false);
        assert_eq!(
            validate_replay_page(cursor(3), &empty_final).expect("empty final"),
            cursor(3)
        );

        let malformed = [
            k::ChangePage::new(vec![event(3)], cursor(3), false),
            k::ChangePage::new(vec![event(2), event(4)], cursor(4), false),
            k::ChangePage::new(vec![event(2)], cursor(1), false),
            k::ChangePage::new(vec![], cursor(1), true),
        ];
        for page in malformed {
            assert!(matches!(
                validate_replay_page(cursor(1), &page),
                Err(PrepareError::Internal)
            ));
        }
    }

    #[test]
    fn replay_page_validation_rejects_cursor_overflow() {
        let max = cursor(u64::MAX);
        let page = k::ChangePage::new(vec![event(1)], cursor(1), false);
        assert!(matches!(
            validate_replay_page(max, &page),
            Err(PrepareError::Internal)
        ));
    }

    #[test]
    fn secret_scan_preserves_normalization_suffixes_and_lease_token_exception() {
        for value in [
            json!({"PASSWORD": "value"}),
            json!({"provider-secret": "value"}),
            json!({"access token": "value"}),
            json!({"nested": {"service_api_key": "value"}}),
        ] {
            assert!(!safe_json(&value, 8, 16, true));
        }
        let lease = json!({"lease-token": "value"});
        assert!(safe_json(&lease, 8, 16, true));
        assert!(!safe_json(&lease, 8, 16, false));
        assert!(safe_json(
            &json!({"tokenized": "value", "key": "value"}),
            8,
            16,
            false
        ));
    }
}
