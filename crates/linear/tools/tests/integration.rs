use linear_tools::test_support::*;
use mockito::Server;
use serial_test::serial;

#[tokio::test]
#[serial(env)]
async fn search_issues_auth_failure() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .with_status(401)
        .with_body(r#"{"errors":[{"message":"Unauthorized"}]}"#)
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "bad-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .search_issues(
            None, None, None, None, None, None, None, None, None, None, None, None, None,
        )
        .await;
    assert!(res.is_err());
}

#[tokio::test]
#[serial(env)]
async fn read_issue_by_identifier_success() {
    let mut server = Server::new_async().await;

    let mut node = issue_node("uuid-1", "ENG-245", "Test Issue");
    node["description"] = serde_json::json!("Description here");
    node["state"] = workflow_state_node("s1", "In Progress", "started");

    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(vec![node], false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool.read_issue("ENG-245".into()).await.unwrap();
    assert_eq!(res.issue.identifier, "ENG-245");
    assert_eq!(res.issue.title, "Test Issue");
    assert_eq!(res.issue.state.as_ref().unwrap().name, "In Progress");
    assert_eq!(res.issue.state.as_ref().unwrap().state_type, "started");
    assert_eq!(res.issue.team.key, "ENG");
    assert_eq!(res.issue.priority, 2);
    assert_eq!(res.issue.priority_label, "High");
    assert!(res.issue.creator.is_some());
    assert_eq!(res.description, Some("Description here".to_string()));
}

#[tokio::test]
#[serial(env)]
async fn search_issues_success() {
    let mut server = Server::new_async().await;

    let mut node1 = issue_node("uuid-1", "ENG-100", "First Issue");
    node1["priority"] = serde_json::json!(1.0);
    node1["priorityLabel"] = serde_json::json!("Urgent");
    node1["assignee"] = user_node("u1", "Alice", "Alice Smith", "alice@example.com");

    let mut node2 = issue_node("uuid-2", "ENG-101", "Second Issue");
    node2["description"] = serde_json::json!("Some description");
    node2["updatedAt"] = serde_json::json!("2025-01-03T00:00:00Z");
    node2["state"] = workflow_state_node("s2", "In Progress", "started");
    node2["project"] = project_node("p1", "Q1 Goals");

    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(vec![node1, node2], true, Some("cursor123")))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .search_issues(
            None, None, None, None, None, None, None, None, None, None, None, None, None,
        )
        .await
        .unwrap();

    assert_eq!(res.issues.len(), 2);
    assert_eq!(res.issues[0].identifier, "ENG-100");
    assert_eq!(res.issues[0].assignee.as_ref().unwrap().name, "Alice Smith");
    assert_eq!(res.issues[0].priority, 1);
    assert_eq!(res.issues[0].priority_label, "Urgent");
    assert_eq!(res.issues[1].identifier, "ENG-101");
    assert_eq!(res.issues[1].project.as_ref().unwrap().name, "Q1 Goals");
    assert!(res.has_next_page);
    assert_eq!(res.end_cursor, Some("cursor123".to_string()));
}

#[tokio::test]
#[serial(env)]
async fn read_issue_not_found() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json; charset=utf-8")
        .with_body(issues_response(vec![], false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool.read_issue("NONEXISTENT-999".into()).await;
    assert!(res.is_err(), "Expected error for non-existent issue");
}

#[tokio::test]
#[serial(env)]
async fn create_issue_success() {
    let mut server = Server::new_async().await;

    let mut node = issue_node("new-uuid", "ENG-500", "New Issue");
    node["description"] = serde_json::json!("Issue description");
    node["state"] = workflow_state_node("s1", "Backlog", "backlog");

    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issue_create_response(node))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .create_issue(
            "team-uuid".to_string(),
            "New Issue".to_string(),
            Some("Issue description".to_string()),
            Some(2),
            None,
            None,
            None,
            None,
            vec![],
        )
        .await
        .unwrap();

    assert!(res.success);
    assert!(res.issue.is_some());
    let issue = res.issue.unwrap();
    assert_eq!(issue.identifier, "ENG-500");
    assert_eq!(issue.title, "New Issue");
}

#[tokio::test]
#[serial(env)]
async fn add_comment_success() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(comment_create_response(
            "comment-uuid",
            "This is a test comment",
        ))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .add_comment(
            "issue-uuid".to_string(),
            "This is a test comment".to_string(),
            None,
        )
        .await
        .unwrap();

    assert!(res.success);
    assert_eq!(res.comment_id, Some("comment-uuid".to_string()));
    assert_eq!(res.body, Some("This is a test comment".to_string()));
}

