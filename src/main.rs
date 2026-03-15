use branch_diff::commands::build::{self, BuildArgs};
use branch_diff::commands::compare::{self, CompareArgs};
use branch_diff::persistence::SessionState;
use clap::{Parser, Subcommand, ValueEnum};
use color_eyre::eyre::{self, Context, Result};
use env_logger::Env;
use log::{debug, error, info};
use std::path::PathBuf;

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
    #[arg(short, long, global = true, default_value_t = LogLevel::Info, ignore_case = true)]
    log_level: LogLevel,
}

/// Log verbosity levels accepted by `--log-level`.
#[derive(ValueEnum, Debug, Clone, Default)]
#[value(rename_all = "lowercase")]
enum LogLevel {
    Off,
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            LogLevel::Off => "off",
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        };
        f.write_str(s)
    }
}

/// Represents the available subcommands for the CLI.
#[derive(Subcommand, Debug)]
enum Commands {
    /// Build the application at a specific tag.
    #[command(visible_alias = "b")]
    Build(BuildArgs),
    /// Compare two build artifacts.
    #[command(visible_alias = "c")]
    Compare(CompareArgs),
}

/// Last-resort workdir when neither `--workdir` nor a saved default is available.
fn hardcoded_default_workdir() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("devel/connectedhomeip")
        .to_string_lossy()
        .to_string()
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    env_logger::Builder::from_env(Env::default().default_filter_or(cli.log_level.to_string()))
        .init();
    color_eyre::install()?;

    if let Err(e) = run_app(&cli) {
        error!("{:?}", e);
        return Err(e);
    }
    Ok(())
}

/// Resolves the workdir, validates it, saves it for future runs, then dispatches.
///
/// The session is loaded once here and passed to the command handler so handlers
/// don't need to load it a second time.
fn run_app(cli: &Cli) -> Result<()> {
    let mut session = SessionState::load().wrap_err("Failed to load session state")?;

    let workdir_str = cli
        .workdir
        .clone()
        .or_else(|| session.workdir.clone())
        .unwrap_or_else(hardcoded_default_workdir);

    let workdir = PathBuf::from(&workdir_str);

    if !workdir.join("scripts/activate.sh").exists() {
        return Err(eyre::eyre!(
            "Invalid workdir: {}. 'scripts/activate.sh' not found.",
            workdir.display()
        ));
    }
    info!("Using working directory: {}", workdir.display());

    // Save the used workdir back to session state immediately so it persists
    // even if the subsequent command fails.
    if session.workdir.as_deref() != Some(workdir_str.as_str()) {
        debug!("Saving new default workdir: {}", workdir_str);
        session.workdir = Some(workdir_str);
        session.save().wrap_err("Failed to save session state")?;
    }

    match &cli.command {
        Commands::Build(args) => {
            build::handle_build(args, &workdir, session).wrap_err("Build command failed")?;
        }
        Commands::Compare(args) => {
            compare::handle_compare(args, &workdir, session).wrap_err("Compare command failed")?;
        }
    }

    Ok(())
}
