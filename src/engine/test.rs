#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{runner::{WXMode, get_project_config_file_path, DebugLevel}, file::project::{load_project_config, load_modules}, analysis::{dependencies::analyse_module_deps, routes::analyse_module_routes}, engine::runtime::{WXRuntime, WXRuntimeInfo}};

    /// Kill the runtime after `TIMEOUT` seconds.
    /// This is useful for debugging the runtime
    /// and when the tests are run in a CI/CD environment
    /// where the runtime should **NOT** run forever.
    static KILL_AFTER_TIMEOUT: bool = true;
    /// Kill the runtime after `TIMEOUT` seconds.
    static TIMEOUT: u64 = 10;

    #[test]
    fn test_example_todo() {
        let mode = WXMode::Dev(DebugLevel::Max);
        let root = Path::new("examples/todo");
        let config = load_project_config(&get_project_config_file_path(root)).unwrap();
        let source_root = root.join(&config.src);
        let webx_modules = load_modules(&source_root);
        analyse_module_deps(&webx_modules);
        analyse_module_routes(&webx_modules);
        let (_, dummy_rx) = std::sync::mpsc::channel();
        if KILL_AFTER_TIMEOUT {
            std::thread::spawn(move || {
                let mut runtime = WXRuntime::new(dummy_rx, mode, WXRuntimeInfo::new(root));
                runtime.load_modules(webx_modules);
                runtime.run();
            });
            std::thread::sleep(std::time::Duration::from_secs(TIMEOUT));
            std::process::exit(0);
        } else {
            let mut runtime = WXRuntime::new(dummy_rx, mode, WXRuntimeInfo::new(root));
            runtime.load_modules(webx_modules);
            runtime.run();
        }
    }
}
