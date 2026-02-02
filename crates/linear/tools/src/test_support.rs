//! Test-only utilities for safely mutating process-global state in tests.
//!
//! # Usage
//!
//! ```rust,ignore
//! use linear_tools::test_support::EnvGuard;
//! use serial_test::serial;
//!
//! #[test]
//! #[serial(env)]
//! fn example() {
//!     let _env = EnvGuard::set("LINEAR_API_KEY", "test-key");
//!     // ... test body ...
//! }
//! ```
//!
//! # Important
//!
//! - All tests that use these guards MUST use `#[serial(env)]` to prevent concurrent
//!   execution and ensure process-global state mutations don't interfere with each other.

/// RAII guard for temporarily setting an environment variable.
///
/// The variable is automatically restored to its previous state (or removed if it
/// was not set) when the guard is dropped.
pub struct EnvGuard {
    key: &'static str,
    prev: Option<String>,
}

impl EnvGuard {
    /// Set an environment variable temporarily.
    ///
    /// The previous value (if any) is captured and will be restored when dropped.
    ///
    /// # Safety
    ///
    /// This function uses `unsafe` because `std::env::set_var` can cause data races
    /// if called concurrently with other environment variable operations. However,
    /// this is safe when used with `#[serial(env)]` which ensures no concurrent execution.
    #[must_use]
    pub fn set(key: &'static str, val: &str) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: Safe when used with #[serial(env)] which prevents concurrent access
        unsafe { std::env::set_var(key, val) };
        Self { key, prev }
    }

