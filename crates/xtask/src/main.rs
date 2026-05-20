use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "xtask", about = "Workdown build orchestrator")]
struct Cli {
    #[command(subcommand)]
    command: SubCmd,
}

#[derive(Subcommand)]
enum SubCmd {
    /// Build the UI bundle (npm ci + npm run check + npm run build).
    BuildUi,
    /// Full release build: build the UI bundle, then `cargo build --release`.
    Build,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let workspace_root = workspace_root()?;
    match cli.command {
        SubCmd::BuildUi => build_ui(&workspace_root),
        SubCmd::Build => {
            build_ui(&workspace_root)?;
            build_release(&workspace_root)
        }
    }
}

fn build_ui(workspace_root: &Path) -> Result<()> {
    let ui_dir = workspace_root.join("ui");
    if !ui_dir.is_dir() {
        bail!(
            "ui directory not found at {}; the workspace layout has drifted",
            ui_dir.display()
        );
    }
    let npm = locate_npm()?;

    run("npm ci", &npm, &["ci"], &ui_dir)?;
    run("npm run check", &npm, &["run", "check"], &ui_dir)?;
    run("npm run build", &npm, &["run", "build"], &ui_dir)?;
    Ok(())
}

fn build_release(workspace_root: &Path) -> Result<()> {
    let cargo = std::env::var_os("CARGO").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("cargo"));
    run(
        "cargo build --release",
        &cargo,
        &["build", "--release", "--workspace"],
        workspace_root,
    )
}

fn run(label: &str, program: &Path, args: &[&str], working_dir: &Path) -> Result<()> {
    println!("→ {label}");
    let status = Command::new(program)
        .args(args)
        .current_dir(working_dir)
        .status()
        .with_context(|| format!("spawning `{label}`"))?;
    if !status.success() {
        bail!("`{label}` exited with {status}");
    }
    Ok(())
}

fn locate_npm() -> Result<PathBuf> {
    which::which("npm").context(
        "could not find `npm` on PATH. Install Node.js (v20 recommended) — see the project README \
         for the dev workflow. In CI/devcontainer, the node feature handles this for you.",
    )
}

fn workspace_root() -> Result<PathBuf> {
    // xtask's own manifest dir is `<root>/crates/xtask` — the workspace
    // root is two levels up. Resolved via `CARGO_MANIFEST_DIR`, set by
    // cargo when invoking `cargo run -p xtask`.
    let manifest = std::env::var("CARGO_MANIFEST_DIR")
        .context("CARGO_MANIFEST_DIR not set — run xtask via `cargo xtask <cmd>`")?;
    let root = PathBuf::from(manifest)
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .context("xtask manifest dir has no grandparent (workspace layout broken)")?;
    Ok(root)
}
