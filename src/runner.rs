use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Mutex, Arc};
use std::time::Instant;
use chrono::offset::Local;
use chrono::{self};
use chrono::DateTime;
use colored::Colorize;
use notify::{self, Watcher, Error, Event};

use crate::analytics::{dependencies::analyse_module_deps, routes::analyse_module_routes};
use crate::engine::runtime::{WXRuntime, WXRuntimeMessage};
use crate::file::parser::parse_webx_file;
use crate::file::webx::WXModule;
use crate::file::project::{load_modules, load_project_config};

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
    // FS Watch
    println!("{} Watching for file changes", prefix);

    println!("{}{}", "+".bright_black(), "-".repeat(width).bright_black());
}

/// Run a WebX **project** from the given root path.
/// 
/// ## Arguments
/// - `root` - The root path of the project.
/// - `prod` - Whether to run in production mode. 
pub fn run(root: &Path, prod: bool) {
    let time_start = Instant::now();
    let config_file = root.join(PROJECT_CONFIG_FILE_NAME);
    let config = load_project_config(&config_file);
    let source_root = root.join(&config.src);
    let webx_modules = load_modules(&source_root);
    analyse_module_deps(&webx_modules);
    analyse_module_routes(&webx_modules);
    print_start_info(&webx_modules, prod, time_start.elapsed());
    main_loop(&source_root, webx_modules);
}

fn main_loop(source_root: &PathBuf, init_modules: Vec<WXModule>) {
    let (rt_tx, rt_rx) = std::sync::mpsc::channel();
    let source_root = source_root.clone();
    let watch_hnd = std::thread::spawn(move || filewatcher(&source_root, rt_tx));
    let mut runtime = WXRuntime::new(rt_rx);
    runtime.load_modules(init_modules);
    let runtime_hnd = std::thread::spawn(move || runtime.run());
    // Check ps info: `ps | ? ProcessName -eq "webx"`
    runtime_hnd.join().unwrap();
    println!("Runtime thread exited");
    watch_hnd.join().unwrap();
    println!("Watcher thread exited");
    // TODO: Handle Ctrl+C to exit gracefully
}

fn filewatcher(source_root: &PathBuf, rt_tx: Sender<WXRuntimeMessage>) {
    let rt_tx_2 = rt_tx.clone();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, Error>| {
        match res {
            Ok(event) => {
                println!("{:?}", event);
                match event.kind {
                    notify::EventKind::Create(_) => {
                        println!("create");
                        match parse_webx_file(&event.paths[0]) {
                            Ok(module) => rt_tx.send(WXRuntimeMessage::NewModule(module)).unwrap(),
                            Err(e) => println!("watch error: {:?}", e)
                        }
                    },
                    notify::EventKind::Modify(_) => {
                        println!("modify");
                        match parse_webx_file(&event.paths[0]) {
                            Ok(module) => rt_tx.send(WXRuntimeMessage::SwapModule(event.paths[0].clone(), module)).unwrap(),
                            Err(e) => println!("watch error: {:?}", e)
                        }
                    },
                    notify::EventKind::Remove(_) => {
                        println!("remove");
                        rt_tx.send(WXRuntimeMessage::RemoveModule(event.paths[0].clone())).unwrap();
                    },
                    _ => {
                        println!("ignored");
                    }
                }
            },
            Err(e) => {
                println!("watch error: {:?}", e);
            }
        }
    }).unwrap();
    watcher.watch(&source_root, notify::RecursiveMode::Recursive).unwrap();
    let mut c = 0;
    loop {
        rt_tx_2.send(WXRuntimeMessage::Info(format!("Dummy text {}", c))).unwrap();
        c += 1;
        std::thread::sleep(std::time::Duration::from_millis(3000));
    }
}