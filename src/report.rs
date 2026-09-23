use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Serialize;

use crate::baseline::Diff;
use crate::classify::{Classified, Kind, Tier};

#[derive(Debug, Serialize)]
pub struct Report {
    pub files_scanned: usize,
    pub links_total: usize,
    pub unresolved_total: usize,
    pub git_history_used: bool,
    pub warnings: Vec<String>,
    pub by_tier: BTreeMap<String, usize>,
    pub by_kind: BTreeMap<String, usize>,
    /// Unresolved links per top-level directory of the linking note (`.` for vault root).
    pub by_source_dir: BTreeMap<String, usize>,
    /// Most frequent unresolved targets, most frequent first.
    pub top_targets: Vec<TargetCount>,
    pub links: Vec<Classified>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<Diff>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TargetCount {
    pub target: String,
    pub count: usize,
}

impl Report {
    pub fn new(
        files_scanned: usize,
        links_total: usize,
        links: Vec<Classified>,
        git_history_used: bool,
        warnings: Vec<String>,
    ) -> Self {
        let mut by_tier: BTreeMap<String, usize> = Tier::ALL
            .iter()
            .map(|t| (t.as_str().to_string(), 0))
            .collect();
        let mut by_kind: BTreeMap<String, usize> = Kind::ALL
            .iter()
            .map(|k| (k.as_str().to_string(), 0))
            .collect();
        let mut by_source_dir: BTreeMap<String, usize> = BTreeMap::new();
        let mut targets: BTreeMap<&str, usize> = BTreeMap::new();
        for l in &links {
            *by_tier.get_mut(l.tier.as_str()).expect("all tiers present") += 1;
            *by_kind.get_mut(l.kind.as_str()).expect("all kinds present") += 1;
            let dir = l.source.split_once('/').map(|(d, _)| d).unwrap_or(".");
            *by_source_dir.entry(dir.to_string()).or_default() += 1;
            *targets.entry(l.target.as_str()).or_default() += 1;
        }
        let mut top_targets: Vec<TargetCount> = targets
            .into_iter()
            .map(|(t, c)| TargetCount {
                target: t.to_string(),
                count: c,
            })
            .collect();
        top_targets.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.target.cmp(&b.target)));

        Self {
            files_scanned,
            links_total,
            unresolved_total: links.len(),
            git_history_used,
            warnings,
            by_tier,
            by_kind,
            by_source_dir,
            top_targets,
            links,
            baseline: None,
        }
    }

    pub fn to_json(&self, top: usize) -> serde_json::Value {
        let mut v = serde_json::to_value(self).expect("report serializes");
        if let Some(arr) = v.get_mut("top_targets").and_then(|t| t.as_array_mut()) {
            arr.truncate(top);
        }
        v
    }

    pub fn to_text(&self, top: usize, list: bool) -> String {
        let mut s = String::new();
        let _ = writeln!(s, "files scanned:   {}", self.files_scanned);
        let _ = writeln!(s, "links found:     {}", self.links_total);
        let _ = writeln!(s, "unresolved:      {}", self.unresolved_total);
        if !self.git_history_used {
            let _ = writeln!(s, "git history:     not used");
        }
        for w in &self.warnings {
            let _ = writeln!(s, "warning: {w}");
        }

        let _ = writeln!(s, "\nby tier / kind");
        for tier in Tier::ALL {
            let _ = writeln!(
                s,
                "  {:<16}{:>6}",
                tier.as_str(),
                self.by_tier[tier.as_str()]
            );
            for kind in Kind::ALL.iter().filter(|k| k.tier() == tier) {
                let _ = writeln!(
                    s,
                    "    {:<20}{:>6}",
                    kind.as_str(),
                    self.by_kind[kind.as_str()]
                );
            }
        }

        let _ = writeln!(s, "\nby source directory");
        let mut dirs: Vec<_> = self.by_source_dir.iter().collect();
        dirs.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        for (d, c) in dirs {
            let _ = writeln!(s, "  {d:<28}{c:>6}");
        }

        let _ = writeln!(s, "\ntop targets");
        for t in self.top_targets.iter().take(top) {
            let _ = writeln!(s, "  {:>5}  {}", t.count, t.target);
        }

        if list {
            let _ = writeln!(s, "\nunresolved links");
            for l in &self.links {
                let _ = writeln!(
                    s,
                    "  {}:{}  {}  [{}/{}]",
                    l.source,
                    l.line,
                    l.raw,
                    l.tier.as_str(),
                    l.kind.as_str()
                );
            }
        }

        if let Some(d) = &self.baseline {
            let _ = writeln!(s, "\nbaseline");
            let _ = writeln!(s, "  new (failing):   {}", d.new_failing.len());
            for e in &d.new_failing {
                let _ = writeln!(s, "    {}  {}  [{}]", e.source, e.target, e.kind.as_str());
            }
            let _ = writeln!(s, "  new (other):     {}", d.new_other.len());
            for e in &d.new_other {
                let _ = writeln!(s, "    {}  {}  [{}]", e.source, e.target, e.kind.as_str());
            }
            let _ = writeln!(s, "  fixed:           {}", d.fixed.len());
            for e in &d.fixed {
                let _ = writeln!(s, "    {}  {}  [{}]", e.source, e.target, e.kind.as_str());
            }
        }
        s
    }
}
