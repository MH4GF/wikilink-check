use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::classify::{Classified, Kind, Tier};

/// A committed snapshot of the unresolved links that are already known.
///
/// Entries carry no line numbers so that editing a note above a link does not change the
/// baseline. The kind is part of the key because a link moving from `unwritten` to
/// `deleted_note` is a real change.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Baseline {
    pub version: u32,
    pub entries: BTreeSet<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Entry {
    pub source: String,
    pub target: String,
    pub kind: Kind,
}

impl Entry {
    pub fn tier(&self) -> Tier {
        self.kind.tier()
    }
}

impl From<&Classified> for Entry {
    fn from(c: &Classified) -> Self {
        Self {
            source: c.source.clone(),
            target: c.target.clone(),
            kind: c.kind,
        }
    }
}

/// The result of comparing a run against a baseline.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Diff {
    /// Unresolved links absent from the baseline, in tiers that fail the run.
    pub new_failing: Vec<Entry>,
    /// Unresolved links absent from the baseline, in tiers that do not fail the run.
    pub new_other: Vec<Entry>,
    /// Baseline entries that no longer occur.
    pub fixed: Vec<Entry>,
}

impl Baseline {
    pub const VERSION: u32 = 1;

    pub fn from_links(links: &[Classified]) -> Self {
        Self {
            version: Self::VERSION,
            entries: links.iter().map(Entry::from).collect(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading baseline {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parsing baseline {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        std::fs::write(path, text + "\n")
            .with_context(|| format!("writing baseline {}", path.display()))
    }

    pub fn diff(&self, links: &[Classified], fail_on: &[Tier]) -> Diff {
        let current: BTreeSet<Entry> = links.iter().map(Entry::from).collect();
        let mut diff = Diff::default();
        for e in current.difference(&self.entries) {
            if fail_on.contains(&e.tier()) {
                diff.new_failing.push(e.clone());
            } else {
                diff.new_other.push(e.clone());
            }
        }
        diff.fixed = self.entries.difference(&current).cloned().collect();
        diff
    }
}
