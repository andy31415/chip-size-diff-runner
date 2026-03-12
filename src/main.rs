use clap::{Parser, Subcommand};
use std::process::{Command, Stdio};
use std::str;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Build the application
    Build(BuildArgs),
}

#[derive(Parser, Debug)]
struct BuildArgs {
    /// Application to build
    application: String,

    /// Optional tag for the build
    #[arg(short, long)]
    tag: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Build(args) => {
            handle_build(args)?;
        }
    }

    Ok(())
}

fn get_jj_tag() -> Result<Option<String>, Box<dyn std::error::Error>> {
    let output = Command::new("jj")
        .arg("tag")
        .arg("list")
        .arg("-r")
        .arg("@-")
        .output()?;

    if output.status.success() {
        let stdout = str::from_utf8(&output.stdout)?;
        // Expecting output like: <tag_name>: <commit_id>
        let tag = stdout.lines().next().and_then(|line| line.split(':').next()).map(str::trim);
        Ok(tag.map(String::from))
    } else {
        Ok(None)
    }
}

fn handle_build(args: &BuildArgs) -> Result<(), Box<dyn std::error::Error>> {
    let tag = match &args.tag {
        Some(t) => t.clone(),
        None => {
            get_jj_tag()?.ok_or("Error: No --tag provided and no jj tag found at @-")?
        }
    };

    println!("Building application: {}", args.application);
    println!("Using tag: {}", tag);

    let output_dir = format!("out/branch-builds/{}", tag);
    std::fs::create_dir_all(&output_dir)?;

    println!("Output directory: {}", output_dir);

    // Placeholder for actual build command
    execute_build(&args.application, &output_dir)?;

    Ok(())
}

fn execute_build(application: &str, output_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let build_command = format!(
        "source ./scripts/activate.sh >/dev/null && ./scripts/build/build_examples.py --log-level info --target '{}' build --copy-artifacts-to {}",
        application,
        output_dir
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

    let status = command.stdout(Stdio::inherit()).stderr(Stdio::inherit()).status()?;

    if !status.success() {
        return Err(format!("Build command failed with status: {}", status).into());
    }

    println!("Artifacts in: {}", output_dir);
    Ok(())
}
