//! Stateful pagination helpers for `just_execute` transcripts.

use agentic_tools_utils::pagination::PaginationCache;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::Mutex as AsyncMutex;

#[cfg(test)]
pub const EXECUTE_PAGE_LINES: usize = 200;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TranscriptPage {
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Default)]
pub struct ExecuteMeta {
    pub dir: String,
    pub recipe: String,
    pub success: bool,
    pub exit_code: Option<i32>,
}

pub type ExecuteTranscriptCache = PaginationCache<TranscriptPage, ExecuteMeta>;

#[derive(Default)]
pub struct SingleFlight {
    map: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl SingleFlight {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_map(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<AsyncMutex<()>>>> {
        match self.map.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub fn get_or_create(&self, key: &str) -> Arc<AsyncMutex<()>> {
        let mut map = self.lock_map();
        Arc::clone(
            map.entry(key.to_string())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    pub fn remove_if_same(&self, key: &str, candidate: &Arc<AsyncMutex<()>>) {
        let mut map = self.lock_map();
        if let Some(existing) = map.get(key)
            && Arc::ptr_eq(existing, candidate)
        {
            map.remove(key);
        }
    }
}

pub fn build_pages(stdout: &str, stderr: &str, page_lines: usize) -> Vec<TranscriptPage> {
    let stdout_pages = split_stream(stdout, page_lines);
    let stderr_pages = split_stream(stderr, page_lines);
    let page_count = stdout_pages.len().max(stderr_pages.len()).max(1);

    (0..page_count)
        .map(|index| TranscriptPage {
            stdout: stdout_pages.get(index).cloned().unwrap_or_default(),
            stderr: stderr_pages.get(index).cloned().unwrap_or_default(),
        })
        .collect()
}

pub fn make_execute_key(
    repo_root: &str,
    recipe: &str,
    dir: &str,
    args: Option<&HashMap<String, Value>>,
) -> Result<String, String> {
    let normalized_args = normalize_args(args)?;
    serde_json::to_string(&(
        repo_root,
        dir.trim_end_matches('/'),
        recipe,
        normalized_args,
    ))
    .map_err(|e| format!("Failed to serialize execute key: {e}"))
}

fn split_stream(stream: &str, page_lines: usize) -> Vec<String> {
    if stream.is_empty() {
        return Vec::new();
    }

    let lines: Vec<&str> = stream.split_inclusive('\n').collect();
    if lines.is_empty() {
        return vec![stream.to_string()];
    }

    lines.chunks(page_lines).map(<[&str]>::concat).collect()
}

fn normalize_args(args: Option<&HashMap<String, Value>>) -> Result<String, String> {
    let Some(args) = args else {
        return Ok("null".to_string());
    };

    let normalized = normalize_object(args)?;
    serde_json::to_string(&normalized).map_err(|e| format!("Failed to serialize execute args: {e}"))
}

fn normalize_object(map: &HashMap<String, Value>) -> Result<Value, String> {
    let sorted: Result<BTreeMap<_, _>, _> = map
        .iter()
        .map(|(key, value)| normalize_value(value).map(|value| (key.clone(), value)))
        .collect();
    sorted
        .map(|map| Value::Object(map.into_iter().collect()))
        .map_err(|e| format!("Failed to normalize execute args: {e}"))
}

fn normalize_value(value: &Value) -> Result<Value, String> {
    match value {
        Value::Array(items) => items
            .iter()
            .map(normalize_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(map) => map
            .iter()
            .map(|(key, value)| normalize_value(value).map(|value| (key.clone(), value)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(|map| Value::Object(map.into_iter().collect())),
        _ => Ok(value.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::thread;

    fn numbered_output(prefix: &str, range: std::ops::RangeInclusive<usize>) -> String {
        use std::fmt::Write;

        let mut out = String::new();
        for n in range {
            let _ = writeln!(out, "{prefix}-{n}");
        }
        out
    }

    #[test]
    fn build_pages_always_returns_at_least_one_page() {
        let pages = build_pages("", "", EXECUTE_PAGE_LINES);
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0], TranscriptPage::default());
    }

    #[test]
    fn build_pages_preserves_head_first_stream_order() {
        let stdout = numbered_output("out", 1..=205);
        let stderr = numbered_output("err", 1..=3);

        let pages = build_pages(&stdout, &stderr, EXECUTE_PAGE_LINES);
        assert_eq!(pages.len(), 2);
        assert!(pages[0].stdout.starts_with("out-1\n"));
        assert!(pages[0].stdout.contains("out-200\n"));
        assert_eq!(pages[0].stderr, stderr);
        assert_eq!(pages[1].stdout, numbered_output("out", 201..=205));
        assert!(pages[1].stderr.is_empty());
    }

    #[test]
    fn build_pages_respects_custom_page_boundary() {
        let stdout = numbered_output("out", 1..=5);
        let stderr = numbered_output("err", 1..=4);

        let pages = build_pages(&stdout, &stderr, 2);
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].stdout, numbered_output("out", 1..=2));
        assert_eq!(pages[0].stderr, numbered_output("err", 1..=2));
        assert_eq!(pages[1].stdout, numbered_output("out", 3..=4));
        assert_eq!(pages[1].stderr, numbered_output("err", 3..=4));
        assert_eq!(pages[2].stdout, numbered_output("out", 5..=5));
        assert!(pages[2].stderr.is_empty());
    }

    #[test]
    fn make_execute_key_normalizes_arg_order() {
        let args_a = HashMap::from([
            ("b".to_string(), json!(2)),
            ("a".to_string(), json!({"y": 2, "x": 1})),
        ]);
        let args_b = HashMap::from([
            ("a".to_string(), json!({"x": 1, "y": 2})),
            ("b".to_string(), json!(2)),
        ]);

        let key_a = make_execute_key("/repo", "check", "/repo", Some(&args_a))
            .unwrap_or_else(|err| panic!("key_a serialization failed: {err}"));
        let key_b = make_execute_key("/repo", "check", "/repo/", Some(&args_b))
            .unwrap_or_else(|err| panic!("key_b serialization failed: {err}"));
        assert_eq!(key_a, key_b);
    }

    #[test]
    fn make_execute_key_structured_serialization_avoids_delimiter_collisions() {
        let key_a = make_execute_key("/repo", "b|args=null|recipe=c", "/a", None)
            .unwrap_or_else(|err| panic!("key_a serialization failed: {err}"));
        let key_b = make_execute_key("/repo", "c", "/a|recipe=b|args=null", None)
            .unwrap_or_else(|err| panic!("key_b serialization failed: {err}"));

        assert_ne!(key_a, key_b);
    }

    #[test]
    fn make_execute_key_distinguishes_null_args_from_empty_map() {
        let empty = HashMap::new();

        let null_args = make_execute_key("/repo", "check", "/repo", None)
            .unwrap_or_else(|err| panic!("null args key serialization failed: {err}"));
        let empty_args = make_execute_key("/repo", "check", "/repo", Some(&empty))
            .unwrap_or_else(|err| panic!("empty args key serialization failed: {err}"));

        assert_ne!(null_args, empty_args);
    }

    #[test]
    fn single_flight_recovers_from_poisoned_outer_mutex() {
        let single_flight = Arc::new(SingleFlight::new());
        let poison_target = Arc::clone(&single_flight);
        let _ = thread::spawn(move || {
            let _guard = poison_target
                .map
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            panic!("poison single-flight outer mutex");
        })
        .join();

        let first = single_flight.get_or_create("same-key");
        let second = single_flight.get_or_create("same-key");
        assert!(Arc::ptr_eq(&first, &second));
    }
}
