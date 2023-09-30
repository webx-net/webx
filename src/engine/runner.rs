use std::path::Path;
use std::time::Instant;
use chrono::offset::Local;
use chrono::{self};
use chrono::DateTime;
use colored::Colorize;

use crate::analytics::{dependencies::analyse_module_deps, routes::analyse_module_routes};
use crate::file::webx::WXModule;
use crate::project::{load_modules, load_project_config};

const PROJECT_CONFIG_FILE_NAME: &str = "webx.config.json";

fn print_start_info(modules: &Vec<WXModule>, prod: bool, start_duration: std::time::Duration) {
    let width = 30;
    println!("{}{} {}{} {}", "+".bright_black(), "-".repeat(3).bright_black(), "Web", "X".bright_blue(), "-".repeat(width - 6 - 3).bright_black());
    let prefix = "|".bright_black();
    // Modules
    if modules.len() == 0 {
        println!("{} No modules found", prefix);
        return;
    } else if modules.len() == 1 {
        println!("{} {}: {}", prefix, "Module".bold(), modules[0].path.module_name());
    } else {
        println!("{} {} ({}):", prefix, "Modules".bold(), modules.len());
        let mut names = modules.iter().map(|module| module.path.module_name()).collect::<Vec<_>>();
        names.sort_by(|a, b| a.cmp(b));
        for name in names.iter() {
            println!("{}   - {}", prefix, name);
        }
    }
    // Mode    
    println!(
        "{} {}: {}",
        prefix,
        "Mode".bold(),
        if prod { "production" } else { "development" }
    );
    // Build duration
    println!(
        "{} {}: {:?}",
        prefix,
        "Took".bold(),
        start_duration
    );
    // Build time
    let now: DateTime<Local> = Local::now();
    let time = now.time().format("%H:%M");
    println!("{} {}: {:?} at {}", prefix, "Build".bold(), now.date_naive(), time);

    println!("{}{}", "+".bright_black(), "-".repeat(width).bright_black());
}

pub fn run(root: &Path, prod: bool) {
    let time_start = Instant::now();
    let config_file = root.join(PROJECT_CONFIG_FILE_NAME);
    let config = load_project_config(&config_file);
    let source_root = root.join(&config.src);
    let webx_modules = load_modules(&source_root);
    analyse_module_deps(&webx_modules);
    analyse_module_routes(&webx_modules);
    print_start_info(&webx_modules, prod, time_start.elapsed());
    dbg!(&webx_modules);
}
