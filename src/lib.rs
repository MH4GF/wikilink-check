pub mod baseline;
pub mod classify;
pub mod config;
pub mod history;
pub mod parse;
pub mod report;
pub mod resolve;
pub mod scan;

use std::path::Path;

use anyhow::Result;

use crate::classify::{Classified, Tier};
use crate::config::Config;
use crate::parse::Syntax;
use crate::report::Report;
use crate::resolve::FileIndex;

/// Options controlling a single check run.
#[derive(Debug, Clone, Default)]
pub struct RunOptions {
    /// Skip the git history lookup even when the vault is a git repository.
    pub no_git: bool,
}

/// Scan the vault, resolve every link and classify the unresolved ones.
pub fn check(vault: &Path, config: &Config, opts: &RunOptions) -> Result<Report> {
    let vault_files = scan::scan(vault, config)?;
    let index = FileIndex::new(vault_files.all.iter().cloned());
    let exclude = scan::build_globset(&config.exclude)?;

    let mut warnings = Vec::new();
    let history = if opts.no_git {
        None
    } else {
        match history::load(vault) {
            Ok(Some(paths)) => Some(FileIndex::new(
                paths.into_iter().filter(|p| scan::is_visible(p, &exclude)),
            )),
            Ok(None) => {
                warnings.push(
                    "git history unavailable (not a git repository or shallow clone); \
                     deleted notes are reported as placeholders"
                        .to_string(),
                );
                None
            }
            Err(e) => {
                warnings.push(format!(
                    "git history unavailable ({e}); deleted notes are reported as placeholders"
                ));
                None
            }
        }
    };

    let classifier = classify::Classifier::new(config)?;
    let mut links_total = 0usize;
    let mut unresolved = Vec::new();

    for source in &vault_files.markdown {
        let text = std::fs::read_to_string(vault.join(source))?;
        for link in parse::extract_links(&text) {
            links_total += 1;
            if index.resolves(source, &link.target) {
                continue;
            }
            let existed = history
                .as_ref()
                .is_some_and(|h| h.resolves(source, &link.target));
            let (tier, kind) = classifier.classify(source, &link, existed);
            unresolved.push(Classified {
                source: source.clone(),
                line: link.line,
                target: link.target,
                raw: link.raw,
                syntax: link.syntax,
                tier,
                kind,
            });
        }
    }

    Ok(Report::new(
        vault_files.markdown.len(),
        links_total,
        unresolved,
        history.is_some(),
        warnings,
    ))
}

/// Whether a classified link belongs to one of the tiers that fail the run.
pub fn is_failing(tier: Tier, fail_on: &[Tier]) -> bool {
    fail_on.contains(&tier)
}

/// Convenience used by tests and the oracle script: the (source, target) pairs of every
/// unresolved link, with the `.md` extension stripped the way Obsidian reports them.
pub fn unresolved_pairs(report: &Report) -> Vec<(String, String)> {
    report
        .links
        .iter()
        .map(|l| {
            let t = match l.syntax {
                Syntax::Markdown | Syntax::Wikilink | Syntax::Embed => strip_md(&l.target),
            };
            (l.source.clone(), t)
        })
        .collect()
}

fn strip_md(target: &str) -> String {
    target.strip_suffix(".md").unwrap_or(target).to_string()
}
