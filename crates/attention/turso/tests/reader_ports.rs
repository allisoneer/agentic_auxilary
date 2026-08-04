use attention_kernel::AttentionReadPort;
use attention_kernel::AttentionSignalId;
use attention_kernel::ChangeGap;
use attention_kernel::ChangesAfterQuery;
use attention_kernel::ChangesResult;
use attention_kernel::CommitCursor;
use attention_kernel::ExternalEntityId;
use attention_kernel::MutationIdempotencyKey;
use attention_kernel::PortError;
use attention_kernel::PriorOutcomeQuery;
use attention_kernel::QueryLimit;
use attention_kernel::ReminderFireId;
use attention_kernel::ReminderFireState;
use attention_kernel::ReminderId;
use attention_kernel::SourceAuthorityQuery;
use attention_kernel::SourceEntityId;
use attention_kernel::SourceEntityKey;
use attention_kernel::SourceInstance;
use attention_kernel::SourceKind;
use attention_kernel::SourceReceiptId;
use attention_kernel::WorkItemId;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use attention_turso::Error;
use std::error::Error as StdError;
use turso_db::Builder;
use turso_db::params;

type TestResult<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;

struct Fixture {
    _root: tempfile::TempDir,
    config: Config,
    work_item: WorkItemId,
    signal: AttentionSignalId,
    reminder: ReminderId,
    fire: ReminderFireId,
    second_reminder: ReminderId,
    second_fires: [ReminderFireId; 3],
    receipt: SourceReceiptId,
    entity_key: SourceEntityKey,
    mutation: MutationIdempotencyKey,
}

async fn fixture() -> TestResult<Fixture> {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config.clone()).await?;
    database.run_startup_migrations().await?;
    database.close().await?;

    let work_item = WorkItemId::new();
    let signal = AttentionSignalId::new();
    let reminder = ReminderId::new();
    let fire = ReminderFireId::new();
    let second_reminder = ReminderId::new();
    let second_fires = [
        ReminderFireId::new(),
        ReminderFireId::new(),
        ReminderFireId::new(),
    ];
    let receipt = SourceReceiptId::new();
    let entity = SourceEntityId::new();
    let mutation = MutationIdempotencyKey::new();
    let entity_key = SourceEntityKey::new(
        SourceKind::new("linear")?,
        SourceInstance::new("workspace")?,
        ExternalEntityId::new("ENG-1120")?,
    );
    let database_file = config.database_directory().database_file();
    let path = database_file.to_str().ok_or("database path is not UTF-8")?;
    let raw = Builder::new_local(path).build().await?;
    let connection = raw.connect()?;
    connection.execute("PRAGMA foreign_keys = ON", ()).await?;
    let one = 1_u64.to_be_bytes().to_vec();
    let time = "2026-08-03T12:34:56.123456789Z";
    connection
        .execute(
            "INSERT INTO mutation_outcomes (mutation_key, operation, fingerprint, \
             outcome_version, outcome_bytes) VALUES (?1, 0, ?2, 99, ?3)",
            params![mutation.to_string(), vec![7_u8; 32], b"sensitive".to_vec()],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO source_receipts (id, source_kind, source_instance, occurrence_id, \
             entity_source_kind, entity_source_instance, external_entity_id, fingerprint, \
             order_mode, order_domain, order_value, occurred_at, ingested_at, \
             accepted_mutation_key) VALUES (?1, 'linear', 'workspace', 'occurrence-1', 'linear', \
             'workspace', 'ENG-1120', ?2, 0, NULL, NULL, ?3, ?3, ?4)",
            params![
                receipt.to_string(),
                vec![7_u8; 32],
                time,
                mutation.to_string()
            ],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO source_entities (id, source_kind, source_instance, external_entity_id, \
             state_version, latest_receipt_id, order_mode, order_domain, order_value) VALUES \
             (?1, 'linear', 'workspace', 'ENG-1120', ?2, ?3, 0, NULL, NULL)",
            params![entity.to_string(), one.clone(), receipt.to_string()],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO work_items (id, revision, lifecycle, due_at, scheduled_at, defer_until, \
             source_kind, source_instance, external_entity_id) VALUES \
             (?1, ?2, 0, ?3, NULL, NULL, NULL, NULL, NULL)",
            params![work_item.to_string(), one.clone(), time],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO attention_signals VALUES (?1, ?2, 0, 0, ?3, ?4)",
            params![
                signal.to_string(),
                one.clone(),
                receipt.to_string(),
                entity.to_string()
            ],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO reminders (id, revision, target_kind, target_id, trigger_at, \
             current_fire_id) VALUES (?1, ?2, 0, ?3, ?4, NULL)",
            params![
                reminder.to_string(),
                one.clone(),
                work_item.to_string(),
                time
            ],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO reminder_fires (id, reminder_id, ordinal, trigger_at, state) \
             VALUES (?1, ?2, 0, ?3, 0)",
            params![fire.to_string(), reminder.to_string(), time],
        )
        .await?;
    connection
        .execute(
            "UPDATE reminders SET current_fire_id = ?1 WHERE id = ?2",
            params![fire.to_string(), reminder.to_string()],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO reminders (id, revision, target_kind, target_id, trigger_at, \
             current_fire_id) VALUES (?1, ?2, 1, ?3, ?4, NULL)",
            params![second_reminder.to_string(), one, signal.to_string(), time],
        )
        .await?;
    for (ordinal, fire, state) in [
        (0, second_fires[0], 2),
        (1, second_fires[1], 3),
        (2, second_fires[2], 0),
    ] {
        connection
            .execute(
                "INSERT INTO reminder_fires (id, reminder_id, ordinal, trigger_at, state) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    fire.to_string(),
                    second_reminder.to_string(),
                    ordinal,
                    time,
                    state
                ],
            )
            .await?;
    }
    connection
        .execute(
            "UPDATE reminders SET current_fire_id = ?1 WHERE id = ?2",
            params![second_fires[2].to_string(), second_reminder.to_string()],
        )
        .await?;
    drop(connection);
    drop(raw);
    Ok(Fixture {
        _root: root,
        config,
        work_item,
        signal,
        reminder,
        fire,
        second_reminder,
        second_fires,
        receipt,
        entity_key,
        mutation,
    })
}

