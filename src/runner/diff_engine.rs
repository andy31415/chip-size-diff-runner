use crate::runner::elf_diff::ElfParser;
use crate::runner::parsers::{GoblinParser, NativeParser, NmParser};
use crate::runner::process::CommandChain;
use crate::runner::symbol_diff;
use eyre::{Result, eyre};
use log::{debug, info};
use std::path::Path;
use std::process::Command;
use which::which;

/// Columns shown by default in csvlens. Omits `Size1`/`Size2` which are rarely
/// useful and waste horizontal space; `Function`, `Type`, and `Size` cover most
/// review workflows.
const CSVLENS_DEFAULT_COLUMNS: &str = "Function|Size$|Type";

/// The diff engine to use for comparison.
#[derive(Debug, PartialEq)]
pub enum DiffEngine {
    Script,
    Nm,
    Native,
    Goblin,
}

impl std::str::FromStr for DiffEngine {
    type Err = eyre::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "script" => Ok(Self::Script),
            "nm" => Ok(Self::Nm),
            "native" => Ok(Self::Native),
            "goblin" => Ok(Self::Goblin),
            other => Err(eyre!(
                "Unknown diff engine '{}'. Valid options: script, nm, native, goblin",
                other
            )),
        }
    }
}

/// The viewer tool to pipe CSV output to.
///
/// Implements [`std::str::FromStr`], so it can be parsed with `.parse()` or
/// `ViewerTool::from_str()`.
///
/// # Examples
///
/// ```
/// use chip_size::runner::diff_engine::ViewerTool;
/// assert!("csvlens".parse::<ViewerTool>().is_ok());
/// assert!("custom:grep chip".parse::<ViewerTool>().is_ok());
/// assert!("unknown".parse::<ViewerTool>().is_err());
/// ```
#[derive(Debug, PartialEq)]
pub enum ViewerTool {
    /// Auto-detect: prefer `vd`, then `csvlens`, then plain table output.
    Default,
    Visidata,
    Csvlens,
    /// Pipe to an arbitrary program (with optional args) that reads CSV from stdin.
    Custom(Vec<String>),
}

impl std::str::FromStr for ViewerTool {
    type Err = eyre::Error;

    /// Parses a viewer name string into a `ViewerTool`.
    ///
    /// Valid values: `default`, `vd`, `visidata`, `csvlens`, `custom:<cmd>`.
    /// For `custom`, arguments are supported: `custom:grep chip` → `grep` with arg `chip`.
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "default" => Ok(Self::Default),
            "vd" | "visidata" => Ok(Self::Visidata),
            "csvlens" => Ok(Self::Csvlens),
            s if s.starts_with("custom:") => {
                let rest = s.trim_start_matches("custom:");
                let parts: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
                if parts.is_empty() {
                    Err(eyre!(
                        r#"custom: viewer requires a program name, e.g. custom:myviewer or custom:"grep chip""#
                    ))
                } else {
                    Ok(Self::Custom(parts))
                }
            }
            other => Err(eyre!(
                "Unknown viewer '{}'. Valid options: default, vd, visidata, csvlens, custom:<name>",
                other
            )),
        }
    }
}

impl ViewerTool {
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
            Self::Custom(parts) => ResolvedViewer::Custom(parts.clone()),
        }
    }
}

#[derive(Debug, PartialEq)]
enum ResolvedViewer {
    Table,
    Visidata,
    Csvlens,
    Custom(Vec<String>),
}

