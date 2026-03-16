/// Integration tests for plugin loading, lifecycle, and panic safety.
///
/// These tests build the `neoimv-test-plugin` cdylib, then load it through
/// `LoadedPlugin` to verify the full plugin FFI contract.
use neoimv::plugin::ffi::LoadedPlugin;
use neoimv_plugin_sdk::{PluginContext, CAPABILITY_ACTIONS};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns the workspace root (parent of the neoimv crate directory).
fn workspace_root() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().expect("neoimv crate has a parent").to_path_buf()
}

/// Builds the test plugin and returns the path to the compiled .so.
fn build_test_plugin() -> PathBuf {
    let ws_root = workspace_root();

    // Use the same cargo that built this test binary.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());

    let status = Command::new(&cargo)
        .args(["build", "-p", "neoimv-test-plugin"])
        .current_dir(&ws_root)
        .status()
        .expect("failed to run cargo build for test plugin");

    assert!(
        status.success(),
        "cargo build -p neoimv-test-plugin failed with exit code: {status}"
    );

    // The .so is placed in target/debug/ under the workspace root.
    let so_path = ws_root.join("target/debug/libneoimv_test_plugin.so");
    assert!(
        so_path.exists(),
        "test plugin .so not found at: {}",
        so_path.display()
    );
    so_path
}

/// Load the test plugin with the supported API version list.
unsafe fn load_plugin(so_path: &Path) -> LoadedPlugin {
    LoadedPlugin::load(so_path, &[1]).expect("failed to load test plugin")
}

/// Create an empty PluginContext for tests that don't need image data.
fn empty_ctx() -> PluginContext {
    PluginContext {
        image: std::ptr::null(),
        image_path: std::ptr::null(),
        image_count: 0,
        image_index: 0,
        config: std::ptr::null(),
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[test]
fn test_plugin_info_correct() {
    let so_path = build_test_plugin();
    let plugin = unsafe { load_plugin(&so_path) };

    assert_eq!(plugin.name(), "test-plugin");
    // plugin_info is called during load — if we get here without panic it worked
}

#[test]
fn test_plugin_init_shutdown_lifecycle() {
    let so_path = build_test_plugin();
    let plugin = unsafe { load_plugin(&so_path) };

    let ctx = empty_ctx();
    plugin.init(&ctx).expect("plugin_init should succeed");
    plugin.shutdown();
    // Dropping plugin calls shutdown again via Drop — must not crash
    drop(plugin);
}

#[test]
fn test_incompatible_api_version_rejected() {
    let so_path = build_test_plugin();
    // Request only version 99 — test plugin exports api_version=1
    let result = unsafe { LoadedPlugin::load(&so_path, &[99]) };
    assert!(
        result.is_err(),
        "loading with wrong API version should fail"
    );
    let err_msg = match result {
        Err(e) => e.to_string(),
        Ok(_) => panic!("expected Err but got Ok"),
    };
    assert!(
        err_msg.contains("incompatible") || err_msg.contains("ncompatible"),
        "error message should mention incompatible: {err_msg}"
    );
}

#[test]
fn test_get_actions_contains_test_hello() {
    let so_path = build_test_plugin();
    let plugin = unsafe { load_plugin(&so_path) };

    let ctx = empty_ctx();
    plugin.init(&ctx).expect("init");

    let actions = plugin.get_actions();
    assert!(
        !actions.is_empty(),
        "test plugin should expose at least one action"
    );

    let ids: Vec<&str> = actions.iter().map(|(id, _, _, _)| id.as_str()).collect();
    assert!(
        ids.contains(&"test:hello"),
        "expected 'test:hello' action, got: {ids:?}"
    );
}

#[test]
fn test_on_action_test_hello_returns_image() {
    let so_path = build_test_plugin();
    let plugin = unsafe { load_plugin(&so_path) };

    let ctx = empty_ctx();
    plugin.init(&ctx).expect("init");

    let action_id = std::ffi::CString::new("test:hello").unwrap();
    let result = plugin.on_action(&action_id, &ctx);

    assert!(
        result.is_some(),
        "test:hello action should return an image"
    );
    let (pixels, width, height) = result.unwrap();
    assert_eq!(width, 1, "expected 1px wide image");
    assert_eq!(height, 1, "expected 1px tall image");
    assert_eq!(pixels.len(), 4, "expected 4 bytes (RGBA)");
    assert_eq!(&pixels[..], &[255, 0, 0, 255], "expected red pixel");
}

#[test]
fn test_on_action_unknown_action_returns_none() {
    let so_path = build_test_plugin();
    let plugin = unsafe { load_plugin(&so_path) };

    let ctx = empty_ctx();
    plugin.init(&ctx).expect("init");

    let action_id = std::ffi::CString::new("test:nonexistent").unwrap();
    let result = plugin.on_action(&action_id, &ctx);
    assert!(result.is_none(), "unknown action should return None, not crash");
}

#[test]
fn test_capabilities_include_actions() {
    let so_path = build_test_plugin();
    let plugin = unsafe { load_plugin(&so_path) };

    assert!(
        plugin.has_capability(CAPABILITY_ACTIONS),
        "test plugin should advertise CAPABILITY_ACTIONS"
    );
}

#[test]
fn test_on_config_changed_does_not_crash() {
    let so_path = build_test_plugin();
    let plugin = unsafe { load_plugin(&so_path) };

    let ctx = empty_ctx();
    plugin.init(&ctx).expect("init");

    let config = std::ffi::CString::new(r#"{"key":"value"}"#).unwrap();
    // Must not panic or crash
    plugin.on_config_changed(&config);
}

#[test]
fn test_drop_calls_shutdown_without_crash() {
    let so_path = build_test_plugin();
    {
        let plugin = unsafe { load_plugin(&so_path) };
        let ctx = empty_ctx();
        plugin.init(&ctx).expect("init");
        // plugin dropped here — Drop impl calls shutdown()
    }
    // If we reach here without a crash/panic, the test passes.
}

#[test]
fn test_load_nonexistent_path_fails() {
    let bad_path = Path::new("/tmp/does_not_exist_abc123.so");
    let result = unsafe { LoadedPlugin::load(bad_path, &[1]) };
    assert!(result.is_err(), "loading a nonexistent .so should fail");
}
