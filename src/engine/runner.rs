use std::path::Path;

use crate::file::parser::parse_webx_file;
use crate::file::webx::WebXFile;
use crate::project::{locate_webx_files, load_project_config, construct_dependency_tree, detect_circular_dependencies};
use crate::reporting::error::{exit_error, ERROR_READ_WEBX_FILES, ERROR_CIRCULAR_DEPENDENCY};

const PROJECT_CONFIG_FILE_NAME: &str = "webx.config.json";

pub fn run(root: &Path, prod: bool) {
    let config_file = root.join(PROJECT_CONFIG_FILE_NAME);
    let config = load_project_config(&config_file);
    let source_root = root.join(&config.src);
    let files = match locate_webx_files(&source_root) {
        Ok(files) => files,
        Err(err) => exit_error(format!("Failed to locate webx program files due to, {}", err), ERROR_READ_WEBX_FILES),
    };

    let webx_modules = files.iter().map(|f| parse_webx_file(f)).collect::<Vec<_>>();
    let errors = webx_modules.iter().filter(|m| m.is_err()).map(|m| m.as_ref().unwrap_err()).collect::<Vec<_>>();
    if !errors.is_empty() {
        exit_error(
            format!(
                "Failed to parse webx files:\n{:?}",
                errors
            ),
            ERROR_READ_WEBX_FILES,
        );
    }
    let webx_modules = webx_modules.into_iter().map(|m| m.unwrap()).collect::<Vec<_>>();
    let dependency_tree = construct_dependency_tree(&webx_modules);
    let circular_dependencies = detect_circular_dependencies(&dependency_tree);
    if !circular_dependencies.is_empty() {
        exit_error(
            format!(
                "Circular dependencies detected:\n{:?}",
                circular_dependencies
            ),
            ERROR_CIRCULAR_DEPENDENCY,
        );
    }

    println!("Webx modules: {:?}", webx_modules.iter().map(WebXFile::module_name).collect::<Vec<_>>().join(", "));
    println!("Running web server in {} mode", if prod { "production" } else { "development" });
    println!("Directory: {}", root.display());
}
