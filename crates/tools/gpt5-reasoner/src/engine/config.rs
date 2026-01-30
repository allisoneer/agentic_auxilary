pub fn select_optimizer_model(optimizer_model: Option<String>) -> String {
    optimizer_model
        .or_else(|| std::env::var("OPTIMIZER_MODEL").ok())
        .unwrap_or_else(|| "anthropic/claude-sonnet-4.5".to_string())
}

#[cfg(test)]
mod model_selection_tests {
    use super::*;
    use crate::test_support::EnvGuard;
    use serial_test::serial;

    #[test]
    #[serial(env)]
    fn test_default_model_when_no_param_no_env() {
        let _g = EnvGuard::remove("OPTIMIZER_MODEL");
        let model = select_optimizer_model(None);
        assert_eq!(model, "anthropic/claude-sonnet-4.5");
    }

    #[test]
    #[serial(env)]
    fn test_env_overrides_default() {
        let _g = EnvGuard::set("OPTIMIZER_MODEL", "test/model-from-env");
        let model = select_optimizer_model(None);
        assert_eq!(model, "test/model-from-env");
    }

    #[test]
    #[serial(env)]
    fn test_param_overrides_env_and_default() {
        let _g = EnvGuard::set("OPTIMIZER_MODEL", "test/env-model");
        let model = select_optimizer_model(Some("test/param-model".into()));
        assert_eq!(model, "test/param-model");
    }
}
