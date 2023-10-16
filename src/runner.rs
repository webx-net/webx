use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::{Instant, Duration};
use chrono::offset::Local;
use chrono::{self};
use chrono::DateTime;
use colored::Colorize;
use notify::{self, Watcher, Error, Event};

use crate::analysis::{dependencies::analyse_module_deps, routes::analyse_module_routes};
use crate::engine::runtime::{WXRuntime, WXRuntimeMessage, WXRuntimeInfo};
use crate::file::parser::parse_webx_file;
use crate::file::webx::{WXModule, WXModulePath};
use crate::file::project::{load_modules, load_project_config};
use crate::reporting::debug::info;
use crate::reporting::error::{ERROR_PROJECT, exit_error_hint};
use crate::reporting::warning::warning;

pub fn get_project_config_file_path(root: &Path) -> PathBuf {
    root.join("webx.config.json")
}

/// Output verbosity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DebugLevel {
    /// Low debug output (only errors)
    Low = 1,
    /// Medium debug output (errors and warnings)
    Medium = 2,
    /// High debug output (errors, warnings and info)
    High = 3,
    /// All debug output
    Max = 4,
}

impl DebugLevel {
    pub fn from_u8(level: u8) -> Self {
        match level {
            1 => Self::Low,
            2 => Self::Medium,
            3 => Self::High,
            4 => Self::Max,
            _ => Self::Low
        }
    }

    pub fn to_u8(&self) -> u8 {
        match self {
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Max => 4,
        }
    }

    pub fn is_low(&self) -> bool {
        match self {
            Self::Low | Self::Medium | Self::High | Self::Max => true,
            _ => false
        }
    }

    pub fn is_medium(&self) -> bool {
        match self {
            Self::Medium | Self::High | Self::Max => true,
            _ => false
        }
    }

    pub fn is_high(&self) -> bool {
        match self {
            Self::High | Self::Max => true,
            _ => false
        }
    }

