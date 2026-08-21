use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use attention_turso::PersistentServerIdentity;
use chrono::Utc;
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

async fn database() -> TestResult<(tempfile::TempDir, AttentionDatabase)> {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config).await?;
    database.run_startup_migrations().await?;
    Ok((root, database))
}

#[tokio::test]
async fn replay_metadata_and_frozen_event_lookup_survive_restart() -> TestResult {
    let (_root, database) = database().await?;
    let command = CreateWorkItem::new(
        WorkItemId::new(),
        None,
        None,
        None,
        None,
        MutationIdempotencyKey::new(),
    );
    let fingerprint = command.canonical_fingerprint();
    let event_id = ChangeEventId::new();
    let bundle =
        evaluate_create_work_item(&command, EvaluationContext::new(event_id, None, Utc::now()));
    let frozen_root = bundle.root().clone();
    let created = database.commit_create_work_item(bundle).await?;

    let record = database
        .prior_mutation(PriorOutcomeQuery::new(command.idempotency_key()))
        .await?
        .expect("stored mutation record");
    assert_eq!(record.operation(), MutationOperation::CreateWorkItem);
    assert_eq!(record.fingerprint(), fingerprint);
    assert_eq!(
        record.outcome(),
        &PriorMutationOutcome::CreateWorkItem(created.clone())
    );
    assert!(
        database
            .prior_mutation(PriorOutcomeQuery::new(MutationIdempotencyKey::new()))
            .await?
            .is_none()
    );

    let current = database.work_item(command.id()).await?.expect("work item");
    database
        .commit_complete_work_item(evaluate_complete_work_item(
            &CompleteWorkItem::new(
                current.id(),
                current.revision(),
                MutationIdempotencyKey::new(),
            ),
            &current,
            EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
        )?)
        .await?;
    let historical = database.change_event(event_id).await?.expect("event");
    assert_eq!(historical.cursor(), created.cursor());
    assert_eq!(historical.draft().id(), event_id);
    assert_eq!(
        historical.draft().affected_views(),
        &[AffectedView::WorkItem {
            work_item: frozen_root.clone()
        }]
    );
    assert!(database.change_event(ChangeEventId::new()).await?.is_none());

    database.close().await?;
    database.reopen().await?;
    assert_eq!(
        database
            .change_event(event_id)
            .await?
            .expect("restarted event")
            .draft()
            .affected_views(),
        &[AffectedView::WorkItem {
            work_item: frozen_root
        }]
    );
    assert_eq!(
        database
            .prior_mutation(PriorOutcomeQuery::new(command.idempotency_key()))
            .await?
            .expect("restarted record")
            .fingerprint(),
        fingerprint
    );
    database.close().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_identity_initialization_has_one_durable_winner() -> TestResult {
    let (_root, database) = database().await?;
    let first_candidate = PersistentServerIdentity::generate();
    let second_candidate = PersistentServerIdentity::generate();
    let first_database = database.clone();
    let second_database = database.clone();
    let first = tokio::spawn(async move {
        first_database
            .load_or_create_server_identity(first_candidate)
            .await
    });
    let second = tokio::spawn(async move {
        second_database
            .load_or_create_server_identity(second_candidate)
            .await
    });
    let first = first.await??;
    let second = second.await??;
    assert_eq!(first, second);
    assert!(first == first_candidate || first == second_candidate);

    database.close().await?;
    database.reopen().await?;
    assert_eq!(
        database
            .load_or_create_server_identity(PersistentServerIdentity::generate())
            .await?,
        first
    );
    assert_eq!(database.run_startup_migrations().await?.applied(), 0);
    database.close().await?;
    Ok(())
}
