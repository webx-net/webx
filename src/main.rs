mod analysis;
mod engine;
mod file;
mod reporting;
mod runner;

use std::{
    ops::Add,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use clap::{Arg, ArgAction, Command};
use colored::*;
use reporting::error::error;
use runner::{DebugLevel, WXMode};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = "webx";
const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
const AUTHOR: &str = env!("CARGO_PKG_AUTHORS");
const TIMEOUT_DURATION_DEV: Duration = Duration::from_secs(1);
const TIMEOUT_DURATION_PROD: Duration = Duration::from_secs(30);

fn timeout_duration(mode: WXMode) -> Duration {
    match mode {
        WXMode::Dev(_) => TIMEOUT_DURATION_DEV,
        WXMode::Prod => TIMEOUT_DURATION_PROD,
    }
}

fn cli() -> Command {
    Command::new(NAME)
        .bin_name(NAME)
        .version(VERSION)
        .author(AUTHOR)
        .about(DESCRIPTION)
        .subcommand(
            Command::new("run")
                .about("Run the project web server")
                .arg(
                    Arg::new("project")
                        .help("The project directory, default: current directory")
                        .required(false),
                )
                .arg(
                    Arg::new("production")
                        .short('p')
                        .long("prod")
                        .action(ArgAction::SetTrue)
                        .help("Run in production mode"),
                )
                .arg(
                    Arg::new("level")
                        .short('l')
                        .long("level")
                        .required(false)
                        .help("Set the debug verbosity level [1-4], default: 2"),
                ),
        )
        .subcommand(
            Command::new("new")
                .about("Create a new project")
                .arg(
                    Arg::new("name")
                        .help("The name of the project")
                        .required(true),
                )
                .arg(
                    Arg::new("override")
                        .short('o')
                        .long("override")
                        .action(ArgAction::SetTrue)
                        .help("Override existing files"),
                ),
        )
        .subcommand(
            Command::new("test")
                .about("Run the project tests (not implemented)")
                .arg(
                    Arg::new("production")
                        .short('p')
                        .long("prod")
                        .action(ArgAction::SetTrue)
                        .help("Test in production mode"),
                ),
        )
        .color(clap::ColorChoice::Auto)
        .override_usage(format!(
            "{name} [command] (options)",
            name = NAME.bright_white()
        ))
        .help_template(format!(
            "{info} - {{about}}\n\n{{usage-heading}} {{usage}} \n\n{{all-args}} {{after-help}}",
            info = "WebX".bright_white(),
        ))
        .after_help(format!(
            "{}{}\n{}{}",
            "Version: ".bright_black(),
            VERSION.bright_black(),
            "More information on ".bright_black(),
            env!("CARGO_PKG_HOMEPAGE").bright_black()
        ))
}

fn parse_debug_level(matches: &clap::ArgMatches) -> DebugLevel {
    if let Some(value) = matches.get_one::<String>("level") {
        if let Ok(level) = value.parse::<u8>() {
            return DebugLevel::from_u8(level);
        }
    }
    DebugLevel::Medium
}

fn register_ctrlc(mode: WXMode, running: Arc<AtomicBool>) {
    ctrlc::set_handler(move || {
        println!(
            "CTRL+C pressed, shutting down... (up to {:?})",
            timeout_duration(mode)
        );
        running.store(false, Ordering::SeqCst);
        std::thread::sleep(timeout_duration(mode).add(Duration::from_secs(2)));
        println!("This is taking longer than expected, force quitting...");
        std::process::exit(1);
    })
    .expect("Error setting Ctrl-C handler");
}

fn main() {
    let matches = cli().get_matches();

    if let Some(matches) = matches.subcommand_matches("new") {
        let name = match matches.get_one::<String>("name") {
            Some(name) => name.to_owned(),
            None => {
                error("No project name was provided.".to_string(), false);
                cli().print_help().unwrap();
                std::process::exit(1);
            }
        };
        let override_existing = matches.get_flag("override");
        file::project::create_new_project(
            WXMode::MAX,
            name,
            &std::env::current_dir().unwrap(),
            override_existing,
        );
    } else if let Some(matches) = matches.subcommand_matches("run") {
        let mode = if matches.get_flag("production") {
            WXMode::Prod
        } else {
            WXMode::Dev(parse_debug_level(matches))
        };
        let project = if let Some(project) = matches.get_one::<String>("project") {
            PathBuf::from(project)
        } else {
            std::env::current_dir().unwrap()
        };
        let running = Arc::new(AtomicBool::new(true));
        register_ctrlc(mode, running.clone());
        runner::run(&project, mode, running);
    } else if let Some(_matches) = matches.subcommand_matches("test") {
        todo!("Test command not implemented.");
    } else {
        cli().print_help().unwrap();
    }

    println!("Goodbye!");
}