#[tokio::test]
#[serial(env)]
async fn add_comment_by_identifier_success() {
    let mut server = Server::new_async().await;

    // First request: resolve identifier ENG-245 to UUID
    let mut resolve_node = issue_node("resolved-uuid", "ENG-245", "Test Issue");
    resolve_node["state"] = serde_json::json!(null);

    let _m1 = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(vec![resolve_node], false, None))
        .expect(1)
        .create_async()
        .await;

    // Second request: create comment
    let _m2 = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(comment_create_response(
            "comment-uuid",
            "Comment via identifier",
        ))
        .expect(1)
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .add_comment(
            "ENG-245".to_string(),
            "Comment via identifier".to_string(),
            None,
        )
        .await
        .unwrap();

    assert!(res.success);
    assert_eq!(res.comment_id, Some("comment-uuid".to_string()));
    assert_eq!(res.body, Some("Comment via identifier".to_string()));
}

#[tokio::test]
#[serial(env)]
async fn add_comment_by_url_success() {
    let mut server = Server::new_async().await;

    // First request: resolve identifier from URL to UUID
    let mut resolve_node = issue_node("url-resolved-uuid", "ENG-100", "URL Test Issue");
    resolve_node["state"] = serde_json::json!(null);

    let _m1 = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(vec![resolve_node], false, None))
        .expect(1)
        .create_async()
        .await;

    // Second request: create comment
    let _m2 = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(comment_create_response(
            "url-comment-uuid",
            "Comment via URL",
        ))
        .expect(1)
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .add_comment(
            "https://linear.app/test/issue/ENG-100/some-slug".to_string(),
            "Comment via URL".to_string(),
            None,
        )
        .await
        .unwrap();

    assert!(res.success);
    assert_eq!(res.comment_id, Some("url-comment-uuid".to_string()));
    assert_eq!(res.body, Some("Comment via URL".to_string()));
}

#[tokio::test]
#[serial(env)]
async fn create_issue_with_state_label_parent_success() {
    let mut server = Server::new_async().await;

    let mut node = issue_node("new-uuid-with-extras", "ENG-600", "Issue with extras");
    node["description"] = serde_json::json!("Has state, labels, parent");
    node["state"] = workflow_state_node("state-1", "In Progress", "started");

    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issue_create_response(node))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .create_issue(
            "team-uuid".to_string(),
            "Issue with extras".to_string(),
            Some("Has state, labels, parent".to_string()),
            Some(2),
            None,
            None,
            Some("state-1".to_string()),
            Some("parent-issue-uuid".to_string()),
            vec!["label-1".to_string(), "label-2".to_string()],
        )
        .await
        .unwrap();

    assert!(res.success);
    assert!(res.issue.is_some());
    let issue = res.issue.unwrap();
    assert_eq!(issue.identifier, "ENG-600");
    assert_eq!(issue.title, "Issue with extras");
    assert_eq!(issue.state.as_ref().unwrap().name, "In Progress");
}

