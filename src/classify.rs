use anyhow::{Context, Result};
use globset::GlobSet;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::parse::{RawLink, Syntax};
use crate::scan::build_globset;

/// The top-level verdict for an unresolved link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Something is broken and a human should fix the note.
    Harmful,
    /// Expected in a working vault, such as a link to a note that has not been written yet.
    Benign,
    /// Not a link in practice: syntax documentation or content that must not be edited.
    FalsePositive,
}

impl Tier {
    pub const ALL: [Tier; 3] = [Tier::Harmful, Tier::Benign, Tier::FalsePositive];

    pub fn as_str(self) -> &'static str {
        match self {
            Tier::Harmful => "harmful",
            Tier::Benign => "benign",
            Tier::FalsePositive => "false_positive",
        }
    }
}

/// The rule that produced the tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Explanatory,
    ImmutableSource,
    Templater,
    MissingMedia,
    BrokenMarkdownLink,
    DeletedNote,
    Placeholder,
}

impl Kind {
    pub const ALL: [Kind; 7] = [
        Kind::Templater,
        Kind::MissingMedia,
        Kind::BrokenMarkdownLink,
        Kind::DeletedNote,
        Kind::Placeholder,
        Kind::Explanatory,
        Kind::ImmutableSource,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Explanatory => "explanatory",
            Kind::ImmutableSource => "immutable_source",
            Kind::Templater => "templater",
            Kind::MissingMedia => "missing_media",
            Kind::BrokenMarkdownLink => "broken_markdown_link",
            Kind::DeletedNote => "deleted_note",
            Kind::Placeholder => "placeholder",
        }
    }

    pub fn tier(self) -> Tier {
        match self {
            Kind::Explanatory | Kind::ImmutableSource => Tier::FalsePositive,
            Kind::Templater | Kind::MissingMedia | Kind::BrokenMarkdownLink | Kind::DeletedNote => {
                Tier::Harmful
            }
            Kind::Placeholder => Tier::Benign,
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
    immutable_sources: GlobSet,
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
            immutable_sources: build_globset(&config.immutable_sources)?,
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
        } else if self.immutable_sources.is_match(source) {
            Kind::ImmutableSource
        } else if self.templater.is_match(&link.target) {
            Kind::Templater
        } else if self.is_media(&link.target) {
            Kind::MissingMedia
        } else if link.syntax == Syntax::Markdown {
            Kind::BrokenMarkdownLink
        } else if existed_in_history {
            Kind::DeletedNote
        } else {
            Kind::Placeholder
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