/// Runs the comparison between two artifact files based on the selected engine.
pub fn run_diff(
    from_path: &Path,
    to_path: &Path,
    workdir: &Path,
    diff_engine: &DiffEngine,
    extra_args: &[String],
    viewer: &ViewerTool,
) -> Result<()> {
    if !from_path.exists() {
        return Err(eyre!("From file not found: {}", from_path.display()));
    }
    if !to_path.exists() {
        return Err(eyre!("To file not found: {}", to_path.display()));
    }

    info!(
        "Comparing {} and {} using {:?}",
        from_path.display(),
        to_path.display(),
        diff_engine
    );

    match diff_engine {
        DiffEngine::Script => run_script_diff(from_path, to_path, workdir, extra_args, viewer)?,
        DiffEngine::Nm => {
            let parser = NmParser::default();
            let from_symbols = parser.get_symbols(from_path)?;
            let to_symbols = parser.get_symbols(to_path)?;
            let csv_data = symbol_diff::generate_diff_csv(from_symbols, to_symbols)?;
            pipe_to_viewer(csv_data.as_bytes(), workdir, viewer)?;
        }
        DiffEngine::Native => {
            let parser = NativeParser;
            let from_symbols = parser.get_symbols(from_path)?;
            let to_symbols = parser.get_symbols(to_path)?;
            let csv_data = symbol_diff::generate_diff_csv(from_symbols, to_symbols)?;
            pipe_to_viewer(csv_data.as_bytes(), workdir, viewer)?;
        }
        DiffEngine::Goblin => {
            let parser = GoblinParser;
            let from_symbols = parser.get_symbols(from_path)?;
            let to_symbols = parser.get_symbols(to_path)?;
            let csv_data = symbol_diff::generate_diff_csv(from_symbols, to_symbols)?;
            pipe_to_viewer(csv_data.as_bytes(), workdir, viewer)?;
        }
    }
    Ok(())
}

fn pipe_to_viewer(input: &[u8], workdir: &Path, viewer: &ViewerTool) -> Result<()> {
    let resolved = viewer.resolve();
    let command = match resolved {
        ResolvedViewer::Visidata => {
            let mut cmd = Command::new("vd");
            cmd.current_dir(workdir).arg("-");
            Some(cmd)
        }
        ResolvedViewer::Csvlens => {
            let mut cmd = Command::new("csvlens");
            cmd.current_dir(workdir)
                .args(["--columns", CSVLENS_DEFAULT_COLUMNS]);
            Some(cmd)
        }
        ResolvedViewer::Custom(parts) => {
            let mut cmd = Command::new(&parts[0]);
            cmd.args(&parts[1..]).current_dir(workdir);
            Some(cmd)
        }
        ResolvedViewer::Table => {
            // For table view, we just print the input directly
            println!("{}", String::from_utf8_lossy(input));
            return Ok(());
        }
    };

    if let Some(mut command) = command {
        use std::io::Write;
        let mut child = command.stdin(std::process::Stdio::piped()).spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input)?;
        }
        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(eyre!(
                "Viewer command failed with status {}:
STDOUT: {}
STDERR: {}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    } else {
        Ok(())
    }
}

/// Executes the size difference script to compare the two artifact files.
///
/// Uses `uv run` to execute `scripts/tools/binary_elf_size_diff.py`.
fn run_script_diff(
    from_path: &Path,
    to_path: &Path,
    workdir: &Path,
    extra_args: &[String],
    viewer: &ViewerTool,
) -> Result<()> {
    let mut diff_cmd = Command::new("uv");
    diff_cmd
        .args(["run", "scripts/tools/binary_elf_size_diff.py"])
        .current_dir(workdir);

    // Build the full diff command (including output format and positional paths)
    // before constructing the chain so the chain's first command is immutable
    // once created.
    let chain = if extra_args.is_empty() {
        build_viewer_chain(diff_cmd, from_path, to_path, workdir, viewer)
    } else {
        diff_cmd.args(extra_args).arg(to_path).arg(from_path);
        CommandChain::new(diff_cmd)
    };
    debug!("Executing: {:?}", chain);
    chain.execute()
}

