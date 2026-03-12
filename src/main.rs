use clap::{Parser, Subcommand};
use log::{error, info};
use std::path::PathBuf;

mod commands;
mod selector;

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
    #[arg(short, long, global = true, default_value_t = default_workdir())]
    workdir: String,
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
fn default_workdir() -> String {
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
    env_logger::init();
    let cli = Cli::parse();

    let workdir = PathBuf::from(&cli.workdir);
    if !workdir.join("scripts/activate.sh").exists() {
        error!(
            "Invalid workdir: {}. 'scripts/activate.sh' not found.",
            cli.workdir
        );
        return Err(format!(
            "Invalid workdir: {}. 'scripts/activate.sh' not found.",
            cli.workdir
        )
        .into());
    }
    info!("Using working directory: {}", workdir.display());

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
