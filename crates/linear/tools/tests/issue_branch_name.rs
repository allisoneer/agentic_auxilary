use linear_tools::test_support::EnvGuard;
use linear_tools::test_support::issue_by_id_response;
use linear_tools::test_support::issues_response;
use mockito::Matcher;
use mockito::Server;
use serde_json::json;
use serial_test::serial;

#[tokio::test]
#[serial(env)]
async fn read_issue_branch_name_by_identifier_returns_branch() {
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("POST", "/graphql")
        .match_body(Matcher::Regex("branchName".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(
            &[json!({
                "id": "uuid-1",
                "identifier": "ENG-123",
                "branchName": "feature/eng-123"
            })],
            false,
            None,
        ))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &format!("{}/graphql", server.url()));
    let _key = EnvGuard::set("LINEAR_API_KEY", "test");

    let tool = linear_tools::LinearTools::new();
    let branch = tool
        .read_issue_branch_name("ENG-123".to_string())
        .await
        .unwrap();

    assert_eq!(branch, "feature/eng-123");
}

#[tokio::test]
#[serial(env)]
async fn read_issue_branch_name_by_id_returns_branch() {
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("POST", "/graphql")
        .match_body(Matcher::Regex("branchName".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issue_by_id_response(&json!({
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "identifier": "ENG-123",
            "branchName": "feature/eng-123"
        })))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &format!("{}/graphql", server.url()));
    let _key = EnvGuard::set("LINEAR_API_KEY", "test");

    let tool = linear_tools::LinearTools::new();
    let branch = tool
        .read_issue_branch_name("550e8400-e29b-41d4-a716-446655440000".to_string())
        .await
        .unwrap();

    assert_eq!(branch, "feature/eng-123");
}

#[tokio::test]
#[serial(env)]
async fn read_issue_branch_name_identifier_not_found_errors() {
    let mut server = Server::new_async().await;
    let _mock = server
        .mock("POST", "/graphql")
        .match_body(Matcher::Regex("branchName".to_string()))
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(&[], false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &format!("{}/graphql", server.url()));
    let _key = EnvGuard::set("LINEAR_API_KEY", "test");

    let tool = linear_tools::LinearTools::new();
    let err = tool
        .read_issue_branch_name("ENG-123".to_string())
        .await
        .expect_err("missing issue should error");

    assert!(
        err.to_string()
            .contains("not found: Issue ENG-123 not found")
    );
}
