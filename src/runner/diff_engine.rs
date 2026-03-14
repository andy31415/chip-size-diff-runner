use crate::runner::process::CommandChain;
use eyre::{Result, eyre};
use log::{debug, error, info};
use std::path::Path;
use std::process::Command;
use which::which;

/// Executes the size difference script to compare the two artifact files.
///
/// Uses `uv run` to execute `scripts/tools/binary_elf_size_diff.py`.
pub fn run_diff(
    from_path: &Path,
    to_path: &Path,
    workdir: &Path,
    extra_args: &[String],
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
        match which("csvlens") {
            Ok(_) => {
                command_chain.commands[0].args(["--output", "csv"]);
                let mut csvlens_command = Command::new("csvlens");

                // Avoid `Size1` and `Size2` columns as they are useless most of the time (take up
                // vertical space). Function, Type and Size seems to be what I use most.
                csvlens_command
                    .current_dir(workdir)
                    .args(["--columns", "Function|Size$|Type"]);
                command_chain = command_chain.pipe(csvlens_command);
            }
            Err(_) => {
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
