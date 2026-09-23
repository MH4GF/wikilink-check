use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::classify::{Classified, Kind, Tier};

/// A committed snapshot of the unresolved links that are already known.
///
/// The file is line oriented: one `source<TAB>target<TAB>kind` per line, sorted, plus `#`
/// comment lines. It is read as a set, so duplicate and unsorted lines are accepted. That
/// makes the file safe to merge with a union strategy when several branches add entries:
/// additions from both sides survive, and a stale line that a union merge resurrects is
/// removed by the next `--update-baseline`.
///
/// Entries carry no line numbers so that editing a note above a link does not change the
/// baseline. The kind is part of the key because a link moving from `unwritten` to
/// `deleted_note` is a real change.
#[derive(Debug, Clone, Default)]
pub struct Baseline {
    pub entries: BTreeSet<Entry>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
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

const HEADER: &str = "# wikilink-check baseline: one known unresolved link per line as source, target, kind (tab separated).\n\
# Regenerate with `wikilink-check --update-baseline <this file>`. Order and duplicates do not matter.\n";

impl Baseline {
    pub fn from_links(links: &[Classified]) -> Self {
        Self {
            entries: links.iter().map(Entry::from).collect(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading baseline {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("parsing baseline {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Self> {
        let mut entries = BTreeSet::new();
        for (i, line) in text.lines().enumerate() {
            let line = line.trim_end_matches('\r');
            if line.trim().is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(3, '\t');
            let (Some(source), Some(target), Some(kind)) =
                (parts.next(), parts.next(), parts.next())
            else {
                bail!(
                    "line {}: expected source, target and kind separated by tabs",
                    i + 1
                );
            };
            let Some(kind) = Kind::parse(kind.trim()) else {
                bail!("line {}: unknown kind {kind:?}", i + 1);
            };
            entries.insert(Entry {
                source: source.to_string(),
                target: target.to_string(),
                kind,
            });
        }
        Ok(Self { entries })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_text())
            .with_context(|| format!("writing baseline {}", path.display()))
    }

    pub fn to_text(&self) -> String {
        let mut s = String::from(HEADER);
        for e in &self.entries {
            let _ = writeln!(s, "{}\t{}\t{}", e.source, e.target, e.kind.as_str());
        }
        s
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_is_order_and_duplicate_tolerant() {
        let text = "# comment\n\nb.md\tX\tunwritten\na.md\tY\tdeleted_note\nb.md\tX\tunwritten\r\n";
        let b = Baseline::parse(text).unwrap();
        assert_eq!(b.entries.len(), 2);
        let again = Baseline::parse(&b.to_text()).unwrap();
        assert_eq!(again.entries, b.entries);
        assert!(b.to_text().starts_with("# wikilink-check baseline"));
    }

    #[test]
    fn parse_rejects_bad_lines() {
        assert!(Baseline::parse("a.md\tX\n").is_err());
        assert!(Baseline::parse("a.md\tX\tnope\n").is_err());
    }
}
