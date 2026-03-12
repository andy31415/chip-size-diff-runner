use clap::{Parser, Subcommand};
use log::{error, info};
use std::path::PathBuf;

mod commands;
mod selector;

use commands::build::{self, BuildArgs};
use commands::compare::{self, CompareArgs};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Working directory for all operations
    #[arg(short, long, global = true, default_value_t = default_workdir())]
    workdir: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Build the application
    Build(BuildArgs),
    /// Compare two builds
    Compare(CompareArgs),
}

fn default_workdir() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("devel/connectedhomeip")
        .to_string_lossy()
        .to_string()
}

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
