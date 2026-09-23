use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};

use wikilink_check::baseline::Baseline;
use wikilink_check::config::Config;
use wikilink_check::{RunOptions, check};

/// Dead link checker for Obsidian wikilinks.
///
/// Scans a vault, reports every link whose target does not exist, and classifies each one
/// as harmful, benign or a false positive. With `--baseline`, exits non-zero only when a new
/// harmful link appears that the baseline does not already list.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    /// Vault root. Defaults to the current directory.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Configuration file. Defaults to `<vault>/.wikilink-check.toml` when it exists.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Output format.
    #[arg(short, long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Compare against this baseline and exit 1 if new failing links appear.
    #[arg(long, value_name = "FILE")]
    baseline: Option<PathBuf>,

    /// Write the current unresolved links to this baseline file (created or replaced).
    #[arg(long, value_name = "FILE")]
    update_baseline: Option<PathBuf>,

    /// Number of most frequent targets to show.
    #[arg(long, default_value_t = 10)]
    top: usize,

    /// List every unresolved link in text output.
    #[arg(long)]
    list: bool,

    /// Do not consult git history; deleted notes are then reported as placeholders.
    #[arg(long)]
    no_git: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();
    let vault = cli
        .path
        .canonicalize()
        .with_context(|| format!("vault path {}", cli.path.display()))?;
    let config = match &cli.config {
        Some(p) => Config::load(p)?,
        None => Config::load_or_default(&vault)?,
    };

    let mut report = check(&vault, &config, &RunOptions { no_git: cli.no_git })?;

    if let Some(path) = &cli.update_baseline {
        Baseline::from_links(&report.links).save(path)?;
        eprintln!(
            "baseline written: {} ({} entries)",
            path.display(),
            report.links.len()
        );
    }

    let mut failed = false;
    if let Some(path) = &cli.baseline {
        let baseline = Baseline::load(path)?;
        let diff = baseline.diff(&report.links, &config.fail_on);
        failed = !diff.new_failing.is_empty();
        report.baseline = Some(diff);
    }

    match cli.format {
        Format::Text => print!("{}", report.to_text(cli.top, cli.list)),
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&report.to_json(cli.top))?
        ),
    }

    Ok(if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    })
}