#[tokio::test]
async fn all_point_reads_and_snapshot_reconstruct_validated_roots() -> TestResult {
    let fixture = fixture().await?;
    let database = AttentionDatabase::open(fixture.config).await?;
    assert_eq!(
        database
            .work_item(fixture.work_item)
            .await?
            .expect("work")
            .id(),
        fixture.work_item
    );
    assert!(database.work_item(WorkItemId::new()).await?.is_none());
    assert_eq!(
        database
            .attention_signal(fixture.signal)
            .await?
            .expect("signal")
            .id(),
        fixture.signal
    );
    let reminder = database
        .reminder(fixture.reminder)
        .await?
        .expect("reminder");
    assert_eq!(reminder.id(), fixture.reminder);
    assert_eq!(reminder.fires()[0].id(), fixture.fire);
    assert_eq!(
        database
            .source_receipt(fixture.receipt)
            .await?
            .expect("receipt")
            .id(),
        fixture.receipt
    );
    assert!(
        database
            .source_entity(SourceAuthorityQuery::new(fixture.entity_key))
            .await?
            .is_some()
    );
    let snapshot = database.snapshot().await?;
    assert_eq!(snapshot.cursor(), CommitCursor::try_from(1)?);
    assert_eq!(snapshot.work_items().len(), 1);
    assert_eq!(snapshot.signals().len(), 1);
    assert_eq!(snapshot.reminders().len(), 2);
    let first = snapshot
        .reminders()
        .iter()
        .find(|reminder| reminder.id() == fixture.reminder)
        .expect("first reminder");
    assert_eq!(first.fires()[0].id(), fixture.fire);
    let second = snapshot
        .reminders()
        .iter()
        .find(|reminder| reminder.id() == fixture.second_reminder)
        .expect("second reminder");
    assert_eq!(
        second
            .fires()
            .iter()
            .map(attention_kernel::ReminderFire::id)
            .collect::<Vec<_>>(),
        fixture.second_fires
    );
    assert_eq!(
        second
            .fires()
            .iter()
            .find(|fire| {
                matches!(
                    fire.state(),
                    ReminderFireState::Scheduled | ReminderFireState::Fired
                )
            })
            .map(attention_kernel::ReminderFire::id),
        Some(fixture.second_fires[2])
    );

    let error = database
        .prior_outcome(PriorOutcomeQuery::new(fixture.mutation))
        .await
        .expect_err("unknown version must fail closed");
    assert!(matches!(
        error,
        PortError::Adapter(Error::UnsupportedCodec { .. })
    ));
    assert!(!error.to_string().contains("sensitive"));
    database.close().await?;
    Ok(())
}

