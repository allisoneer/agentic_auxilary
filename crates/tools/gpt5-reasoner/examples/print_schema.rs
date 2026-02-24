use agentic_config::types::ReasoningConfig;
use gpt5_reasoner::tools::build_registry;

fn main() {
    let registry = build_registry(ReasoningConfig::default());

    println!("gpt5_reasoner Tools ({}):", registry.len());
    for name in registry.list_names() {
        println!("  - {}", name);
    }
}
