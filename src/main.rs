use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str;

mod selector;
use selector::BuildArtifacts;

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

#[derive(Parser, Debug)]
struct BuildArgs {
    /// Application to build
    application: String,

    /// Optional tag for the build
    #[arg(short, long)]
    tag: Option<String>,
}

#[derive(Parser, Debug)]
struct CompareArgs {
    /// Baseline build file path (e.g., out/branch-builds/tag/app)
    from_file: Option<String>,

    /// Comparison build file path (e.g., out/branch-builds/tag/app)
    to_file: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let workdir = PathBuf::from(&cli.workdir);
    if !workdir.join("scripts/activate.sh").exists() {
        return Err(format!(
            "Invalid workdir: {}. 'scripts/activate.sh' not found.",
            cli.workdir
        )
        .into());
    }
    println!("Using working directory: {}", workdir.display());

    match &cli.command {
        Commands::Build(args) => {
            handle_build(args, &workdir)?;
        }
        Commands::Compare(args) => {
            handle_compare(args, &workdir)?;
        }
    }

    Ok(())
}

fn get_jj_tag(workdir: &Path) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let output = Command::new("jj")
        .arg("tag")
        .arg("list")
        .arg("-r")
        .arg("@-")
        .current_dir(workdir)
        .output()?;

    if output.status.success() {
        let stdout = str::from_utf8(&output.stdout)?;
        let tag = stdout
            .lines()
            .next()
            .and_then(|line| line.split(':').next())
            .map(str::trim);
        Ok(tag.map(String::from))
    } else {
        Ok(None)
    }
}

fn handle_build(args: &BuildArgs, workdir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let tag = match &args.tag {
        Some(t) => t.clone(),
        None => get_jj_tag(workdir)?.ok_or(
            "Error: No --tag provided and no jj tag found at @- in this repository",
        )?,
    };

    println!("Building application: {}", args.application);
    println!("Using tag: {}", tag);

    let output_dir = workdir.join(format!("out/branch-builds/{}", tag));
    std::fs::create_dir_all(&output_dir)?;

    println!("Output directory: {}", output_dir.display());

    execute_build(&args.application, &output_dir, workdir)?;

    Ok(())
}

fn execute_build(
    application: &str,
    output_dir: &Path,
    workdir: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let output_dir_str = output_dir.to_string_lossy();
    let build_command = format!(
        "source ./scripts/activate.sh >/dev/null && ./scripts/build/build_examples.py --log-level info --target '{}' build --copy-artifacts-to {}",
        application,
        output_dir_str
    );

    let mut command;
    if application.starts_with("linux-x64-") {
        println!("Building on HOST...");
        command = Command::new("bash");
        command.arg("-c").arg(build_command);
    } else {
        println!("Building via PODMAN...");
        command = Command::new("podman");
        command.args([
            "exec",
            "-w",
            "/workspace",
            "bld_vscode",
            "/bin/bash",
            "-c",
            &build_command,
        ]);
    }

    command.current_dir(workdir);
    let status = command.stdout(Stdio::inherit()).stderr(Stdio::inherit()).status()?;

    if !status.success() {
        return Err(format!("Build command failed with status: {}", status).into());
    }

    println!("Artifacts in: {}", output_dir.display());
    Ok(())
}

fn run_diff(from_path: &Path, to_path: &Path, workdir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    if !from_path.exists() {
        return Err(format!("From file not found: {}", from_path.display()).into());
    }
    if !to_path.exists() {
        return Err(format!("To file not found: {}", to_path.display()).into());
    }

    let diff_script = workdir.join("scripts/tools/binary_elf_size_diff.py");

    if !diff_script.exists() {
        return Err(format!("Diff script not found: {}", diff_script.display()).into());
    }

    println!("Comparing {} and {}", from_path.display(), to_path.display());

    let mut command = Command::new("python3");
    command
        .arg(diff_script)
        .arg("--output")
        .arg("csv")
        .arg(&to_path)
        .arg(&from_path);

    command.current_dir(workdir);
    let status = command.stdout(Stdio::inherit()).stderr(Stdio::inherit()).status()?;

    if !status.success() {
        return Err(format!("Diff command failed with status: {}", status).into());
    }
    Ok(())
}

fn parse_artifact_path(path_str: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = path_str.splitn(4, '/').collect();
    if parts.len() == 4 && parts[0] == "out" && parts[1] == "branch-builds" {
        Some((parts[2].to_string(), parts[3].to_string())) // (tag, app_path)
    } else {
        None
    }
}

fn handle_compare(args: &CompareArgs, workdir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let artifacts = BuildArtifacts::find(workdir)?;

    let from_file = match &args.from_file {
        Some(f) => f.clone(),
        None => {
            let app_paths = artifacts.get_app_paths();
            if app_paths.is_empty() {
                return Err("No build artifacts found.".into());
            }
            // TODO: Display available tags per app path
            let selected_app_path = selector::select_string("Select application", &app_paths)?;
            let tags = artifacts.get_tags_for_app(&selected_app_path).unwrap();
            let selected_tag = selector::select_tag("Select BASELINE tag", tags)?;
            selector::build_path(&selected_tag, &selected_app_path)
        }
    };

    let (from_tag, from_app_path) = parse_artifact_path(&from_file)
        .ok_or_else(|| format!("Invalid from_file path format: {}", from_file))?;

    let to_file = match &args.to_file {
        Some(f) => f.clone(),
        None => {
            let tags = artifacts.get_tags_for_app(&from_app_path).unwrap();
            let other_tags: Vec<&String> = tags.iter().filter(|t| t != &&from_tag).collect();
            if other_tags.is_empty() {
                return Err(format!("No other tags found for application: {}", from_app_path).into());
            }
             let selected_tag = selector::select_string("Select COMPARISON tag", &other_tags)?;
            selector::build_path(&selected_tag, &from_app_path)
        }
    };

    let from_path = workdir.join(&from_file);
    let to_path = workdir.join(&to_file);

    run_diff(&from_path, &to_path, workdir)
}
