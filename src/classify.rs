use anyhow::{Context, Result};
use globset::GlobSet;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::parse::{RawLink, Syntax};
use crate::scan::build_globset;

/// The state of an unresolved link, which is what the report and `fail_on` are about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// The target should exist and does not: the note needs fixing.
    Broken,
    /// The target is a note that has not been written yet, which Obsidian treats as normal.
    Unwritten,
    /// Excluded from judgement by configuration: syntax documentation or read-only content.
    Ignored,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::Broken, Tier::Unwritten, Tier::Ignored];

    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Broken => "broken",
            Tier::Unwritten => "unwritten",
            Tier::Ignored => "ignored",
        }
    }
}

/// The rule that produced the tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Explanatory,
    ReadonlySource,
    Templater,
    MissingMedia,
    BrokenMarkdownLink,
    DeletedNote,
    Unwritten,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Templater,
        Kind::MissingMedia,
        Kind::BrokenMarkdownLink,
        Kind::DeletedNote,
        Kind::Unwritten,
        Kind::Explanatory,
        Kind::ReadonlySource,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Explanatory => "explanatory",
            Kind::ReadonlySource => "readonly_source",
            Kind::Templater => "templater",
            Kind::MissingMedia => "missing_media",
            Kind::BrokenMarkdownLink => "broken_markdown_link",
            Kind::DeletedNote => "deleted_note",
            Kind::Unwritten => "unwritten",
        }
    }

    pub fn tier(self) -> Tier {
        match self {
            Kind::Explanatory | Kind::ReadonlySource => Tier::Ignored,
            Kind::Templater | Kind::MissingMedia | Kind::BrokenMarkdownLink | Kind::DeletedNote => {
                Tier::Broken
            }
            Kind::Unwritten => Tier::Unwritten,
        }
    }
}

/// An unresolved link with its verdict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Classified {
    pub source: String,
    pub line: usize,
    pub target: String,
    pub raw: String,
    pub syntax: Syntax,
    pub tier: Tier,
    pub kind: Kind,
}

pub struct Classifier {
    explanatory_files: GlobSet,
    explanatory_patterns: Vec<Regex>,
    readonly_sources: GlobSet,
    templater: Regex,
    media_extensions: Vec<String>,
}

impl Classifier {
    pub fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            explanatory_files: build_globset(&config.explanatory_files)?,
            explanatory_patterns: config
                .explanatory_patterns
                .iter()
                .map(|p| {
                    Regex::new(p)
                        .with_context(|| format!("invalid explanatory_patterns entry {p:?}"))
                })
                .collect::<Result<_>>()?,
            readonly_sources: build_globset(&config.readonly_sources)?,
            templater: Regex::new(&config.templater_pattern)
                .context("invalid templater_pattern")?,
            media_extensions: config
                .media_extensions
                .iter()
                .map(|e| e.to_lowercase())
                .collect(),
        })
    }

    /// Rules are evaluated in order; the first match wins.
    pub fn classify(&self, source: &str, link: &RawLink, existed_in_history: bool) -> (Tier, Kind) {
        let kind = if self.explanatory_files.is_match(source)
            || self
                .explanatory_patterns
                .iter()
                .any(|r| r.is_match(&link.target))
        {
            Kind::Explanatory
        } else if self.readonly_sources.is_match(source) {
            Kind::ReadonlySource
        } else if self.templater.is_match(&link.target) {
            Kind::Templater
        } else if self.is_media(&link.target) {
            Kind::MissingMedia
        } else if link.syntax == Syntax::Markdown {
            Kind::BrokenMarkdownLink
        } else if existed_in_history {
            Kind::DeletedNote
        } else {
            Kind::Unwritten
        };
        (kind.tier(), kind)
    }

    fn is_media(&self, target: &str) -> bool {
        let base = target.rsplit('/').next().unwrap_or(target);
        base.rsplit_once('.').is_some_and(|(_, ext)| {
            self.media_extensions
                .iter()
                .any(|e| e == &ext.to_lowercase())
        })
    }
}
