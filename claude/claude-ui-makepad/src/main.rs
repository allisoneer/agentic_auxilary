//! Claude UI Makepad - Desktop client entry point

mod app;
mod runtime;

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("claude_ui_makepad=debug".parse().unwrap()),
        )
        .init();

    // Run the Makepad app
    app::app_main();
}
