use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use chrono::Utc;
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn bundle() -> CreateWorkItemBundle {
    evaluate_create_work_item(
        &CreateWorkItem::new(
            WorkItemId::new(),
            None,
            None,
            None,
            None,
            MutationIdempotencyKey::new(),
        ),
        EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
    )
}

#[tokio::test]
async fn commit_between_snapshot_and_changes_is_strictly_after_snapshot_cursor() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config).await?;
    database.run_startup_migrations().await?;
    database.commit_create_work_item(bundle()).await?;
    let snapshot = database.snapshot().await?;
    let second = database.commit_create_work_item(bundle()).await?;
    let changes = database
        .changes_after(ChangesAfterQuery::new(
            snapshot.cursor(),
            QueryLimit::try_from(8)?,
        ))
        .await?;
    let ChangesResult::Page(page) = changes else {
        return Err("expected page".into());
    };
    assert_eq!(page.events().len(), 1);
    assert_eq!(page.events()[0].cursor(), second.cursor());
    assert!(page.events()[0].cursor() > snapshot.cursor());
    database.close().await?;
    Ok(())
}
