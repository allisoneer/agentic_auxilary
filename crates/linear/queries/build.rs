fn main() -> Result<(), Box<dyn std::error::Error>> {
    cynic_codegen::register_schema("linear").from_sdl_file("linear.graphql")?;
    Ok(())
}