#[tokio::test]
async fn genesis_future_expired_and_floor_boundaries_are_explicit() -> TestResult {
    let fixture = fixture().await?;
    let database = AttentionDatabase::open(fixture.config.clone()).await?;
    let genesis = CommitCursor::try_from(1)?;
    let result = database
        .changes_after(ChangesAfterQuery::new(genesis, QueryLimit::try_from(2)?))
        .await?;
    assert!(matches!(result, ChangesResult::Page(ref page) if page.events().is_empty()));
    let future = CommitCursor::try_from(2)?;
    assert_eq!(
        database
            .changes_after(ChangesAfterQuery::new(future, QueryLimit::try_from(2)?))
            .await?,
        ChangesResult::Gap(ChangeGap::Future {
            requested_after: future,
            latest_available: genesis,
        })
    );
    database.close().await?;

    let database_file = fixture.config.database_directory().database_file();
    let path = database_file.to_str().ok_or("database path is not UTF-8")?;
    let raw = Builder::new_local(path).build().await?;
    let connection = raw.connect()?;
    connection
        .execute(
            "UPDATE attention_stream_state SET head_cursor = ?1, floor_cursor = ?1 WHERE singleton = 1",
            params![2_u64.to_be_bytes().to_vec()],
        )
        .await?;
    drop(connection);
    drop(raw);

    let database = AttentionDatabase::open(fixture.config).await?;
    assert_eq!(
        database
            .changes_after(ChangesAfterQuery::new(genesis, QueryLimit::try_from(2)?))
            .await?,
        ChangesResult::Gap(ChangeGap::Expired {
            requested_after: genesis,
            earliest_available: future,
            latest_available: future,
        })
    );
    assert!(matches!(
        database
            .changes_after(ChangesAfterQuery::new(future, QueryLimit::try_from(2)?))
            .await?,
        ChangesResult::Page(ref page) if page.events().is_empty() && page.resume_after() == future
    ));
    database.close().await?;
    Ok(())
}

#[tokio::test]
async fn malformed_roots_and_unknown_event_versions_fail_complete_reads_closed() -> TestResult {
    let fixture = fixture().await?;
    let database_file = fixture.config.database_directory().database_file();
    let path = database_file.to_str().ok_or("database path is not UTF-8")?;
    let raw = Builder::new_local(path).build().await?;
    let connection = raw.connect()?;
    connection
        .execute(
            "UPDATE work_items SET revision = ?1 WHERE id = ?2",
            params![vec![0_u8; 8], fixture.work_item.to_string()],
        )
        .await?;
    drop(connection);
    drop(raw);

    let database = AttentionDatabase::open(fixture.config.clone()).await?;
    assert!(matches!(
        database.work_item(fixture.work_item).await,
        Err(PortError::Adapter(Error::Decode(_)))
    ));
    database.close().await?;

    let raw = Builder::new_local(path).build().await?;
    let connection = raw.connect()?;
    connection
        .execute(
            "UPDATE work_items SET revision = ?1 WHERE id = ?2",
            params![1_u64.to_be_bytes().to_vec(), fixture.work_item.to_string()],
        )
        .await?;
    connection
        .execute(
            "INSERT INTO change_events VALUES (?1, ?2, ?3, 0, 99, ?4)",
            params![
                2_u64.to_be_bytes().to_vec(),
                attention_kernel::ChangeEventId::new().to_string(),
                "2026-08-03T12:34:56.123456789Z",
                b"sensitive event bytes".to_vec()
            ],
        )
        .await?;
    connection
        .execute(
            "UPDATE attention_stream_state SET head_cursor = ?1 WHERE singleton = 1",
            params![2_u64.to_be_bytes().to_vec()],
        )
        .await?;
    drop(connection);
    drop(raw);

    let database = AttentionDatabase::open(fixture.config).await?;
    let error = database
        .changes_after(ChangesAfterQuery::new(
            CommitCursor::try_from(1)?,
            QueryLimit::try_from(2)?,
        ))
        .await
        .expect_err("unknown event version must fail closed");
    assert!(matches!(
        error,
        PortError::Adapter(Error::UnsupportedCodec { .. })
    ));
    assert!(!error.to_string().contains("sensitive"));
    database.close().await?;
    Ok(())
}
