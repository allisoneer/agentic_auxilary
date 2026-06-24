use anyhow::Context;
use anyhow::Result;
use opencode_rs::Client;
use opencode_rs::server::ManagedServer;
use opencode_rs::server::ServerOptions;
use opencode_rs::types::event::Event;
use opencode_rs::types::message::CommandRequest;
use opencode_rs::types::permission::PermissionReply;
use opencode_rs::types::permission::PermissionReplyRequest;
use opencode_rs::types::question::QuestionReply;
use opencode_rs::types::session::CreateSessionRequest;
use opencode_rs::types::session::SessionStatusInfo;
use opencode_rs::version;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

const IDLE_GRACE: Duration = Duration::from_millis(1000);
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const SESSION_DEADLINE: Duration = Duration::from_secs(3600);
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(300);

pub struct OpenCodeSupervisor {
    _managed_server: ManagedServer,
    client: Client,
    _directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupervisedOutcome {
    Completed {
        session_id: String,
    },
    PermissionRequired {
        session_id: String,
        request_id: String,
        permission_type: String,
    },
    QuestionRequired {
        session_id: String,
        request_id: String,
        prompt: String,
    },
    Failed {
        session_id: Option<String>,
        error: String,
    },
}

impl OpenCodeSupervisor {
    pub async fn start(directory: &Path) -> Result<Self> {
        let cwd = directory.canonicalize().with_context(|| {
            format!(
                "failed to canonicalize OpenCode directory {}",
                directory.display()
            )
        })?;
        let launcher_config = resolve_launcher_config(&cwd)
            .context("failed to resolve OpenCode launcher configuration")?;

        tracing::info!(
            binary = %launcher_config.binary,
            launcher_args = ?launcher_config.launcher_args,
            expected_version = %version::PINNED_OPENCODE_VERSION,
            "starting app-local opencode serve"
        );

        let managed = ManagedServer::start(
            ServerOptions::default()
                .binary(&launcher_config.binary)
                .launcher_args(launcher_config.launcher_args)
                .directory(cwd.clone()),
        )
        .await
        .context("failed to start embedded opencode serve")?;

        let base_url = managed.url().to_string().trim_end_matches('/').to_string();
        let client = Client::builder()
            .base_url(&base_url)
            .directory(cwd.to_string_lossy().to_string())
            .build()
            .context("failed to build opencode client")?;

        let health = client
            .misc()
            .health()
            .await
            .context("failed to fetch /global/health for version validation")?;
        version::validate_exact_version(health.version.as_deref()).with_context(|| {
            format!(
                "embedded OpenCode server did not match pinned stable v{} (binary={})",
                version::PINNED_OPENCODE_VERSION,
                launcher_config.binary
            )
        })?;

        Ok(Self {
            _managed_server: managed,
            client,
            _directory: cwd,
        })
    }

    pub async fn ensure_commands_present(&self, required: &[&str]) -> Result<()> {
        let commands = self
            .client
            .tools()
            .commands()
            .await
            .context("failed to list OpenCode commands")?;
        for required_name in required {
            if commands.iter().all(|command| {
                command.name != *required_name
                    && command.name.trim_start_matches('/') != required_name.trim_start_matches('/')
            }) {
                anyhow::bail!("required OpenCode command not found: {required_name}");
            }
        }
        Ok(())
    }

