use universal_tool_core::prelude::*;

struct MathTools;

#[universal_tool_router]
impl MathTools {
    fn new() -> Self {
        Self
    }
}

fn main() {
    let tools = MathTools::new();
    println!("Tools created");
}