    /// Remove an environment variable temporarily.
    ///
    /// The previous value (if any) is captured and will be restored when dropped.
    ///
    /// # Safety
    ///
    /// This function uses `unsafe` because `std::env::remove_var` can cause data races
    /// if called concurrently with other environment variable operations. However,
    /// this is safe when used with `#[serial(env)]` which ensures no concurrent execution.
    #[must_use]
    pub fn remove(key: &'static str) -> Self {
        let prev = std::env::var(key).ok();
        // SAFETY: Safe when used with #[serial(env)] which prevents concurrent access
        unsafe { std::env::remove_var(key) };
        Self { key, prev }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.prev {
            // SAFETY: Safe in drop because test serialization is still active
            Some(v) => unsafe { std::env::set_var(self.key, v) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

// ============================================================================
// JSON fixture builders for integration tests
// ============================================================================

use serde_json::{Value, json};

pub fn user_node(id: &str, name: &str, display_name: &str, email: &str) -> Value {
    json!({ "id": id, "name": name, "displayName": display_name, "email": email })
}

pub fn team_node(id: &str, key: &str, name: &str) -> Value {
    json!({ "id": id, "key": key, "name": name })
}

pub fn workflow_state_node(id: &str, name: &str, state_type: &str) -> Value {
    json!({ "id": id, "name": name, "type": state_type })
}

pub fn project_node(id: &str, name: &str) -> Value {
    json!({ "id": id, "name": name })
}

pub fn parent_issue_node(id: &str, identifier: &str) -> Value {
    json!({ "id": id, "identifier": identifier })
}

/// Build a full issue node with sensible defaults. Override fields by mutating the returned Value.
pub fn issue_node(id: &str, identifier: &str, title: &str) -> Value {
    json!({
        "id": id,
        "identifier": identifier,
        "title": title,
        "description": null,
        "priority": 2.0,
        "priorityLabel": "High",
        "labelIds": [],
        "dueDate": null,
        "estimate": null,
        "parent": null,
        "startedAt": null,
        "completedAt": null,
        "canceledAt": null,
        "url": format!("https://linear.app/test/issue/{}", identifier),
        "createdAt": "2025-01-01T00:00:00Z",
        "updatedAt": "2025-01-02T00:00:00Z",
        "team": team_node("t1", "ENG", "Engineering"),
        "state": workflow_state_node("s1", "Todo", "unstarted"),
        "assignee": null,
        "creator": user_node("u0", "Creator", "Creator", "creator@example.com"),
        "project": null
    })
}

/// Build an issue node suitable for searchIssues responses (no details-only fields).
pub fn search_issue_node(id: &str, identifier: &str, title: &str) -> Value {
    json!({
        "id": id,
        "identifier": identifier,
        "title": title,
        "description": null,
        "priority": 2.0,
        "priorityLabel": "High",
        "labelIds": [],
        "dueDate": null,
        "url": format!("https://linear.app/test/issue/{}", identifier),
        "createdAt": "2025-01-01T00:00:00Z",
        "updatedAt": "2025-01-02T00:00:00Z",
        "team": team_node("t1", "ENG", "Engineering"),
        "state": workflow_state_node("s1", "Todo", "unstarted"),
        "assignee": null,
        "creator": user_node("u0", "Creator", "Creator", "creator@example.com"),
        "project": null
    })
}

pub fn issues_response(nodes: Vec<Value>, has_next_page: bool, end_cursor: Option<&str>) -> String {
    serde_json::to_string(&json!({
        "data": {
            "issues": {
                "nodes": nodes,
                "pageInfo": { "hasNextPage": has_next_page, "endCursor": end_cursor }
            }
        }
    }))
    .unwrap()
}

pub fn search_response(nodes: Vec<Value>, has_next_page: bool, end_cursor: Option<&str>) -> String {
    serde_json::to_string(&json!({
        "data": {
            "searchIssues": {
                "nodes": nodes,
                "pageInfo": { "hasNextPage": has_next_page, "endCursor": end_cursor }
            }
        }
    }))
    .unwrap()
}

pub fn issue_by_id_response(issue: Value) -> String {
    serde_json::to_string(&json!({
        "data": { "issue": issue }
    }))
    .unwrap()
}

pub fn issue_create_response(issue: Value) -> String {
    serde_json::to_string(&json!({
        "data": { "issueCreate": { "success": true, "issue": issue } }
    }))
    .unwrap()
}

pub fn comment_create_response(id: &str, body: &str) -> String {
    serde_json::to_string(&json!({
        "data": {
            "commentCreate": {
                "success": true,
                "comment": { "id": id, "body": body, "createdAt": "2025-01-01T00:00:00Z" }
            }
        }
    }))
    .unwrap()
}

pub fn archive_response(success: bool) -> String {
    serde_json::to_string(&json!({
        "data": { "issueArchive": { "success": success } }
    }))
    .unwrap()
}

pub fn users_response(nodes: Vec<Value>, has_next_page: bool, end_cursor: Option<&str>) -> String {
    serde_json::to_string(&json!({
        "data": {
            "users": {
                "nodes": nodes,
                "pageInfo": { "hasNextPage": has_next_page, "endCursor": end_cursor }
            }
        }
    }))
    .unwrap()
}

pub fn teams_response(nodes: Vec<Value>, has_next_page: bool, end_cursor: Option<&str>) -> String {
    serde_json::to_string(&json!({
        "data": {
            "teams": {
                "nodes": nodes,
                "pageInfo": { "hasNextPage": has_next_page, "endCursor": end_cursor }
            }
        }
    }))
    .unwrap()
}

pub fn projects_response(
    nodes: Vec<Value>,
    has_next_page: bool,
    end_cursor: Option<&str>,
) -> String {
    serde_json::to_string(&json!({
        "data": {
            "projects": {
                "nodes": nodes,
                "pageInfo": { "hasNextPage": has_next_page, "endCursor": end_cursor }
            }
        }
    }))
    .unwrap()
}

pub fn workflow_states_response(
    nodes: Vec<Value>,
    has_next_page: bool,
    end_cursor: Option<&str>,
) -> String {
    serde_json::to_string(&json!({
        "data": {
            "workflowStates": {
                "nodes": nodes,
                "pageInfo": { "hasNextPage": has_next_page, "endCursor": end_cursor }
            }
        }
    }))
    .unwrap()
}

pub fn issue_labels_response(
    nodes: Vec<Value>,
    has_next_page: bool,
    end_cursor: Option<&str>,
) -> String {
    serde_json::to_string(&json!({
        "data": {
            "issueLabels": {
                "nodes": nodes,
                "pageInfo": { "hasNextPage": has_next_page, "endCursor": end_cursor }
            }
        }
    }))
    .unwrap()
}

pub fn issue_label_node(id: &str, name: &str, team: Option<Value>) -> Value {
    json!({ "id": id, "name": name, "team": team })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial(env)]
    fn envguard_set_and_restore_when_unset() {
        let key = "LINEAR_TEST_ENVVAR_A";
        let _r = EnvGuard::remove(key);
        {
            let _g = EnvGuard::set(key, "123");
            assert_eq!(std::env::var(key).unwrap(), "123");
        }
        assert!(std::env::var(key).is_err(), "should restore to unset");
    }

    #[test]
    #[serial(env)]
    fn envguard_restore_previous_value() {
        let key = "LINEAR_TEST_ENVVAR_B";
        let _orig = EnvGuard::set(key, "orig");
        {
            let _g = EnvGuard::set(key, "shadow");
            assert_eq!(std::env::var(key).unwrap(), "shadow");
        }
        assert_eq!(std::env::var(key).unwrap(), "orig");
    }

    #[test]
    #[serial(env)]
    fn envguard_remove_and_restore() {
        let key = "LINEAR_TEST_ENVVAR_C";
        let _orig = EnvGuard::set(key, "value");
        {
            let _g = EnvGuard::remove(key);
            assert!(std::env::var(key).is_err());
        }
        assert_eq!(std::env::var(key).unwrap(), "value");
    }
}
