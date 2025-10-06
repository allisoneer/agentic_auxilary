use gpt5_reasoner::Gpt5Reasoner;

fn main() {
    let tool = Gpt5Reasoner;
    let tools = tool.get_mcp_tools();

    println!("MCP Tools Schema:");
    println!("{}", serde_json::to_string_pretty(&tools).unwrap());
}
