use std::io::IsTerminal;
use std::path::Path;

use crate::cli::args::{Args, Command};
use crate::cli::config::{Config, load_config};
use crate::cli::reports::RenderOptions;

mod diagnose;
mod history;
mod migrate;
mod observability;
mod watch;

type CommandResult = Result<(), i32>;
const EXIT_UNHEALTHY: i32 = 1;
const EXIT_ERROR: i32 = 2;

pub async fn run(args: Args) -> CommandResult {
    match args.command {
        Command::Diagnose(args) => diagnose::run(args).await,
        Command::History { command } => history::run(command).await,
        Command::Migrate { config } => migrate::run(config).await,
    }
}

fn load_config_or_exit(path: Option<&Path>) -> Result<Config, i32> {
    load_config(path).map_err(|error| {
        eprintln!("Error: could not load config: {error}");
        EXIT_ERROR
    })
}

fn render_options(verbose: bool) -> RenderOptions {
    RenderOptions {
        verbose,
        width: terminal_size::terminal_size()
            .map(|(width, _)| width.0 as usize)
            .unwrap_or(80),
        color: std::io::stdout().is_terminal(),
    }
}
