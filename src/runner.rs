use chrono::offset::Local;
use chrono::DateTime;
use chrono::{self};
use colored::Colorize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use crate::analysis::{dependencies::analyze_module_deps, routes::analyze_module_routes};
use crate::engine::filewatcher::WXFileWatcher;
use crate::engine::runtime::{WXRuntime, WXRuntimeInfo};
use crate::engine::server::WXServer;
use crate::file::project::{load_modules, load_project_config, ProjectConfig};
use crate::file::webx::WXModule;
use crate::reporting::error::DateTimeSpecifier;
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
            _ => Self::Low,
        }
    }

    pub fn is_medium(&self) -> bool {
        matches!(self, Self::Medium | Self::High | Self::Max)
    }

    pub fn is_high(&self) -> bool {
        matches!(self, Self::High | Self::Max)
    }

    pub fn is_max(&self) -> bool {
        matches!(self, Self::Max)
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
        matches!(self, Self::Dev(_))
    }

    pub fn is_prod(&self) -> bool {
        matches!(self, Self::Prod)
    }

    pub fn debug_level(&self) -> DebugLevel {
        match self {
            Self::Dev(level) => *level,
            _ => DebugLevel::Low,
        }
    }

    pub fn date_specifier(&self) -> DateTimeSpecifier {
        if self.debug_level().is_high() {
            DateTimeSpecifier::Verbose
        } else {
            DateTimeSpecifier::Short
        }
    }
}

//* Implement PartialEq for WXMode without taking DebugLevel into account
impl PartialEq<WXMode> for WXMode {
    fn eq(&self, other: &WXMode) -> bool {
        matches!(
            (self, other),
            (WXMode::Dev(_), WXMode::Dev(_)) | (WXMode::Prod, WXMode::Prod)
        )
    }
}

fn print_start_info(
    modules: &[WXModule],
    mode: WXMode,
    config: &ProjectConfig,
    start_duration: std::time::Duration,
) {
    let width = 28;
    println!(
        "{}{} Web{} {}",
        "+".bright_black(),
        "-".repeat(3).bright_black(),
        "X".bright_blue(),
        "-".repeat(width - 6 - 3).bright_black()
    );
    let prefix = "|".bright_black();
    // Project Name
    println!("{} {}: {}", prefix, "Project".bold(), config.name);
    // Modules
    if modules.is_empty() {
        println!("{} No modules found", prefix);
        return;
    } else if modules.len() == 1 {
        println!(
            "{} {}: {}",
            prefix,
            "Module".bold(),
            modules[0].path.module_name()
        );
    } else {
        println!("{} {} ({}):", prefix, "Modules".bold(), modules.len());
        let mut names = modules
            .iter()
            .map(|module| module.path.module_name())
            .collect::<Vec<_>>();
        names.sort();
        for name in names.iter() {
            println!("{}   - {}", prefix, name);
        }
    }
    // Mode
    println!(
        "{} {}: {}",
        prefix,
        "Mode".bold(),
        if mode.is_prod() {
            "production"
        } else {
            "development"
        }
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
    println!("{} {}: {:?}", prefix, "Took".bold(), start_duration);
    // Build time
    let now: DateTime<Local> = Local::now();
    let time = now.time().format("%H:%M");
    println!(
        "{} {}: {:?} at {}",
        prefix,
        "Build".bold(),
        now.date_naive(),
        time
    );
    // WebX version
    println!(
        "{} {}: {}",
        prefix,
        "Version".bold(),
        env!("CARGO_PKG_VERSION")
    );
    // WebX homepage
    println!(
        "{} {}: {}",
        prefix,
        "Homepage".bold(),
        env!("CARGO_PKG_HOMEPAGE")
    );
    println!("{}{}", "+".bright_black(), "-".repeat(width).bright_black());
}

/// Run a WebX **project** from the given root path.
///
/// ## Arguments
/// - `root` - The root path of the project.
/// - `mode` - The mode to run in.
pub fn run(root: &Path, mode: WXMode, running: Arc<AtomicBool>) {
    let time_start = Instant::now();
    let config_file = get_project_config_file_path(root);
    let config = load_project_config(&config_file);
    let source_root = if let Some(src) = &config.src {
        root.join(src)
    } else {
        root.to_path_buf()
    };
    let webx_modules = load_modules(&source_root);
    analyze_module_deps(&webx_modules);
    analyze_module_routes(&webx_modules);
    print_start_info(&webx_modules, mode, &config, time_start.elapsed());

    let (rt_tx, rt_rx) = std::sync::mpsc::channel();
    if mode.is_dev() {
        let fw_rt_tx = rt_tx.clone();
        let fw_running = running.clone();
        let fw_hnd =
            std::thread::spawn(move || WXFileWatcher::run(mode, source_root, fw_rt_tx, fw_running));
        let info = WXRuntimeInfo::new(root);
        let runtime_running = running.clone();
        let runtime_hnd = std::thread::spawn(move || {
            let mut runtime = WXRuntime::new(rt_rx, mode, info);
            runtime.load_modules(webx_modules);
            runtime.run(runtime_running)
        });
        let sv_rt_tx = rt_tx.clone();
        let mut server = WXServer::new(mode, config, sv_rt_tx);
        server.run(running).expect("Failed to run server");
        if runtime_hnd.join().is_err() {
            warning(mode, "Failed to stop runtime".into());
        }
        if fw_hnd.join().is_err() {
            warning(mode, "Failed to stop file watcher".into())
        }
    } else {
        // If we are in production mode, run the `server` in main thread.
        let info = WXRuntimeInfo::new(root);
        let runtime_running = running.clone();
        let runtime_hnd = std::thread::spawn(move || {
            let mut runtime = WXRuntime::new(rt_rx, mode, info);
            runtime.load_modules(webx_modules);
            runtime.run(runtime_running);
        });
        let sv_rt_tx = rt_tx.clone();
        let mut server = WXServer::new(mode, config, sv_rt_tx);
        server.run(running).expect("Failed to run server");
        if runtime_hnd.join().is_err() {
            warning(mode, "Failed to stop runtime".into())
        }
    }
    // Check ps info: `ps | ? ProcessName -eq "webx"`
    // On interrupt, all threads are also terminated
}
