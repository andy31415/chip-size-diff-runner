use clap::{Parser, Subcommand};
use dialoguer::{Select, theme::ColorfulTheme};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::str;
use walkdir::WalkDir;

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
    /// Baseline build file path
    from_file: Option<String>,

    /// Comparison build file path
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

fn find_build_artifacts(workdir: &Path) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let builds_dir = workdir.join("out/branch-builds");
    let mut artifacts = Vec::new();

    if !builds_dir.exists() {
        return Ok(artifacts);
    }

    for entry in WalkDir::new(builds_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(filename) = entry.path().file_name().and_then(|n| n.to_str()) {
                // Basic filter for potential ELF files (no extension or common binary ones)
                if !filename.contains('.') || filename.ends_with(".elf") || filename.ends_with(".bin") {
                    let relative_path = entry.path().strip_prefix(workdir)?.to_string_lossy().to_string();
                    artifacts.push(relative_path);
                }
            }
        }
    }

    // Parse and sort artifacts
    // Group by app_path, then sort by tag
    let mut grouped: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for artifact in artifacts {
        let parts: Vec<&str> = artifact.split('/').collect();
        if parts.len() > 3 {
            let tag = parts[2].to_string();
            let app_path = parts[3..].join("/");
            grouped.entry(app_path).or_default().push((tag, artifact));
        }
    }

    let mut sorted_artifacts = Vec::new();
    for (_, mut builds) in grouped {
        builds.sort_by(|a, b| a.0.cmp(&b.0)); // Sort by tag
        for (_, full_path) in builds {
            sorted_artifacts.push(full_path);
        }
    }

    Ok(sorted_artifacts)
}

fn select_file(prompt: &str, artifacts: &[String]) -> Result<String, Box<dyn std::error::Error>> {
    if artifacts.is_empty() {
        return Err("No build artifacts found to select from.".into());
    }
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(prompt)
        .items(artifacts)
        .default(0)
        .interact()?;
    Ok(artifacts[selection].clone())
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

fn handle_compare(args: &CompareArgs, workdir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let from_file = match &args.from_file {
        Some(f) => f.clone(),
        None => {
            let artifacts = find_build_artifacts(workdir)?;
            select_file("Select BASELINE file (from)", &artifacts)?
        }
    };

    let to_file = match &args.to_file {
        Some(f) => f.clone(),
        None => {
            let artifacts = find_build_artifacts(workdir)?;
            select_file("Select COMPARISON file (to)", &artifacts)?
        }
    };

    let from_path = workdir.join(&from_file);
    let to_path = workdir.join(&to_file);

    run_diff(&from_path, &to_path, workdir)
}
