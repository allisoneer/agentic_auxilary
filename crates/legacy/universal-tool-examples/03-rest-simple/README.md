# Example: REST API - Simple

## Overview

This example demonstrates how to create a basic REST API using the Universal Tool Framework. It shows UTF's automatic route generation, JSON handling, error mapping, and integration with custom endpoints.

## Key Features

- **Automatic Route Generation**: UTF creates REST endpoints from your methods
- **JSON Serialization**: Automatic request/response handling
- **Error Mapping**: ToolError automatically maps to appropriate HTTP status codes
- **Custom Endpoints**: Mix UTF-generated routes with custom handlers
- **Thread-Safe State**: Proper Arc usage for shared state
- **Simple Setup**: Minimal boilerplate to get started

## Running the Example

### Start the Server

```bash
# Build and run
cargo run --example 03-rest-simple

# The server will start on http://127.0.0.1:3000
```

### API Endpoints

Once running, the following endpoints are available:

- `GET  /health` - Health check endpoint
- `GET  /openapi.json` - OpenAPI specification
- `GET  /api/v1/info` - Calculator information
- `POST /api/v1/add` - Add two numbers
- `POST /api/v1/subtract` - Subtract two numbers
- `POST /api/v1/multiply` - Multiply two numbers
- `POST /api/v1/divide` - Divide two numbers

### Example Requests

```bash
# Health check
curl http://127.0.0.1:3000/health

# Calculator info
curl http://127.0.0.1:3000/api/v1/info

# Add two numbers
curl -X POST http://127.0.0.1:3000/api/v1/add \
  -H "Content-Type: application/json" \
  -d '{"a": 10, "b": 5}'
# Response: {"value": 15, "operation": "10 + 5 = 15"}

# Subtract
curl -X POST http://127.0.0.1:3000/api/v1/subtract \
  -H "Content-Type: application/json" \
  -d '{"a": 10, "b": 3}'
# Response: {"value": 7, "operation": "10 - 3 = 7"}

# Multiply
curl -X POST http://127.0.0.1:3000/api/v1/multiply \
  -H "Content-Type: application/json" \
  -d '{"a": 4, "b": 7}'
# Response: {"value": 28, "operation": "4 × 7 = 28"}

# Divide
curl -X POST http://127.0.0.1:3000/api/v1/divide \
  -H "Content-Type: application/json" \
  -d '{"a": 20, "b": 4}'
# Response: {"value": 5, "operation": "20 ÷ 4 = 5"}

# Division by zero (error handling)
curl -X POST http://127.0.0.1:3000/api/v1/divide \
  -H "Content-Type: application/json" \
  -d '{"a": 10, "b": 0}'
# Response: HTTP 400 with error message
```

## Code Highlights

### Basic REST Router Setup

```rust
#[universal_tool_router(
    rest(prefix = "/api/v1")
)]
impl Calculator {
    // Tool methods here
}
```

The `rest(prefix = "/api/v1")` attribute:
- Enables REST API generation
- Sets the base path for all endpoints
- All tool methods will be prefixed with this path

### Simple Tool Method

```rust
#[universal_tool(
    description = "Add two numbers together",
    rest(method = "POST", path = "/add")
)]
pub async fn add(&self, a: f64, b: f64) -> Result<CalculationResult, ToolError> {
    Ok(CalculationResult {
        value: a + b,
        operation: format!("{} + {} = {}", a, b, a + b),
    })
}
```

UTF automatically:
- Creates a POST endpoint at `/api/v1/add`
- Deserializes JSON body to extract `a` and `b`
- Serializes the result to JSON
- Handles errors appropriately

### Error Handling

```rust
pub async fn divide(&self, a: f64, b: f64) -> Result<CalculationResult, ToolError> {
    if b == 0.0 {
        return Err(ToolError::new(
            ErrorCode::InvalidArgument, 
            "Cannot divide by zero"
        ));
    }
    // ... rest of implementation
}
```

UTF's error handling:
- `InvalidArgument` → HTTP 400 Bad Request
- `NotFound` → HTTP 404 Not Found
- `Internal` → HTTP 500 Internal Server Error

### State Management

```rust
// Create the calculator instance wrapped in Arc
let calculator = Arc::new(Calculator {
    name: "UTF Calculator v1.0".to_string(),
});

// Pass to the router
let app = Calculator::create_rest_router(calculator.clone());
```

Important notes:
- State must be wrapped in `Arc` for thread safety
- UTF uses Axum's `State` extractor internally
- Each handler gets access to the shared state

### Mixing Custom Endpoints

```rust
let app = app
    .route("/health", get(health_check))
    .route("/openapi.json", get({
        let calculator = calculator.clone();
        move || async move {
            calculator.get_openapi_spec()
        }
    }));
```

You can:
- Add custom routes alongside UTF-generated ones
- Use standard Axum handlers
- Access the same shared state

## Response Types

UTF automatically handles JSON serialization for return types:

```rust
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
struct CalculationResult {
    value: f64,
    operation: String,
}
```

Requirements:
- Must implement `Serialize` and `Deserialize`
- Must implement `JsonSchema` for OpenAPI generation
- Can be any serializable type

## Next Steps

1. **Add More Endpoints**: Extend with additional mathematical operations
2. **Complex Parameters**: See [REST Idiomatic](../04-rest-idiomatic) for advanced patterns
3. **Middleware**: Add authentication, logging, or rate limiting
4. **Database Integration**: Add persistence to your calculations
5. **OpenAPI**: Enable the `openapi` feature for full API documentation

## Production Considerations

- **CORS**: Add CORS middleware for browser access
- **Authentication**: Implement auth middleware
- **Logging**: Use structured logging with tracing
- **Error Details**: Customize error responses
- **Graceful Shutdown**: Handle server shutdown properly

## Learn More

- [REST Idiomatic Example](../04-rest-idiomatic/README.md) - Production patterns
- [Kitchen Sink Example](../06-kitchen-sink/README.md) - All interfaces combined
- [UTF Architecture](../../docs/architecture.md) - How UTF generates REST code