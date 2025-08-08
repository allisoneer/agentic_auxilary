use universal_tool_core::prelude::*;

struct Tools;

#[universal_tool_router]
impl Tools {
    fn new() -> Self {
        Self
    }

    /// Test method
    #[universal_tool(description = "Test method")]
    pub async fn test(&self) -> Result<String, ToolError> {
        Ok("test".to_string())
    }
}

fn main() {
    println!("Test compiled");
}