/// Finalises the diff command with the correct `--output` flag and paths, then
/// wraps it in a `CommandChain` with the appropriate viewer piped on the end.
fn build_viewer_chain(
    mut diff_cmd: Command,
    from_path: &Path,
    to_path: &Path,
    workdir: &Path,
    viewer: &ViewerTool,
) -> CommandChain {
    let resolved = viewer.resolve();
    let output_format = if matches!(resolved, ResolvedViewer::Table) {
        "table"
    } else {
        "csv"
    };
    diff_cmd
        .args(["--output", output_format])
        .arg(to_path)
        .arg(from_path);

    match resolved {
        ResolvedViewer::Visidata => {
            let mut vd = Command::new("vd");
            vd.current_dir(workdir).arg("-");
            CommandChain::new(diff_cmd).pipe(vd)
        }
        ResolvedViewer::Csvlens => {
            let mut csvlens = Command::new("csvlens");
            csvlens
                .current_dir(workdir)
                .args(["--columns", CSVLENS_DEFAULT_COLUMNS]);
            CommandChain::new(diff_cmd).pipe(csvlens)
        }
        ResolvedViewer::Custom(parts) => {
            let mut custom = Command::new(&parts[0]);
            custom.args(&parts[1..]).current_dir(workdir);
            CommandChain::new(diff_cmd).pipe(custom)
        }
        ResolvedViewer::Table => CommandChain::new(diff_cmd),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── FromStr parsing ───────────────────────────────────────────────────────

    #[test]
    fn test_parse_known_variants() {
        assert_eq!(
            "default".parse::<ViewerTool>().unwrap(),
            ViewerTool::Default
        );
        assert_eq!("vd".parse::<ViewerTool>().unwrap(), ViewerTool::Visidata);
        assert_eq!(
            "visidata".parse::<ViewerTool>().unwrap(),
            ViewerTool::Visidata
        );
        assert_eq!(
            "csvlens".parse::<ViewerTool>().unwrap(),
            ViewerTool::Csvlens
        );
    }

    #[test]
    fn test_parse_custom_single_arg() {
        assert_eq!(
            "custom:myviewer".parse::<ViewerTool>().unwrap(),
            ViewerTool::Custom(vec!["myviewer".to_string()]),
        );
    }

    #[test]
    fn test_parse_custom_multi_arg() {
        // Simulates --viewer custom:"grep chip" after shell quote stripping.
        assert_eq!(
            "custom:grep chip".parse::<ViewerTool>().unwrap(),
            ViewerTool::Custom(vec!["grep".to_string(), "chip".to_string()]),
        );
    }

    #[test]
    fn test_parse_custom_extra_whitespace() {
        assert_eq!(
            "custom:  grep   -i  foo  ".parse::<ViewerTool>().unwrap(),
            ViewerTool::Custom(vec![
                "grep".to_string(),
                "-i".to_string(),
                "foo".to_string()
            ]),
        );
    }

    #[test]
    fn test_parse_custom_empty_is_error() {
        assert!("custom:".parse::<ViewerTool>().is_err());
        assert!("custom:   ".parse::<ViewerTool>().is_err());
    }

    #[test]
    fn test_parse_unknown_is_error() {
        assert!("".parse::<ViewerTool>().is_err());
        assert!("foobar".parse::<ViewerTool>().is_err());
        assert!("Custom:foo".parse::<ViewerTool>().is_err()); // case-sensitive
    }

    // ── DiffEngine parsing ────────────────────────────────────────────────────

    #[test]
    fn test_parse_diff_engine_known() {
        assert_eq!("script".parse::<DiffEngine>().unwrap(), DiffEngine::Script);
        assert_eq!("nm".parse::<DiffEngine>().unwrap(), DiffEngine::Nm);
        assert_eq!("native".parse::<DiffEngine>().unwrap(), DiffEngine::Native);
        assert_eq!("goblin".parse::<DiffEngine>().unwrap(), DiffEngine::Goblin);
    }

    #[test]
    fn test_parse_diff_engine_unknown() {
        assert!("".parse::<DiffEngine>().is_err());
        assert!("default".parse::<DiffEngine>().is_err());
        assert!("Native".parse::<DiffEngine>().is_err());
    }

    // ── resolve (deterministic variants only) ─────────────────────────────────

    #[test]
    fn test_resolve_visidata() {
        assert_eq!(ViewerTool::Visidata.resolve(), ResolvedViewer::Visidata);
    }

    #[test]
    fn test_resolve_csvlens() {
        assert_eq!(ViewerTool::Csvlens.resolve(), ResolvedViewer::Csvlens);
    }

    #[test]
    fn test_resolve_custom_preserves_args() {
        let parts = vec!["grep".to_string(), "chip".to_string()];
        assert_eq!(
            ViewerTool::Custom(parts.clone()).resolve(),
            ResolvedViewer::Custom(parts),
        );
    }

    #[test]
    fn test_resolve_default_returns_valid_variant() {
        // We can't know which tool is installed in CI, but the result must be
        // one of the three valid fallback variants.
        let resolved = ViewerTool::Default.resolve();
        assert!(matches!(
            resolved,
            ResolvedViewer::Visidata | ResolvedViewer::Csvlens | ResolvedViewer::Table
        ));
    }
}