#[tokio::test]
#[serial(env)]
async fn search_issues_with_state_filter() {
    let mut server = Server::new_async().await;

    let mut node = issue_node("uuid-filtered", "ENG-200", "Filtered Issue");
    node["state"] = workflow_state_node("state-in-progress", "In Progress", "started");

    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(vec![node], false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .search_issues(
            None,
            None,
            None,
            Some("state-in-progress".to_string()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(res.issues.len(), 1);
    assert_eq!(res.issues[0].identifier, "ENG-200");
    assert_eq!(res.issues[0].state.as_ref().unwrap().name, "In Progress");
}

#[tokio::test]
#[serial(env)]
async fn search_issues_with_date_ranges() {
    let mut server = Server::new_async().await;

    let mut node = issue_node("uuid-recent", "ENG-300", "Recent Issue");
    node["priority"] = serde_json::json!(3.0);
    node["priorityLabel"] = serde_json::json!("Normal");
    node["createdAt"] = serde_json::json!("2025-01-15T00:00:00Z");
    node["updatedAt"] = serde_json::json!("2025-01-16T00:00:00Z");

    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(vec![node], false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .search_issues(
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some("2025-01-01T00:00:00Z".to_string()),
            Some("2025-01-31T23:59:59Z".to_string()),
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(res.issues.len(), 1);
    assert_eq!(res.issues[0].identifier, "ENG-300");
    assert_eq!(res.issues[0].title, "Recent Issue");
}

#[tokio::test]
#[serial(env)]
async fn auth_header_personal_key() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .match_header("authorization", "lin_api_abc123")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(vec![], false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "lin_api_abc123");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .search_issues(
            None, None, None, None, None, None, None, None, None, None, None, None, None,
        )
        .await
        .unwrap();
    assert_eq!(res.issues.len(), 0);
}

#[tokio::test]
#[serial(env)]
async fn auth_header_oauth_token() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .match_header("authorization", "Bearer oauth_token_123")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(vec![], false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "oauth_token_123");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .search_issues(
            None, None, None, None, None, None, None, None, None, None, None, None, None,
        )
        .await
        .unwrap();
    assert_eq!(res.issues.len(), 0);
}

#[tokio::test]
#[serial(env)]
async fn search_issues_full_text_success() {
    let mut server = Server::new_async().await;

    let mut node = search_issue_node("uuid-s1", "ENG-777", "Found via full-text search");
    node["description"] = serde_json::json!("This issue contains the search term");
    node["state"] = workflow_state_node("s1", "In Progress", "started");

    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(search_response(vec![node], false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .search_issues(
            Some("search term".to_string()),
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(10),
            None,
        )
        .await
        .unwrap();

    assert_eq!(res.issues.len(), 1);
    assert_eq!(res.issues[0].identifier, "ENG-777");
    assert_eq!(res.issues[0].title, "Found via full-text search");
    assert_eq!(res.issues[0].state.as_ref().unwrap().name, "In Progress");
}

#[tokio::test]
#[serial(env)]
async fn graphql_errors_fail_fast() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"{"errors":[{"message":"Some GraphQL error"}]}"#)
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .search_issues(
            Some("anything".into()),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(1),
            None,
        )
        .await;
    assert!(res.is_err(), "Expected failure on GraphQL errors");
    let err = res.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("GraphQL errors"),
        "Error message should contain 'GraphQL errors': {}",
        err_msg
    );
}

// ============================================================================
// Archive issue tests
// ============================================================================

#[tokio::test]
#[serial(env)]
async fn archive_issue_success_by_id() {
    let mut server = Server::new_async().await;
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(archive_response(true))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool.archive_issue("some-uuid".to_string()).await.unwrap();
    assert!(res.success);
}

#[tokio::test]
#[serial(env)]
async fn archive_issue_success_by_identifier() {
    let mut server = Server::new_async().await;

    // First request: resolve identifier to UUID
    let mut resolve_node = issue_node("resolved-uuid", "ENG-245", "Test Issue");
    resolve_node["state"] = serde_json::json!(null);

    let _m1 = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issues_response(vec![resolve_node], false, None))
        .expect(1)
        .create_async()
        .await;

    // Second request: archive
    let _m2 = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(archive_response(true))
        .expect(1)
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool.archive_issue("ENG-245".to_string()).await.unwrap();
    assert!(res.success);
}

// ============================================================================
// Get metadata tests
// ============================================================================

#[tokio::test]
#[serial(env)]
async fn get_metadata_users_success() {
    let mut server = Server::new_async().await;
    let nodes = vec![
        user_node("u1", "Alice", "Alice Smith", "alice@example.com"),
        user_node("u2", "Bob", "Bob Jones", "bob@example.com"),
    ];
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(users_response(nodes, false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .get_metadata(
            linear_tools::models::MetadataKind::Users,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(res.items.len(), 2);
    assert_eq!(res.items[0].name, "Alice Smith");
    assert_eq!(res.items[0].email, Some("alice@example.com".to_string()));
    assert!(!res.has_next_page);
}

#[tokio::test]
#[serial(env)]
async fn get_metadata_teams_success() {
    let mut server = Server::new_async().await;
    let nodes = vec![
        team_node("t1", "ENG", "Engineering"),
        team_node("t2", "DES", "Design"),
    ];
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(teams_response(nodes, false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .get_metadata(
            linear_tools::models::MetadataKind::Teams,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(res.items.len(), 2);
    assert_eq!(res.items[0].name, "Engineering");
    assert_eq!(res.items[0].key, Some("ENG".to_string()));
}

#[tokio::test]
#[serial(env)]
async fn get_metadata_labels_success() {
    let mut server = Server::new_async().await;
    let nodes = vec![
        issue_label_node("l1", "Bug", Some(team_node("t1", "ENG", "Engineering"))),
        issue_label_node("l2", "Feature", None),
    ];
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(issue_labels_response(nodes, false, None))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .get_metadata(
            linear_tools::models::MetadataKind::Labels,
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();

    assert_eq!(res.items.len(), 2);
    assert_eq!(res.items[0].name, "Bug");
    assert_eq!(res.items[0].team_id, Some("t1".to_string()));
    assert_eq!(res.items[1].name, "Feature");
    assert_eq!(res.items[1].team_id, None);
}

#[tokio::test]
#[serial(env)]
async fn get_metadata_pagination() {
    let mut server = Server::new_async().await;
    let nodes = vec![team_node("t1", "ENG", "Engineering")];
    let _m = server
        .mock("POST", "/")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(teams_response(nodes, true, Some("cursor-abc")))
        .create_async()
        .await;

    let _url = EnvGuard::set("LINEAR_GRAPHQL_URL", &server.url());
    let _key = EnvGuard::set("LINEAR_API_KEY", "good-key");

    let tool = linear_tools::LinearTools::new();
    let res = tool
        .get_metadata(
            linear_tools::models::MetadataKind::Teams,
            None,
            None,
            Some(1),
            None,
        )
        .await
        .unwrap();

    assert_eq!(res.items.len(), 1);
    assert!(res.has_next_page);
    assert_eq!(res.end_cursor, Some("cursor-abc".to_string()));
}
