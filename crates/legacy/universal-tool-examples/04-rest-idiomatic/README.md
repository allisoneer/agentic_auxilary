# Example: REST API - Idiomatic Patterns

## Overview

This example demonstrates production-ready REST API patterns using the Universal Tool Framework. It implements a complete task management system with projects and tasks, showcasing RESTful design principles and best practices.

## Key Features

- **RESTful Resource Design**: Proper HTTP methods and nested resources
- **CRUD Operations**: Full Create, Read, Update, Delete for all resources
- **Query Parameters**: Filtering, pagination, and search
- **Request/Response Models**: Separate DTOs for different operations
- **Error Handling**: Consistent error responses with proper HTTP status codes
- **Middleware**: CORS support and request tracing
- **Resource Relationships**: Projects contain tasks with proper nesting
- **Statistics Endpoint**: Aggregated data endpoint

## Running the Example

### Start the REST API

```bash
cargo run --example 04-rest-idiomatic
# API will be available at http://localhost:3000/api/v1
```

### API Endpoints

#### Projects

```bash
# List all projects (with pagination)
curl "http://localhost:3000/api/v1/projects?page=1&per_page=10"

# Create a new project
curl -X POST http://localhost:3000/api/v1/projects \
  -H "Content-Type: application/json" \
  -d '{"name": "Website Redesign", "description": "Complete overhaul of company website"}'

# Get a specific project
curl http://localhost:3000/api/v1/projects/{project_id}

# Update a project
curl -X PUT http://localhost:3000/api/v1/projects/{project_id} \
  -H "Content-Type: application/json" \
  -d '{"name": "Updated Name"}'

# Delete a project (and all its tasks)
curl -X DELETE http://localhost:3000/api/v1/projects/{project_id}
```

#### Tasks

```bash
# List tasks for a project (with filtering)
curl "http://localhost:3000/api/v1/projects/{project_id}/tasks?status=todo&priority=high&page=1"

# Create a task in a project
curl -X POST http://localhost:3000/api/v1/projects/{project_id}/tasks \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Design homepage mockup",
    "description": "Create initial design concepts",
    "priority": "high",
    "assigned_to": "john.doe@example.com",
    "due_date": "2024-12-31T23:59:59Z"
  }'

# Get a specific task
curl http://localhost:3000/api/v1/tasks/{task_id}

# Update a task
curl -X PUT http://localhost:3000/api/v1/tasks/{task_id} \
  -H "Content-Type: application/json" \
  -d '{"status": "inprogress", "priority": "critical"}'

# Delete a task
curl -X DELETE http://localhost:3000/api/v1/tasks/{task_id}

# List all tasks across all projects
curl "http://localhost:3000/api/v1/tasks?status=inprogress&assigned_to=john.doe@example.com"

# Get task statistics
curl http://localhost:3000/api/v1/stats/tasks
```

## Code Highlights

### RESTful Resource Design

The API follows REST conventions with properly nested resources:

```rust
#[universal_tool(
    description = "List tasks for a project with filtering",
    rest(method = "GET", path = "/projects/:project_id/tasks")
)]
async fn list_project_tasks(
    &self,
    #[universal_tool_param(source = "path")] project_id: Uuid,
    #[universal_tool_param(source = "query")] status: Option<TaskStatus>,
    #[universal_tool_param(source = "query")] priority: Option<Priority>,
    // ... pagination params
) -> Result<PaginatedResponse<Task>, ToolError>
```

### Pagination Support

All list endpoints support pagination with a consistent response format:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct PaginatedResponse<T> {
    data: Vec<T>,
    page: u32,
    per_page: u32,
    total: usize,
    total_pages: u32,
}
```

### Request Validation

Separate request/response models for different operations:

```rust
// Creating resources
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CreateTaskRequest {
    title: String,
    description: Option<String>,
    priority: Priority,
    assigned_to: Option<String>,
    due_date: Option<DateTime<Utc>>,
}

// Updating resources (all fields optional)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct UpdateTaskRequest {
    title: Option<String>,
    description: Option<String>,
    status: Option<TaskStatus>,
    priority: Option<Priority>,
    assigned_to: Option<String>,
    due_date: Option<DateTime<Utc>>,
}
```

### Error Handling

UTF's built-in error handling provides proper HTTP status codes:

```rust
projects.get(&project_id)
    .cloned()
    .ok_or_else(|| ToolError::not_found(format!("Project {} not found", project_id)))
```

### Middleware Integration

The example shows how to add standard REST API middleware:

```rust
let app = task_manager.create_rest_router()
    .layer(CorsLayer::permissive())
    .layer(TraceLayer::new_for_http());
```

## Production Considerations

1. **Authentication**: Add authentication middleware to protect endpoints
2. **Rate Limiting**: Implement rate limiting for public APIs
3. **Database**: Replace in-memory storage with a real database
4. **Validation**: Add more comprehensive input validation
5. **Monitoring**: Integrate with monitoring and observability tools
6. **API Documentation**: Consider adding OpenAPI/Swagger documentation

## Learn More

- [UTF REST API Documentation](../../docs/rest-api.md)
- [Error Handling Guide](../../docs/error-handling.md)
- [Example: Simple REST API](../03-rest-simple/README.md)
- [Example: Kitchen Sink](../06-kitchen-sink/README.md)