    pub async fn run_command_supervised(
        &self,
        existing_session_id: Option<&str>,
        command_name: &str,
        message: Option<&str>,
    ) -> Result<SupervisedOutcome> {
        let session_id = if let Some(session_id) = existing_session_id {
            self.client
                .sessions()
                .get(session_id)
                .await
                .with_context(|| format!("failed to load session {session_id}"))?;
            session_id.to_string()
        } else {
            self.client
                .sessions()
                .create(&CreateSessionRequest::default())
                .await
                .context("failed to create OpenCode session")?
                .id
        };

        if let Some(outcome) = self.preflight_pending_interruptions(&session_id).await? {
            return Ok(outcome);
        }

        let mut subscription = self
            .client
            .subscribe_session(&session_id)
            .context("failed to subscribe to session events")?;

        let cmd_client = self.client.clone();
        let dispatch_session_id = session_id.clone();
        let dispatch_command = command_name.to_string();
        let dispatch_message = message.unwrap_or_default().to_string();
        let mut command_task = Some(tokio::spawn(async move {
            let request = CommandRequest {
                command: dispatch_command,
                arguments: dispatch_message,
                message_id: None,
            };
            cmd_client
                .messages()
                .command(&dispatch_session_id, &request)
                .await
                .map(|_| ())
        }));

        let deadline = tokio::time::Instant::now() + SESSION_DEADLINE;
        let mut last_activity = tokio::time::Instant::now();
        let mut poll_interval = tokio::time::interval(POLL_INTERVAL);
        poll_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut observed_busy = false;
        let mut idle_grace_deadline: Option<tokio::time::Instant> = None;
        let mut awaiting_idle_grace = false;
        let mut sse_active = true;

        loop {
            let now = tokio::time::Instant::now();
            if now.duration_since(last_activity) >= INACTIVITY_TIMEOUT {
                return Ok(SupervisedOutcome::Failed {
                    session_id: Some(session_id.clone()),
                    error: "session idle timeout after 5 minutes".to_string(),
                });
            }
            if now >= deadline {
                return Ok(SupervisedOutcome::Failed {
                    session_id: Some(session_id.clone()),
                    error: "session execution timed out after 1 hour".to_string(),
                });
            }

            tokio::select! {
                maybe_event = subscription.recv(), if sse_active => {
                    let Some(event) = maybe_event else {
                        sse_active = false;
                        continue;
                    };

                    match event {
                        Event::PermissionAsked { properties } => {
                            return Ok(SupervisedOutcome::PermissionRequired {
                                session_id,
                                request_id: properties.request.id,
                                permission_type: properties.request.permission,
                            });
                        }
                        Event::QuestionAsked { properties } => {
                            let prompt = properties
                                .request
                                .questions
                                .first()
                                .map(|question| question.question.clone())
                                .unwrap_or_default();
                            return Ok(SupervisedOutcome::QuestionRequired {
                                session_id,
                                request_id: properties.request.id,
                                prompt,
                            });
                        }
                        Event::MessagePartDelta { .. }
                        | Event::MessagePartUpdated { .. }
                        | Event::MessageUpdated { .. } => {
                            last_activity = tokio::time::Instant::now();
                            observed_busy = true;
                            awaiting_idle_grace = false;
                        }
                        Event::SessionIdle { .. } => {
                            return Ok(SupervisedOutcome::Completed { session_id });
                        }
                        Event::SessionError { properties } => {
                            return Ok(SupervisedOutcome::Failed {
                                session_id: properties.session_id.or(Some(session_id)),
                                error: format!("session error: {:?}", properties.error),
                            });
                        }
                        _ => {}
                    }
                }
                _ = poll_interval.tick() => {
                    if let Some(outcome) = self.preflight_pending_interruptions(&session_id).await? {
                        return Ok(outcome);
                    }

                    match self.client.sessions().status_for(&session_id).await {
                        Ok(SessionStatusInfo::Busy | SessionStatusInfo::Retry { .. } | SessionStatusInfo::Unknown) => {
                            last_activity = tokio::time::Instant::now();
                            observed_busy = true;
                            awaiting_idle_grace = false;
                        }
                        Ok(SessionStatusInfo::Idle) => {
                            if observed_busy {
                                return Ok(SupervisedOutcome::Completed { session_id });
                            }

                            let grace = idle_grace_deadline.get_or_insert_with(|| tokio::time::Instant::now() + IDLE_GRACE);
                            if tokio::time::Instant::now() >= *grace {
                                return Ok(SupervisedOutcome::Completed { session_id });
                            }
                            awaiting_idle_grace = true;
                        }
                        Err(error) => {
                            tracing::warn!(error = %error, "failed to poll session status");
                        }
                    }
                }
                result = async {
                    match command_task.as_mut() {
                        Some(task) => Some(task.await),
                        None => std::future::pending::<Option<Result<Result<(), opencode_rs::OpencodeError>, tokio::task::JoinError>>>().await,
                    }
                }, if command_task.is_some() => {
                    match result {
                        Some(Ok(Ok(()))) => {
                            idle_grace_deadline = Some(tokio::time::Instant::now() + IDLE_GRACE);
                            command_task = None;
                        }
                        Some(Ok(Err(error))) => {
                            return Ok(SupervisedOutcome::Failed {
                                session_id: Some(session_id),
                                error: error.to_string(),
                            });
                        }
                        Some(Err(error)) => {
                            return Ok(SupervisedOutcome::Failed {
                                session_id: Some(session_id),
                                error: format!("command task failed: {error}"),
                            });
                        }
                        None => unreachable!("command task guard should prevent None"),
                    }
                }
                () = async {
                    match idle_grace_deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending::<()>().await,
                    }
                }, if awaiting_idle_grace => {
                    awaiting_idle_grace = false;
                    if matches!(self.client.sessions().status_for(&session_id).await, Ok(SessionStatusInfo::Idle)) {
                        return Ok(SupervisedOutcome::Completed { session_id });
                    }
                    observed_busy = true;
                    last_activity = tokio::time::Instant::now();
                }
            }
        }
    }

    pub async fn respond_permission(
        &self,
        _session_id: &str,
        request_id: &str,
        allow: bool,
    ) -> Result<()> {
        let reply = if allow {
            PermissionReply::Once
        } else {
            PermissionReply::Reject
        };

        self.client
            .permissions()
            .reply(
                request_id,
                &PermissionReplyRequest {
                    reply,
                    message: None,
                },
            )
            .await
            .with_context(|| format!("failed to respond to permission request {request_id}"))?;
        Ok(())
    }

    pub async fn respond_question(
        &self,
        _session_id: &str,
        request_id: &str,
        answer: &str,
    ) -> Result<()> {
        self.client
            .question()
            .reply(
                request_id,
                &QuestionReply {
                    answers: vec![vec![answer.to_string()]],
                },
            )
            .await
            .with_context(|| format!("failed to respond to question request {request_id}"))?;
        Ok(())
    }

