#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::{runner::{WXMode, get_project_config_file_path, DebugLevel}, file::project::{load_project_config, load_modules}, analysis::{dependencies::analyse_module_deps, routes::analyse_module_routes}, engine::runtime::WXRuntime};

    #[test]
    fn test_example_todo() {
        let mode = WXMode::Dev(DebugLevel::Max);
        let root = &Path::new("examples/todo");
        let config = load_project_config(&get_project_config_file_path(root)).unwrap();
        let source_root = root.join(&config.src);
        let webx_modules = load_modules(&source_root);
        analyse_module_deps(&webx_modules);
        analyse_module_routes(&webx_modules);
        let (_, dummy_rx) = std::sync::mpsc::channel();
        let mut runtime = WXRuntime::new(dummy_rx, mode);
        runtime.load_modules(webx_modules);
        std::thread::spawn(move || runtime.run());
        std::thread::sleep(std::time::Duration::from_secs(10));
        std::process::exit(0);
    }
}
