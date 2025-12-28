fn main() {
    cynic_codegen::register_schema("linear")
        .from_sdl_file("../schemas/linear.graphql")
        .expect("Failed to find Linear GraphQL schema");
}
