use std::path::Path;

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use unicode_normalization::UnicodeNormalization;
use walkdir::WalkDir;

use crate::config::Config;

/// Files found in the vault, as `/`-separated paths relative to the vault root.
#[derive(Debug, Default)]
pub struct VaultFiles {
    /// Every visible file, used as the link resolution index.
    pub all: Vec<String>,
    /// Markdown files that are scanned for links.
    pub markdown: Vec<String>,
}

pub fn build_globset(patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        b.add(Glob::new(p).with_context(|| format!("invalid glob {p:?}"))?);
    }
    Ok(b.build()?)
}

/// Walk the vault the way Obsidian indexes it: every file except hidden ones, regardless of
/// `.gitignore`, minus the configured `exclude` globs.
pub fn scan(vault: &Path, config: &Config) -> Result<VaultFiles> {
    let exclude = build_globset(&config.exclude)?;
    let mut files = VaultFiles::default();

    let walker = WalkDir::new(vault)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || !is_hidden(e.file_name().to_string_lossy().as_ref()));

    for entry in walker {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(vault)
            .expect("walkdir yields paths under the root")
            .to_string_lossy()
            .replace('\\', "/")
            .nfc()
            .collect::<String>();
        if !is_visible(&rel, &exclude) {
            continue;
        }
        if rel.to_ascii_lowercase().ends_with(".md") {
            files.markdown.push(rel.clone());
        }
        files.all.push(rel);
    }

    files.all.sort();
    files.markdown.sort();
    Ok(files)
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.')
}

/// Whether a vault-relative path takes part in resolution: no hidden segment and not
/// matched by the configured `exclude` globs. Applied to the live tree and to git history
/// alike so that both indexes follow the same rules.
pub fn is_visible(rel: &str, exclude: &GlobSet) -> bool {
    !rel.split('/').any(is_hidden) && !exclude.is_match(rel)
}
