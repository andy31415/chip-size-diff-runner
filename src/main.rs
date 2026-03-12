use clap::{Parser, Subcommand};
use env_logger::Env;
use log::{error, info, debug};
use std::path::PathBuf;

mod commands;
mod defaults;
mod selector;
mod tag_generator;

use commands::build::{self, BuildArgs};
use commands::compare::{self, CompareArgs};

/// A CLI tool to build and compare application binaries across different tags.
///
/// This tool helps automate the process of building a target application
/// at different source code revisions (identified by jj tags) and then
/// comparing the resulting artifacts, for example, to analyze size differences.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Working directory for all operations.
    ///
    /// This directory should contain the source code and build scripts
    /// (e.g., 'scripts/activate.sh').
    #[arg(short, long, global = true)]
    workdir: Option<String>,

    /// Set the logging level.
    ///
    /// Options: off, error, warn, info, debug, trace
    #[arg(short, long, global = true, default_value = "info")]
    log_level: String,
}

/// Represents the available subcommands for the CLI.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Build the application at a specific tag.
    Build(BuildArgs),
    /// Compare two build artifacts.
    Compare(CompareArgs),
}

/// Determines the default working directory for the application.
///
/// Defaults to "~/devel/connectedhomeip".
fn hardcoded_default_workdir() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("devel/connectedhomeip")
        .to_string_lossy()
        .to_string()
}

/// The main entry point of the application.
///
/// Parses command line arguments, validates the working directory,
/// and dispatches to the appropriate subcommand handler.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    env_logger::Builder::from_env(Env::default().default_filter_or(&cli.log_level)).init();

    let mut defaults = defaults::load_defaults()?;

    let workdir_str = cli
        .workdir
        .clone()
        .or_else(|| defaults.workdir.clone())
        .unwrap_or_else(hardcoded_default_workdir);

    let workdir = PathBuf::from(&workdir_str);

    if !workdir.join("scripts/activate.sh").exists() {
        error!(
            "Invalid workdir: {}. 'scripts/activate.sh' not found.",
            workdir.display()
        );
        return Err(format!(
            "Invalid workdir: {}. 'scripts/activate.sh' not found.",
            workdir.display()
        )
        .into());
    }
    info!("Using working directory: {}", workdir.display());

    // Save the used workdir back to defaults
    if defaults.workdir.as_deref() != Some(workdir_str.as_str()) {
        debug!("Saving new default workdir: {}", workdir_str);
        defaults.workdir = Some(workdir_str);
        defaults::save_defaults(&defaults)?;
    }

    match &cli.command {
        Commands::Build(args) => {
            build::handle_build(args, &workdir)?;
        }
        Commands::Compare(args) => {
            compare::handle_compare(args, &workdir)?;
        }
    }

    Ok(())
}
