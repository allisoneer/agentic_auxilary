use attention_kernel::*;
use attention_turso::AttentionDatabase;
use attention_turso::Config;
use chrono::DateTime;
use chrono::Utc;
use std::error::Error;

type TestResult<T = ()> = Result<T, Box<dyn Error + Send + Sync>>;

fn at(value: &str) -> TestResult<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}

async fn create_scheduled(
    database: &AttentionDatabase,
    trigger_at: DateTime<Utc>,
) -> TestResult<(ReminderId, ReminderFireId, Revision)> {
    let reminder_id = ReminderId::new();
    let fire_id = ReminderFireId::new();
    database
        .commit_create_reminder(evaluate_create_reminder(
            &CreateReminder::new(
                reminder_id,
                fire_id,
                ReminderTarget::WorkItem(WorkItemId::new()),
                trigger_at,
                MutationIdempotencyKey::new(),
            ),
            EvaluationContext::new(ChangeEventId::new(), None, trigger_at),
        ))
        .await?;
    let revision = database
        .reminder(reminder_id)
        .await?
        .ok_or("created reminder missing")?
        .revision();
    Ok((reminder_id, fire_id, revision))
}

#[tokio::test]
async fn due_fires_decode_filter_globally_order_limit_and_survive_restart() -> TestResult {
    let root = tempfile::tempdir()?;
    let config = Config::new(root.path().join("database"), root.path().join("backups"))?;
    let database = AttentionDatabase::open(config).await?;
    database.run_startup_migrations().await?;

    let whole = create_scheduled(&database, at("2026-08-03T12:00:00Z")?).await?;
    let fractional = create_scheduled(&database, at("2026-08-03T12:00:00.5Z")?).await?;
    let tie_time = at("2026-08-03T12:00:01Z")?;
    let tie_a = create_scheduled(&database, tie_time).await?;
    let tie_b = create_scheduled(&database, tie_time).await?;
    let boundary = create_scheduled(&database, at("2026-08-03T12:00:02Z")?).await?;
    let _future = create_scheduled(&database, at("2026-08-03T12:00:03Z")?).await?;
    let fired = create_scheduled(&database, at("2026-08-03T11:59:59Z")?).await?;
    let reminder = database
        .reminder(fired.0)
        .await?
        .ok_or("fire fixture reminder missing")?;
    database
        .commit_fire_reminder(evaluate_fire_reminder(
            &FireReminder::new(fired.0, fired.1, MutationIdempotencyKey::new()),
            &reminder,
            EvaluationContext::new(ChangeEventId::new(), None, tie_time),
        )?)
        .await?;

    let query = DueReminderFiresQuery::new(at("2026-08-03T12:00:02Z")?, QueryLimit::try_from(16)?);
    let observed = database.due_reminder_fires(query).await?;
    let mut ties = [tie_a, tie_b];
    ties.sort_by_key(|value| (value.1, value.0));
    let expected = [whole, fractional, ties[0], ties[1], boundary];
    assert_eq!(observed.len(), expected.len());
    for (actual, expected) in observed.iter().zip(expected) {
        assert_eq!(actual.reminder_id(), expected.0);
        assert_eq!(actual.fire_id(), expected.1);
        assert_eq!(actual.reminder_revision(), expected.2);
    }

    let limited = database
        .due_reminder_fires(DueReminderFiresQuery::new(
            at("2026-08-03T12:00:02Z")?,
            QueryLimit::try_from(2)?,
        ))
        .await?;
    assert_eq!(
        limited
            .iter()
            .map(|fire| fire.fire_id())
            .collect::<Vec<_>>(),
        [whole.1, fractional.1]
    );

    database.close().await?;
    database.reopen().await?;
    assert_eq!(database.due_reminder_fires(query).await?, observed);
    database.close().await?;
    Ok(())
}
