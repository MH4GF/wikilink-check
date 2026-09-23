use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::classify::Tier;

/// Configuration read from `.wikilink-check.toml`.
///
/// Every field is optional; the defaults describe a plain Obsidian vault with no exclusions.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Glob patterns (relative to the vault root) that are neither scanned as sources nor
    /// indexed as link targets. Hidden files and directories are always excluded, matching
    /// Obsidian's own behaviour.
    pub exclude: Vec<String>,
    /// Glob patterns for files whose links are documentation of the link syntax itself.
    /// Every unresolved link in these files is ignored.
    pub explanatory_files: Vec<String>,
    /// Regular expressions matched against the link target. A match ignores the link (for
    /// example `^\.\.\.$`).
    pub explanatory_patterns: Vec<String>,
    /// Glob patterns for files that are verbatim copies of external content and are never
    /// edited. Their unresolved links are ignored because nothing can be done about them.
    pub readonly_sources: Vec<String>,
    /// Regular expression that identifies an unexpanded template variable in a link target.
    pub templater_pattern: String,
    /// Extensions (without the dot) treated as media or attachments.
    pub media_extensions: Vec<String>,
    /// Tiers that make `--baseline` exit non-zero when a new link appears in them.
    pub fail_on: Vec<Tier>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            exclude: Vec::new(),
            explanatory_files: Vec::new(),
            explanatory_patterns: Vec::new(),
            readonly_sources: Vec::new(),
            templater_pattern: r"<%.*%>".to_string(),
            media_extensions: [
                "png", "jpg", "jpeg", "gif", "svg", "webp", "bmp", "avif", "pdf", "mp3", "wav",
                "m4a", "ogg", "flac", "webm", "mp4", "mov", "mkv", "ogv", "canvas", "base",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            fail_on: vec![Tier::Broken],
        }
    }
}

impl Config {
    pub const DEFAULT_FILE: &'static str = ".wikilink-check.toml";

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parsing config {}", path.display()))
    }

    /// Load `<vault>/.wikilink-check.toml` when present, otherwise the defaults.
    pub fn load_or_default(vault: &Path) -> Result<Self> {
        let path = vault.join(Self::DEFAULT_FILE);
        if path.is_file() {
            Self::load(&path)
        } else {
            Ok(Self::default())
        }
    }
}
