mod support;

use attention_kernel::*;
use chrono::Utc;
use futures::executor::block_on;
use support::MemoryAdapter;

#[test]
fn snapshots_pages_and_gaps_preserve_cursor_contracts() {
    let adapter = MemoryAdapter::new();
    let initial = block_on(adapter.snapshot()).expect("initial snapshot");
    for _ in 0..3 {
        let command = CreateWorkItem::new(
            WorkItemId::new(),
            None,
            None,
            None,
            None,
            MutationIdempotencyKey::new(),
        );
        let bundle = evaluate_create_work_item(
            &command,
            EvaluationContext::new(ChangeEventId::new(), None, Utc::now()),
        );
        block_on(adapter.commit_create_work_item(bundle)).expect("create");
    }
    let page = block_on(adapter.changes_after(ChangesAfterQuery::new(
        initial.cursor(),
        QueryLimit::try_from(2).expect("limit"),
    )))
    .expect("changes");
    let ChangesResult::Page(page) = page else {
        panic!("expected page");
    };
    assert_eq!(page.events().len(), 2);
    assert!(page.has_more());
    assert!(page.events()[0].cursor() < page.events()[1].cursor());

    adapter.set_retention_floor(page.resume_after());
    assert!(matches!(
        block_on(adapter.changes_after(ChangesAfterQuery::new(
            initial.cursor(),
            QueryLimit::try_from(2).expect("limit"),
        )))
        .expect("gap result"),
        ChangesResult::Gap(_)
    ));
}
