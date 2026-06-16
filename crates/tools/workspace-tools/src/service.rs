use agentic_tools_core::ToolError;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use tokio::sync::Mutex as AsyncMutex;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct TodoItem {
    pub content: String,
    pub status: String,
    pub priority: String,
}

#[derive(Clone)]
pub struct WorkspaceRuntime {
    inner: Result<Arc<WorkspaceTools>, Arc<String>>,
}

impl WorkspaceRuntime {
    pub fn discover() -> Self {
        let inner = WorkspaceTools::discover().map(Arc::new).map_err(Arc::new);
        Self { inner }
    }

    #[cfg(test)]
    pub(crate) fn from_root(root: &Path) -> Result<Self, String> {
        WorkspaceTools::from_root(root)
            .map(Arc::new)
            .map(|tools| Self { inner: Ok(tools) })
    }

    pub fn tools(&self) -> Result<Arc<WorkspaceTools>, ToolError> {
        self.inner.clone().map_err(|message| {
            ToolError::Internal(format!(
                "workspace tools are unavailable: {message}. Use workspace-relative paths such as `src/main.rs`."
            ))
        })
    }
}

pub struct WorkspaceTools {
    root: PathBuf,
    todos: Arc<RwLock<Vec<TodoItem>>>,
    file_locks: Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>,
}

impl WorkspaceTools {
    fn discover() -> Result<Self, String> {
        let cwd = std::env::current_dir().map_err(|error| error.to_string())?;
        Self::from_root(&cwd)
    }

    fn from_root(root: &Path) -> Result<Self, String> {
        let root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;

        Ok(Self {
            root,
            todos: Arc::new(RwLock::new(Vec::new())),
            file_locks: Mutex::new(HashMap::new()),
        })
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn read_todos(&self) -> Result<Vec<TodoItem>, ToolError> {
        self.todos
            .read()
            .map_err(|error| ToolError::Internal(error.to_string()))
            .map(|todos| todos.clone())
    }

    pub fn replace_todos(&self, todos: Vec<TodoItem>) -> Result<Vec<TodoItem>, ToolError> {
        let mut guard = self
            .todos
            .write()
            .map_err(|error| ToolError::Internal(error.to_string()))?;
        *guard = todos;
        Ok(guard.clone())
    }

    pub fn file_lock(&self, path: &std::path::Path) -> Result<Arc<AsyncMutex<()>>, ToolError> {
        let mut guard = self
            .file_locks
            .lock()
            .map_err(|error| ToolError::Internal(error.to_string()))?;

        Ok(Arc::clone(
            guard
                .entry(path.to_path_buf())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        ))
    }
}
