use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use ::tower_http::cors::CorsLayer;
use ::tower_http::trace::TraceLayer;
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::info;
use universal_tool_core::prelude::*;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct Project {
    id: Uuid,
    name: String,
    description: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct Task {
    id: Uuid,
    project_id: Uuid,
    title: String,
    description: Option<String>,
    status: TaskStatus,
    priority: Priority,
    assigned_to: Option<String>,
    due_date: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum TaskStatus {
    Todo,
    InProgress,
    Review,
    Done,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum Priority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CreateProjectRequest {
    name: String,
    description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct UpdateProjectRequest {
    name: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CreateTaskRequest {
    title: String,
    description: Option<String>,
    priority: Priority,
    assigned_to: Option<String>,
    due_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct UpdateTaskRequest {
    title: Option<String>,
    description: Option<String>,
    status: Option<TaskStatus>,
    priority: Option<Priority>,
    assigned_to: Option<String>,
    due_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct TaskFilter {
    status: Option<TaskStatus>,
    priority: Option<Priority>,
    assigned_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct PaginationParams {
    page: Option<u32>,
    per_page: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct PaginatedResponse<T> {
    data: Vec<T>,
    page: u32,
    per_page: u32,
    total: usize,
    total_pages: u32,
}

#[derive(Clone)]
struct TaskManager {
    projects: Arc<RwLock<HashMap<Uuid, Project>>>,
    tasks: Arc<RwLock<HashMap<Uuid, Task>>>,
}

impl TaskManager {
    fn new() -> Self {
        Self {
            projects: Arc::new(RwLock::new(HashMap::new())),
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn paginate<T: Clone>(items: Vec<T>, params: &PaginationParams) -> PaginatedResponse<T> {
        let page = params.page.unwrap_or(1).max(1);
        let per_page = params.per_page.unwrap_or(20).min(100);
        let total = items.len();
        let total_pages = ((total as f32) / (per_page as f32)).ceil() as u32;

        let start = ((page - 1) * per_page) as usize;
        let end = (start + per_page as usize).min(total);

        PaginatedResponse {
            data: items[start..end].to_vec(),
            page,
            per_page,
            total,
            total_pages,
        }
    }
}

#[universal_tool_router(rest(prefix = "/api/v1"))]
impl TaskManager {
    #[universal_tool(
        description = "List all projects with pagination",
        rest(method = "GET", path = "/projects")
    )]
    async fn list_projects(
        &self,
        #[universal_tool_param(source = "query")] page: Option<u32>,
        #[universal_tool_param(source = "query")] per_page: Option<u32>,
    ) -> Result<PaginatedResponse<Project>, ToolError> {
        let projects = self
            .projects
            .read()
            .map_err(|_| ToolError::internal("Failed to read projects"))?;

        let mut project_list: Vec<Project> = projects.values().cloned().collect();
        project_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let pagination = PaginationParams { page, per_page };
        Ok(Self::paginate(project_list, &pagination))
    }

    #[universal_tool(
        description = "Create a new project",
        rest(method = "POST", path = "/projects")
    )]
    async fn create_project(
        &self,
        #[universal_tool_param(source = "body")] request: CreateProjectRequest,
    ) -> Result<Project, ToolError> {
        let now = Utc::now();
        let project = Project {
            id: Uuid::new_v4(),
            name: request.name,
            description: request.description,
            created_at: now,
            updated_at: now,
        };

        self.projects
            .write()
            .map_err(|_| ToolError::internal("Failed to write projects"))?
            .insert(project.id, project.clone());

        info!("Created project: {}", project.id);
        Ok(project)
    }

    #[universal_tool(
        description = "Get a project by ID",
        rest(method = "GET", path = "/projects/:project_id")
    )]
    async fn get_project(&self, project_id: Uuid) -> Result<Project, ToolError> {
        let projects = self
            .projects
            .read()
            .map_err(|_| ToolError::internal("Failed to read projects"))?;

        projects
            .get(&project_id)
            .cloned()
            .ok_or_else(|| ToolError::not_found(format!("Project {} not found", project_id)))
    }

    #[universal_tool(
        description = "Update a project",
        rest(method = "PUT", path = "/projects/:project_id")
    )]
    async fn update_project(
        &self,
        project_id: Uuid,
        #[universal_tool_param(source = "body")] request: UpdateProjectRequest,
    ) -> Result<Project, ToolError> {
        let mut projects = self
            .projects
            .write()
            .map_err(|_| ToolError::internal("Failed to write projects"))?;

        let project = projects
            .get_mut(&project_id)
            .ok_or_else(|| ToolError::not_found(format!("Project {} not found", project_id)))?;

        if let Some(name) = request.name {
            project.name = name;
        }
        if let Some(description) = request.description {
            project.description = description;
        }
        project.updated_at = Utc::now();

        info!("Updated project: {}", project_id);
        Ok(project.clone())
    }

    #[universal_tool(
        description = "Delete a project and all its tasks",
        rest(method = "DELETE", path = "/projects/:project_id")
    )]
    async fn delete_project(&self, project_id: Uuid) -> Result<(), ToolError> {
        let mut projects = self
            .projects
            .write()
            .map_err(|_| ToolError::internal("Failed to write projects"))?;

        projects
            .remove(&project_id)
            .ok_or_else(|| ToolError::not_found(format!("Project {} not found", project_id)))?;

        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| ToolError::internal("Failed to write tasks"))?;

        tasks.retain(|_, task| task.project_id != project_id);

        info!("Deleted project and its tasks: {}", project_id);
        Ok(())
    }

    #[universal_tool(
        description = "List tasks for a project with filtering",
        rest(method = "GET", path = "/projects/:project_id/tasks")
    )]
    async fn list_project_tasks(
        &self,
        project_id: Uuid,
        #[universal_tool_param(source = "query")] status: Option<TaskStatus>,
        #[universal_tool_param(source = "query")] priority: Option<Priority>,
        #[universal_tool_param(source = "query")] assigned_to: Option<String>,
        #[universal_tool_param(source = "query")] page: Option<u32>,
        #[universal_tool_param(source = "query")] per_page: Option<u32>,
    ) -> Result<PaginatedResponse<Task>, ToolError> {
        let projects = self
            .projects
            .read()
            .map_err(|_| ToolError::internal("Failed to read projects"))?;

        if !projects.contains_key(&project_id) {
            return Err(ToolError::not_found(format!(
                "Project {} not found",
                project_id
            )));
        }

        let tasks = self
            .tasks
            .read()
            .map_err(|_| ToolError::internal("Failed to read tasks"))?;

        let mut task_list: Vec<Task> = tasks
            .values()
            .filter(|task| task.project_id == project_id)
            .filter(|task| status.as_ref().map_or(true, |s| &task.status == s))
            .filter(|task| priority.as_ref().map_or(true, |p| &task.priority == p))
            .filter(|task| {
                assigned_to
                    .as_ref()
                    .map_or(true, |a| task.assigned_to.as_ref() == Some(a))
            })
            .cloned()
            .collect();

        task_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let pagination = PaginationParams { page, per_page };
        Ok(Self::paginate(task_list, &pagination))
    }

    #[universal_tool(
        description = "Create a new task in a project",
        rest(method = "POST", path = "/projects/:project_id/tasks")
    )]
    async fn create_task(
        &self,
        project_id: Uuid,
        #[universal_tool_param(source = "body")] request: CreateTaskRequest,
    ) -> Result<Task, ToolError> {
        let projects = self
            .projects
            .read()
            .map_err(|_| ToolError::internal("Failed to read projects"))?;

        if !projects.contains_key(&project_id) {
            return Err(ToolError::not_found(format!(
                "Project {} not found",
                project_id
            )));
        }

        let now = Utc::now();
        let task = Task {
            id: Uuid::new_v4(),
            project_id,
            title: request.title,
            description: request.description,
            status: TaskStatus::Todo,
            priority: request.priority,
            assigned_to: request.assigned_to,
            due_date: request.due_date,
            created_at: now,
            updated_at: now,
        };

        self.tasks
            .write()
            .map_err(|_| ToolError::internal("Failed to write tasks"))?
            .insert(task.id, task.clone());

        info!("Created task: {} in project: {}", task.id, project_id);
        Ok(task)
    }

    #[universal_tool(
        description = "Get a specific task",
        rest(method = "GET", path = "/tasks/:task_id")
    )]
    async fn get_task(&self, task_id: Uuid) -> Result<Task, ToolError> {
        let tasks = self
            .tasks
            .read()
            .map_err(|_| ToolError::internal("Failed to read tasks"))?;

        tasks
            .get(&task_id)
            .cloned()
            .ok_or_else(|| ToolError::not_found(format!("Task {} not found", task_id)))
    }

    #[universal_tool(
        description = "Update a task",
        rest(method = "PUT", path = "/tasks/:task_id")
    )]
    async fn update_task(
        &self,
        task_id: Uuid,
        #[universal_tool_param(source = "body")] request: UpdateTaskRequest,
    ) -> Result<Task, ToolError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| ToolError::internal("Failed to write tasks"))?;

        let task = tasks
            .get_mut(&task_id)
            .ok_or_else(|| ToolError::not_found(format!("Task {} not found", task_id)))?;

        if let Some(title) = request.title {
            task.title = title;
        }
        if let Some(description) = request.description {
            task.description = Some(description);
        }
        if let Some(status) = request.status {
            task.status = status;
        }
        if let Some(priority) = request.priority {
            task.priority = priority;
        }
        if let Some(assigned_to) = request.assigned_to {
            task.assigned_to = Some(assigned_to);
        }
        if let Some(due_date) = request.due_date {
            task.due_date = Some(due_date);
        }
        task.updated_at = Utc::now();

        info!("Updated task: {}", task_id);
        Ok(task.clone())
    }

    #[universal_tool(
        description = "Delete a task",
        rest(method = "DELETE", path = "/tasks/:task_id")
    )]
    async fn delete_task(&self, task_id: Uuid) -> Result<(), ToolError> {
        let mut tasks = self
            .tasks
            .write()
            .map_err(|_| ToolError::internal("Failed to write tasks"))?;

        tasks
            .remove(&task_id)
            .ok_or_else(|| ToolError::not_found(format!("Task {} not found", task_id)))?;

        info!("Deleted task: {}", task_id);
        Ok(())
    }

    #[universal_tool(
        description = "Get all tasks across all projects with filtering",
        rest(method = "GET", path = "/tasks")
    )]
    async fn list_all_tasks(
        &self,
        #[universal_tool_param(source = "query")] status: Option<TaskStatus>,
        #[universal_tool_param(source = "query")] priority: Option<Priority>,
        #[universal_tool_param(source = "query")] assigned_to: Option<String>,
        #[universal_tool_param(source = "query")] page: Option<u32>,
        #[universal_tool_param(source = "query")] per_page: Option<u32>,
    ) -> Result<PaginatedResponse<Task>, ToolError> {
        let tasks = self
            .tasks
            .read()
            .map_err(|_| ToolError::internal("Failed to read tasks"))?;

        let mut task_list: Vec<Task> = tasks
            .values()
            .filter(|task| status.as_ref().map_or(true, |s| &task.status == s))
            .filter(|task| priority.as_ref().map_or(true, |p| &task.priority == p))
            .filter(|task| {
                assigned_to
                    .as_ref()
                    .map_or(true, |a| task.assigned_to.as_ref() == Some(a))
            })
            .cloned()
            .collect();

        task_list.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let pagination = PaginationParams { page, per_page };
        Ok(Self::paginate(task_list, &pagination))
    }

    #[universal_tool(
        description = "Get task statistics",
        rest(method = "GET", path = "/stats/tasks")
    )]
    async fn get_task_stats(&self) -> Result<serde_json::Value, ToolError> {
        let tasks = self
            .tasks
            .read()
            .map_err(|_| ToolError::internal("Failed to read tasks"))?;

        let mut by_status: HashMap<String, usize> = HashMap::new();
        let mut by_priority: HashMap<String, usize> = HashMap::new();

        for task in tasks.values() {
            let status = format!("{:?}", task.status);
            let priority = format!("{:?}", task.priority);

            *by_status.entry(status).or_insert(0) += 1;
            *by_priority.entry(priority).or_insert(0) += 1;
        }

        let stats = serde_json::json!({
            "total": tasks.len(),
            "by_status": by_status,
            "by_priority": by_priority
        });

        Ok(stats)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(true)
        .with_level(true)
        .init();

    let task_manager = Arc::new(TaskManager::new());

    let app = TaskManager::create_rest_router(task_manager.clone())
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("Task Manager API listening on: {}", listener.local_addr()?);
    info!("API endpoints available at: http://localhost:3000/api/v1");

    axum::serve(listener, app).await?;
    Ok(())
}
