use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use unicode_normalization::UnicodeNormalization;

/// Every path that has ever existed in the history of `HEAD`, relative to the vault root
/// with `/` separators. Only `HEAD` is walked, not every ref, so that the answer depends on
/// the commit being checked and not on which branches a clone happens to have fetched.
/// Returns `Ok(None)` when the vault is not inside a git repository or the clone is shallow
/// (history would be incomplete and the answer misleading).
pub fn load(vault: &Path) -> Result<Option<Vec<String>>> {
    let inside = git(vault, &["rev-parse", "--is-inside-work-tree"]);
    match inside {
        Ok(out) if out.trim() == "true" => {}
        _ => return Ok(None),
    }
    let shallow = git(vault, &["rev-parse", "--is-shallow-repository"])?;
    if shallow.trim() == "true" {
        return Ok(None);
    }
    let prefix = git(vault, &["rev-parse", "--show-prefix"])?;
    let prefix = prefix.trim().to_string();

    let out = git(
        vault,
        &[
            "-c",
            "core.quotepath=false",
            "log",
            "HEAD",
            "--name-only",
            "--format=",
        ],
    )?;
    let mut paths: Vec<String> = out
        .lines()
        .filter(|l| !l.is_empty())
        .filter_map(|l| l.strip_prefix(prefix.as_str()))
        .map(|l| l.nfc().collect::<String>())
        .collect();
    paths.sort();
    paths.dedup();
    Ok(Some(paths))
}

fn git(vault: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(vault)
        .output()
        .context("running git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
