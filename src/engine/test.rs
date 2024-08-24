#[cfg(test)]
mod tests {
    use std::{path::Path, sync::Arc};

    use crate::{
        analysis::{dependencies::analyze_module_deps, routes::analyze_module_routes},
        engine::runtime::{WXRuntime, WXRuntimeInfo},
        file::project::{load_modules, load_project_config},
        runner::{get_project_config_file_path, DebugLevel, WXMode},
    };

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
        let source_root = root.join(config.src);
        let webx_modules = load_modules(&source_root).unwrap();
        analyze_module_deps(&webx_modules);
        analyze_module_routes(&webx_modules);
        let (_, dummy_rx) = std::sync::mpsc::channel();
        if KILL_AFTER_TIMEOUT {
            let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
            let runtime_running = running.clone();
            std::thread::spawn(move || {
                let mut runtime = WXRuntime::new(dummy_rx, mode, WXRuntimeInfo::new(root));
                runtime.load_modules(webx_modules);
                runtime.run(runtime_running);
            });
            std::thread::sleep(std::time::Duration::from_secs(TIMEOUT));
            running.store(false, std::sync::atomic::Ordering::Relaxed);
            std::process::exit(0);
        } else {
            let mut runtime = WXRuntime::new(dummy_rx, mode, WXRuntimeInfo::new(root));
            runtime.load_modules(webx_modules);
            runtime.run(Arc::new(std::sync::atomic::AtomicBool::new(true)));
        }
    }
}
