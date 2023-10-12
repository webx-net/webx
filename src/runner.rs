use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Instant;
use chrono::offset::Local;
use chrono::{self};
use chrono::DateTime;
use colored::Colorize;
use notify::{self, Watcher, Error, Event};

use crate::analytics::{dependencies::analyse_module_deps, routes::analyse_module_routes};
use crate::engine::runtime::{WXRuntime, WXRuntimeMessage};
use crate::file::parser::parse_webx_file;
use crate::file::webx::{WXModule, WXModulePath};
use crate::file::project::{load_modules, load_project_config};
use crate::reporting::warning::warning;

const PROJECT_CONFIG_FILE_NAME: &str = "webx.config.json";

#[derive(Debug, Clone, Copy)]
pub enum WXMode {
    Dev,
    Prod,
}

impl PartialEq<WXMode> for WXMode {
    fn eq(&self, other: &WXMode) -> bool {
        match (self, other) {
            (WXMode::Dev, WXMode::Dev) => true,
            (WXMode::Prod, WXMode::Prod) => true,
            _ => false
        }
    }
}

fn print_start_info(modules: &Vec<WXModule>, mode: WXMode, start_duration: std::time::Duration) {
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
        if mode == WXMode::Prod { "production" } else { "development" }
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
    // FS Watch
    println!("{} Watching for file changes", prefix);

    println!("{}{}", "+".bright_black(), "-".repeat(width).bright_black());
}

/// Run a WebX **project** from the given root path.
/// 
/// ## Arguments
/// - `root` - The root path of the project.
/// - `mode` - The mode to run in.
pub fn run(root: &Path, mode: WXMode) {
    let time_start = Instant::now();
    let config_file = root.join(PROJECT_CONFIG_FILE_NAME);
    let config = load_project_config(&config_file);
    let source_root = root.join(&config.src);
    let webx_modules = load_modules(&source_root);
    analyse_module_deps(&webx_modules);
    analyse_module_routes(&webx_modules);
    print_start_info(&webx_modules, mode, time_start.elapsed());
    let (rt_tx, rt_rx) = std::sync::mpsc::channel();
    let mut runtime = WXRuntime::new(rt_rx, mode);
    runtime.load_modules(webx_modules);
    register_filewatcher(&source_root, rt_tx);
    let runtime_hnd = std::thread::spawn(move || runtime.run());
    runtime_hnd.join().unwrap();
    // Check ps info: `ps | ? ProcessName -eq "webx"`
    // On interrupt, all threads are also terminated
}

fn register_filewatcher(source_root: &PathBuf, rt_tx: Sender<WXRuntimeMessage>) {
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, Error>| {
        match res {
            Ok(event) => {
                match event.kind {
                    notify::EventKind::Create(_) => {
                        match parse_webx_file(&event.paths[0]) {
                            Ok(module) => rt_tx.send(WXRuntimeMessage::NewModule(module)).unwrap(),
                            Err(e) => warning(format!("watch error: {:?}", e))
                        }
                    },
                    notify::EventKind::Modify(_) => {
                        match parse_webx_file(&event.paths[0]) {
                            Ok(module) => rt_tx.send(WXRuntimeMessage::SwapModule(WXModulePath::new(event.paths[0].clone()), module)).unwrap(),
                            Err(e) => warning(format!("watch error: {:?}", e))
                        }
                    },
                    notify::EventKind::Remove(_) => {
                        println!("remove");
                        rt_tx.send(WXRuntimeMessage::RemoveModule(WXModulePath::new(event.paths[0].clone()))).unwrap();
                    },
                    _ => ()
                }
            },
            Err(e) => warning(format!("watch error: {:?}", e))
        }
    }).unwrap();
    watcher.watch(&source_root, notify::RecursiveMode::Recursive).unwrap();
}