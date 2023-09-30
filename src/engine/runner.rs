use std::path::Path;

use crate::analytics::{dependencies::analyse_module_deps, routes::analyse_module_routes};
use crate::file::webx::WXModule;
use crate::project::{load_modules, load_project_config};

const PROJECT_CONFIG_FILE_NAME: &str = "webx.config.json";

fn print_modules(modules: &Vec<WXModule>) {
    if modules.len() == 0 {
        println!("No WebX modules found");
        return;
    } else if modules.len() == 1 {
        println!("WebX module: {}", modules[0].path.module_name());
    } else {
        println!("{} WebX modules:", modules.len());
        let mut names = modules.iter().map(|module| module.path.module_name()).collect::<Vec<_>>();
        names.sort_by(|a, b| a.cmp(b));
        for name in names.iter() {
            println!("  - {}", name);
        }
    }
}

pub fn run(root: &Path, prod: bool) {
    let config_file = root.join(PROJECT_CONFIG_FILE_NAME);
    let config = load_project_config(&config_file);
    let source_root = root.join(&config.src);
    let webx_modules = load_modules(&source_root);
    analyse_module_deps(&webx_modules);
    analyse_module_routes(&webx_modules);
    print_modules(&webx_modules);
    println!(
        "Running web server in {} mode",
        if prod { "production" } else { "development" }
    );
    println!("Directory: {}", root.display());
}
