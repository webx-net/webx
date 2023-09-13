use std::path::Path;

use crate::analytics::dependencies::analyse_module_deps;
use crate::file::webx::WXModule;
use crate::project::{load_modules, load_project_config};

const PROJECT_CONFIG_FILE_NAME: &str = "webx.config.json";

pub fn run(root: &Path, prod: bool) {
    let config_file = root.join(PROJECT_CONFIG_FILE_NAME);
    let config = load_project_config(&config_file);
    let source_root = root.join(&config.src);
    let webx_modules = load_modules(&source_root);
    analyse_module_deps(&webx_modules);

    println!(
        "Webx modules: {:?}",
        webx_modules
            .iter()
            .map(WXModule::module_name)
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "Running web server in {} mode",
        if prod { "production" } else { "development" }
    );
    println!("Directory: {}", root.display());
}