    pub fn is_max(&self) -> bool {
        match self {
            Self::Max => true,
            _ => false
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Max => "max",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum WXMode {
    Dev(DebugLevel),
    Prod,
}

impl WXMode {
    pub const MAX: WXMode = WXMode::Dev(DebugLevel::Max);

    pub fn is_dev(&self) -> bool {
        match self {
            Self::Dev(_) => true,
            _ => false
        }
    }

    pub fn debug_level(&self) -> DebugLevel {
        match self {
            Self::Dev(level) => *level,
            _ => DebugLevel::Low
        }
    }

    pub fn is_prod(&self) -> bool {
        match self {
            Self::Prod => true,
            _ => false
        }
    }
}

//* Implement PartialEq for WXMode without taking DebugLevel into account
impl PartialEq<WXMode> for WXMode {
    fn eq(&self, other: &WXMode) -> bool {
        match (self, other) {
            (WXMode::Dev(_), WXMode::Dev(_)) => true,
            (WXMode::Prod, WXMode::Prod) => true,
            _ => false
        }
    }
}

fn print_start_info(modules: &Vec<WXModule>, mode: WXMode, start_duration: std::time::Duration) {
    let width = 28;
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
        if mode.is_prod() { "production" } else { "development" }
    );
    // Debug level
    if mode.is_dev() {
        println!(
            "{} {}: {}",
            prefix,
            "Debug".bold(),
            mode.debug_level().name()
        );
    }
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

/// Run a WebX **project** from the given root path.
/// 
/// ## Arguments
/// - `root` - The root path of the project.
/// - `mode` - The mode to run in.
pub fn run(root: &Path, mode: WXMode) {
    let time_start = Instant::now();
    let config_file = get_project_config_file_path(root);
    let config = if let Some(config) = load_project_config(&config_file) { config} else {
        exit_error_hint("Failed to open WebX configuration.", &[
            "Have you created a WebX project?",
            "Are you in the project root directory?"
        ], ERROR_PROJECT);
    };
    let source_root = root.join(&config.src);
    let webx_modules = load_modules(&source_root);
    analyse_module_deps(&webx_modules);
    analyse_module_routes(&webx_modules);
    print_start_info(&webx_modules, mode, time_start.elapsed());
    let (rt_tx, rt_rx) = std::sync::mpsc::channel();
    let mut runtime = WXRuntime::new(rt_rx, mode, WXRuntimeInfo::new(root));
    runtime.load_modules(webx_modules);
    if mode.is_dev() {
        let fw_rt_tx = rt_tx.clone();
        let fw_hnd = std::thread::spawn(move || run_filewatcher(mode, &source_root, fw_rt_tx));
        let runtime_hnd = std::thread::spawn(move || runtime.run());
        runtime_hnd.join().unwrap();
        fw_hnd.join().unwrap();
    } else {
        // If we are in production mode, run in main thread.
        runtime.run();
    }
    // Check ps info: `ps | ? ProcessName -eq "webx"`
    // On interrupt, all threads are also terminated
}

struct FSWEvent {
    pub kind: notify::EventKind,
    pub path: WXModulePath,
    pub timestamp: Instant,
    is_empty_state: bool,
}

impl FSWEvent {
    fn new(kind: notify::EventKind, path: &PathBuf) -> Self {
        Self {
            kind,
            path: WXModulePath::new(path.clone()),
            timestamp: Instant::now(),
            is_empty_state: false
        }
    }

    fn empty() -> Self {
        Self {
            kind: notify::EventKind::default(),
            path: WXModulePath::new(PathBuf::default()),
            timestamp: Instant::now(),
            is_empty_state: true
        }
    }

    fn is_duplicate(&self, earlier: &Self) -> bool {
        if self.is_empty_state || earlier.is_empty_state { return false; }
        const EPSILON: u128 = 100; // ms
        self.kind == earlier.kind &&
        self.path == earlier.path &&
        self.timestamp.duration_since(earlier.timestamp).as_millis() < EPSILON
    }
}

/// Registers the file watcher thread
fn run_filewatcher(mode: WXMode, source_root: &PathBuf, rt_tx: Sender<WXRuntimeMessage>) {
    let mut last_event: FSWEvent = FSWEvent::empty();
    let mut watcher = notify::recommended_watcher(move |res: Result<Event, Error>| {
        match res {
            Ok(event) => {
                match event.kind {
                    notify::EventKind::Create(_) => {
                        let event = FSWEvent::new(event.kind, &event.paths[0]);
                        if !event.is_duplicate(&last_event) {
                            match parse_webx_file(&event.path.inner) {
                                Ok(module) => rt_tx.send(WXRuntimeMessage::NewModule(module)).unwrap(),
                                Err(e) => warning(mode, format!("(FileWatcher) Error: {:?}", e))
                            }
                        }
                        last_event = event; // Update last event
                    },
                    notify::EventKind::Modify(_) => {
                        let event = FSWEvent::new(event.kind, &event.paths[0]);
                        if !event.is_duplicate(&last_event) {
                            match parse_webx_file(&event.path.inner) {
                                Ok(module) => rt_tx.send(WXRuntimeMessage::SwapModule(event.path.clone(), module)).unwrap(),
                                Err(e) => warning(mode, format!("(FileWatcher) Error: {:?}", e))
                            }
                        }
                        last_event = event; // Update last event
                    },
                    notify::EventKind::Remove(_) => {
                        let event = FSWEvent::new(event.kind, &event.paths[0]);
                        if !event.is_duplicate(&last_event) {
                            rt_tx.send(WXRuntimeMessage::RemoveModule(event.path.clone())).unwrap();
                        }
                        last_event = event; // Update last event
                    },
                    _ => ()
                }
            },
            Err(e) => warning(mode, format!("watch error: {:?}", e))
        }
    }).unwrap();
    watcher.watch(&source_root, notify::RecursiveMode::Recursive).unwrap();
    info(mode, "Hot reloading is enabled.");
    loop {
        std::thread::sleep(Duration::from_millis(1000));
    }
}
