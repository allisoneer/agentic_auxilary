use coding_agent_tools::CodingAgentTools;
use serial_test::serial;
use tempfile::TempDir;

#[tokio::test]
#[serial]
async fn ls_works_with_tilde_home_override() {
    let td = TempDir::new().unwrap();
    // Canonicalize to resolve symlinks (e.g., /var -> /private/var on macOS)
    let home = td.path().canonicalize().unwrap();

    // SAFETY: serialized test ensures no concurrent env mutations
    unsafe {
        std::env::set_var("__CAT_HOME_FOR_TESTS", &home);
    }

    let tools = CodingAgentTools::new();
    let out = tools
        .ls(Some("~".into()), None, None, None, Some(true))
        .await
        .unwrap();
    assert!(out.root.starts_with(home.to_string_lossy().as_ref()));

    // Cleanup override
    unsafe {
        std::env::remove_var("__CAT_HOME_FOR_TESTS");
    }
}

#[tokio::test]
#[serial]
async fn ls_works_with_tilde_slash_home_override() {
    let td = TempDir::new().unwrap();
    // Canonicalize to resolve symlinks (e.g., /var -> /private/var on macOS)
    let home = td.path().canonicalize().unwrap();
    std::fs::create_dir_all(home.join("subdir")).unwrap();

    unsafe {
        std::env::set_var("__CAT_HOME_FOR_TESTS", &home);
    }

    let tools = CodingAgentTools::new();
    let out = tools
        .ls(Some("~/".into()), None, None, None, Some(true))
        .await
        .unwrap();
    assert!(out.root.starts_with(home.to_string_lossy().as_ref()));

    unsafe {
        std::env::remove_var("__CAT_HOME_FOR_TESTS");
    }
}

#[tokio::test]
#[serial]
async fn search_glob_accepts_tilde_root() {
    let td = TempDir::new().unwrap();
    let home = td.path().to_path_buf();
    std::fs::create_dir_all(home.join("sub")).unwrap();

    unsafe {
        std::env::set_var("__CAT_HOME_FOR_TESTS", &home);
    }

    let tools = CodingAgentTools::new();
    let res = tools
        .search_glob(
            "**/*".into(),
            Some("~".into()),
            None,
            Some(true),
            None,
            Some(10),
            Some(0),
        )
        .await;
    assert!(res.is_ok());

    unsafe {
        std::env::remove_var("__CAT_HOME_FOR_TESTS");
    }
}

#[tokio::test]
#[serial]
async fn search_grep_accepts_tilde_root() {
    let td = TempDir::new().unwrap();
    let home = td.path().to_path_buf();
    std::fs::write(home.join("test.txt"), "hello world").unwrap();

    unsafe {
        std::env::set_var("__CAT_HOME_FOR_TESTS", &home);
    }

    let tools = CodingAgentTools::new();
    let res = tools
        .search_grep(
            "hello".into(),
            Some("~".into()),
            None,
            None,
            None,
            Some(true),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(10),
            Some(0),
        )
        .await;
    assert!(res.is_ok());

    unsafe {
        std::env::remove_var("__CAT_HOME_FOR_TESTS");
    }
}

#[tokio::test]
#[serial]
async fn tilde_expansion_error_when_home_unavailable() {
    unsafe {
        std::env::set_var("__CAT_FORCE_HOME_NONE", "1");
    }

    let tools = CodingAgentTools::new();
    let res = tools.ls(Some("~".into()), None, None, None, None).await;

    unsafe {
        std::env::remove_var("__CAT_FORCE_HOME_NONE");
    }

    assert!(res.is_err());
    let err_msg = res.unwrap_err().to_string();
    assert!(
        err_msg.contains("Could not determine home directory"),
        "Expected home directory error, got: {}",
        err_msg
    );
}
