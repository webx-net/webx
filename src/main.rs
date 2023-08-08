mod engine;

use clap::{Arg, Command};
use colored::*;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const NAME: &str = "webx";
const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
const AUTHOR: &str = env!("CARGO_PKG_AUTHORS");

fn cli() -> Command {
    Command::new(NAME)
        .bin_name(NAME)
        .version(VERSION)
        .author(AUTHOR)
        .about(DESCRIPTION)
        .subcommand(
            Command::new("run").about("Run the web server").arg(
                Arg::new("prod")
                    .short('p')
                    .long("prod")
                    .help("Run in production mode"),
            ),
        )
        .help_template(format!(
            "{info} - {{about}}\n{author} \n\n{{usage-heading}} {{usage}} \n\n{{all-args}} {{after-help}}",
            info = "{name} {version}".bright_white(),
            author = "Created by {author}".italic()
        ))
        .after_help(format!("For more information, visit: {}.", "https://github.com/WilliamRagstad/webx".bright_black()))
}

fn main() {
    let matches = cli().get_matches();

    if let Some(matches) = matches.subcommand_matches("run") {
        let prod = matches.contains_id("prod");
        let dir = std::env::current_dir().unwrap();
        engine::runner::run(&dir, prod);
    } else {
        cli().print_help().unwrap();
    }
}