    async fn preflight_pending_interruptions(
        &self,
        session_id: &str,
    ) -> Result<Option<SupervisedOutcome>> {
        let permissions = self
            .client
            .permissions()
            .list()
            .await
            .context("failed to list permissions")?;
        if let Some(permission) = permissions
            .into_iter()
            .find(|permission| permission.session_id == session_id)
        {
            return Ok(Some(SupervisedOutcome::PermissionRequired {
                session_id: session_id.to_string(),
                request_id: permission.id,
                permission_type: permission.permission,
            }));
        }

        let questions = self
            .client
            .question()
            .list()
            .await
            .context("failed to list questions")?;
        if let Some(question) = questions
            .into_iter()
            .find(|question| question.session_id == session_id)
        {
            return Ok(Some(SupervisedOutcome::QuestionRequired {
                session_id: session_id.to_string(),
                request_id: question.id,
                prompt: question
                    .questions
                    .first()
                    .map(|entry| entry.question.clone())
                    .unwrap_or_default(),
            }));
        }

        Ok(None)
    }
}

#[derive(Debug, Clone)]
struct LauncherConfig {
    binary: String,
    launcher_args: Vec<String>,
}

fn resolve_launcher_config(base_dir: &Path) -> Result<LauncherConfig> {
    let launcher_args = parse_launcher_args();
    if !launcher_args.is_empty() {
        let binary = std::env::var(version::OPENCODE_BINARY_ENV)
            .map_or_else(|_| "opencode".to_string(), |value| value.trim().to_string());
        if binary.is_empty() {
            anyhow::bail!(
                "OPENCODE_BINARY_ARGS is set but OPENCODE_BINARY is empty; set it to the launcher command"
            );
        }

        return Ok(LauncherConfig {
            binary,
            launcher_args,
        });
    }

    let binary = resolve_opencode_binary(base_dir)?;
    Ok(LauncherConfig {
        binary: binary.to_string_lossy().to_string(),
        launcher_args: Vec::new(),
    })
}

fn resolve_opencode_binary(base_dir: &Path) -> Result<PathBuf> {
    if let Ok(value) = std::env::var(version::OPENCODE_BINARY_ENV) {
        let value = value.trim();
        if !value.is_empty() {
            let path = PathBuf::from(value);
            return path.canonicalize().with_context(|| {
                format!("OPENCODE_BINARY points to missing path: {}", path.display())
            });
        }
    }

    let candidate = base_dir
        .join(".opencode")
        .join("bin")
        .join(format!("opencode-v{}", version::PINNED_OPENCODE_VERSION));
    if candidate.exists() {
        return candidate
            .canonicalize()
            .with_context(|| format!("failed to canonicalize {}", candidate.display()));
    }

    Ok(PathBuf::from("opencode"))
}

fn parse_launcher_args() -> Vec<String> {
    match std::env::var(version::OPENCODE_BINARY_ARGS_ENV) {
        Ok(value) => {
            let value = value.trim();
            if value.is_empty() {
                Vec::new()
            } else {
                value.split_whitespace().map(str::to_string).collect()
            }
        }
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Stdio;
    use tempfile::TempDir;
    use tokio::process::Command;
    use wiremock::Mock;
    use wiremock::MockServer;
    use wiremock::ResponseTemplate;
    use wiremock::matchers::method;
    use wiremock::matchers::path;

    #[tokio::test]
    async fn preflight_returns_pending_permission() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/permission"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "perm-1",
                    "sessionID": "session-1",
                    "permission": "file.write",
                    "patterns": ["src/**/*.rs"]
                }
            ])))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/question"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let outcome = supervisor
            .preflight_pending_interruptions("session-1")
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            Some(SupervisedOutcome::PermissionRequired { request_id, .. }) if request_id == "perm-1"
        ));
    }

    #[tokio::test]
    async fn preflight_returns_pending_question() {
        let mock = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/permission"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&mock)
            .await;
        Mock::given(method("GET"))
            .and(path("/question"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                {
                    "id": "question-1",
                    "sessionID": "session-2",
                    "questions": [{ "question": "Continue?" }]
                }
            ])))
            .mount(&mock)
            .await;

        let supervisor = test_supervisor(&mock, TempDir::new().unwrap().path());
        let outcome = supervisor
            .preflight_pending_interruptions("session-2")
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            Some(SupervisedOutcome::QuestionRequired { request_id, .. }) if request_id == "question-1"
        ));
    }

    fn test_supervisor(mock: &MockServer, directory: &Path) -> OpenCodeSupervisor {
        let child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let managed = ManagedServer::from_child_for_testing(child, mock.uri(), 1234);
        let client = Client::builder()
            .base_url(mock.uri())
            .directory(directory.display().to_string())
            .build()
            .unwrap();
        OpenCodeSupervisor {
            _managed_server: managed,
            client,
            _directory: directory.to_path_buf(),
        }
    }
}
