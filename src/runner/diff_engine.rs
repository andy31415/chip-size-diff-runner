use crate::runner::process::CommandChain;
use eyre::{Result, eyre};
use log::{debug, error, info};
use std::path::Path;
use std::process::Command;
use which::which;

/// The viewer tool to pipe CSV output to.
pub enum ViewerTool {
    /// Auto-detect: prefer `vd`, then `csvlens`, then plain table output.
    Default,
    Visidata,
    Csvlens,
    /// Pipe to an arbitrary program that reads CSV from stdin.
    Custom(String),
}

impl ViewerTool {
    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "default" => Ok(Self::Default),
            "vd" | "visidata" => Ok(Self::Visidata),
            "csvlens" => Ok(Self::Csvlens),
            s if s.starts_with("custom:") => {
                let name = s.trim_start_matches("custom:");
                if name.is_empty() {
                    Err(eyre!("custom: viewer requires a program name, e.g. custom:myviewer"))
                } else {
                    Ok(Self::Custom(name.to_string()))
                }
            }
            other => Err(eyre!(
                "Unknown viewer '{}'. Valid options: default, vd, visidata, csvlens, custom:<name>",
                other
            )),
        }
    }

    /// Resolve `Default` to a concrete choice based on what is installed.
    fn resolve(&self) -> ResolvedViewer {
        match self {
            Self::Default => {
                if which("vd").is_ok() {
                    ResolvedViewer::Visidata
                } else if which("csvlens").is_ok() {
                    ResolvedViewer::Csvlens
                } else {
                    ResolvedViewer::Table
                }
            }
            Self::Visidata => ResolvedViewer::Visidata,
            Self::Csvlens => ResolvedViewer::Csvlens,
            Self::Custom(name) => ResolvedViewer::Custom(name.clone()),
        }
    }
}

enum ResolvedViewer {
    Table,
    Visidata,
    Csvlens,
    Custom(String),
}

/// Executes the size difference script to compare the two artifact files.
///
/// Uses `uv run` to execute `scripts/tools/binary_elf_size_diff.py`.
pub fn run_diff(
    from_path: &Path,
    to_path: &Path,
    workdir: &Path,
    extra_args: &[String],
    viewer: &ViewerTool,
) -> Result<()> {
    if !from_path.exists() {
        error!("From file not found: {}", from_path.display());
        return Err(eyre!("From file not found: {}", from_path.display()));
    }
    if !to_path.exists() {
        error!("To file not found: {}", to_path.display());
        return Err(eyre!("To file not found: {}", to_path.display()));
    }

    info!(
        "Comparing {} and {}",
        from_path.display(),
        to_path.display()
    );

    let mut diff_command = Command::new("uv");
    diff_command.args(["run", "scripts/tools/binary_elf_size_diff.py"]);
    diff_command.current_dir(workdir);

    let mut command_chain = CommandChain::new(diff_command);

    if extra_args.is_empty() {
        match viewer.resolve() {
            ResolvedViewer::Visidata => {
                command_chain.commands[0].args(["--output", "csv"]);
                let mut vd_command = Command::new("vd");
                vd_command.current_dir(workdir).arg("-");
                command_chain = command_chain.pipe(vd_command);
            }
            ResolvedViewer::Csvlens => {
                command_chain.commands[0].args(["--output", "csv"]);
                let mut csvlens_command = Command::new("csvlens");

                // Avoid `Size1` and `Size2` columns as they are useless most of the time (take up
                // vertical space). Function, Type and Size seems to be what I use most.
                csvlens_command
                    .current_dir(workdir)
                    .args(["--columns", "Function|Size$|Type"]);
                command_chain = command_chain.pipe(csvlens_command);
            }
            ResolvedViewer::Custom(name) => {
                command_chain.commands[0].args(["--output", "csv"]);
                let mut custom_command = Command::new(&name);
                custom_command.current_dir(workdir);
                command_chain = command_chain.pipe(custom_command);
            }
            ResolvedViewer::Table => {
                command_chain.commands[0].args(["--output", "table"]);
            }
        }
    } else {
        command_chain.commands[0].args(extra_args);
    }

    command_chain.commands[0].arg(to_path);
    command_chain.commands[0].arg(from_path);

    debug!("Running command chain: {:?}", command_chain.commands);
    command_chain.execute()
}
