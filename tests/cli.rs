use std::path::{Path, PathBuf};
use std::process::Command;

use wikilink_check::baseline::Baseline;
use wikilink_check::config::Config;
use wikilink_check::{RunOptions, check};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mini-vault")
}

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_wikilink-check"))
}

fn run_fixture() -> wikilink_check::report::Report {
    let vault = fixture();
    let config = Config::load_or_default(&vault).unwrap();
    // The fixture lives inside this repository's git history, so the deleted-note rule is
    // exercised separately in `deleted_note_from_git_history` and disabled here.
    check(&vault, &config, &RunOptions { no_git: true }).unwrap()
}

#[test]
fn fixture_report_snapshot() {
    let report = run_fixture();
    insta::assert_json_snapshot!(report.to_json(10));
}

#[test]
fn fixture_classification() {
    let report = run_fixture();
    let kinds = |source: &str| -> Vec<String> {
        report
            .links
            .iter()
            .filter(|l| l.source == source)
            .map(|l| format!("{}={}", l.raw, l.kind.as_str()))
            .collect()
    };

    assert_eq!(
        kinds("docs/SKILL.md"),
        vec![
            "[[wikilink]]=explanatory",
            "[[ファイル名|alias]]=explanatory",
        ]
    );
    assert_eq!(kinds("Placeholders.md"), vec!["[[...]]=explanatory"]);
    assert_eq!(
        kinds("raw/articles/External.md"),
        vec![
            "implementation.md=readonly_source",
            "[[Upstream Ref]]=readonly_source"
        ]
    );
    assert_eq!(
        kinds("templates/daily.md"),
        vec![
            "[[<% tp.date.now(\"YYYY-MM-DD\", -1) %>]]=templater",
            "[[<% tp.date.now(\"YYYY-MM-DD\", 1) %>]]=templater",
        ]
    );
    assert_eq!(kinds("Table.md"), vec!["[[missing/x\\|X]]=unwritten"]);
    assert!(kinds(".hidden/secret.md").is_empty());
    assert!(kinds("ignored/Ignored.md").is_empty());

    let alpha = kinds("notes/Alpha.md");
    assert_eq!(
        alpha,
        vec![
            "[[Missing Note]]=unwritten",
            "[[Missing Note|alias]]=unwritten",
            "[[Missing Note#heading]]=unwritten",
            "[[/notes/Beta]]=unwritten",
            "[[wrong/Beta]]=unwritten",
            "[[../../Beta]]=unwritten",
            "[[.hidden/secret]]=unwritten",
            "![[missing.png]]=missing_media",
            "missing.pdf=missing_media",
            "nope.md=broken_markdown_link",
            "Missing%20Note.md=broken_markdown_link",
            "[[Missing Note]]=unwritten",
        ]
    );
}

#[test]
fn baseline_exit_codes() {
    let dir = tempfile::tempdir().unwrap();
    let baseline = dir.path().join("baseline.json");

    let status = bin()
        .arg(fixture())
        .arg("--no-git")
        .arg("--update-baseline")
        .arg(&baseline)
        .output()
        .unwrap();
    assert!(status.status.success());

    // Everything is in the baseline: exit 0.
    let out = bin()
        .arg(fixture())
        .args(["--no-git", "--format", "json", "--baseline"])
        .arg(&baseline)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["baseline"]["new_failing"].as_array().unwrap().len(), 0);
    assert_eq!(json["baseline"]["fixed"].as_array().unwrap().len(), 0);

    // Remove a broken entry from the baseline: it is now "new" and fails the run.
    let mut b = Baseline::load(&baseline).unwrap();
    let broken = b
        .entries
        .iter()
        .find(|e| e.kind == wikilink_check::classify::Kind::MissingMedia)
        .cloned()
        .unwrap();
    b.entries.remove(&broken);
    b.save(&baseline).unwrap();
    let out = bin()
        .arg(fixture())
        .args(["--no-git", "--baseline"])
        .arg(&baseline)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).contains("new (failing):   1"));

    // Remove only an unwritten entry instead: reported but exit 0.
    let mut b = Baseline::from_links(&run_fixture().links);
    let unwritten = b
        .entries
        .iter()
        .find(|e| e.kind == wikilink_check::classify::Kind::Unwritten)
        .cloned()
        .unwrap();
    b.entries.remove(&unwritten);
    // Also add a stale entry so that "fixed" is exercised.
    b.entries.insert(wikilink_check::baseline::Entry {
        source: "gone.md".into(),
        target: "Whatever".into(),
        kind: wikilink_check::classify::Kind::Unwritten,
    });
    b.save(&baseline).unwrap();
    let out = bin()
        .arg(fixture())
        .args(["--no-git", "--baseline"])
        .arg(&baseline)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("new (other):     1"));
    assert!(text.contains("fixed:           1"));
}

#[test]
fn deleted_note_from_git_history() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.com",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(vault)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    std::fs::write(vault.join("Old.md"), "old\n").unwrap();
    std::fs::create_dir(vault.join("sub")).unwrap();
    std::fs::write(vault.join("sub/Moved.md"), "moved\n").unwrap();
    std::fs::write(
        vault.join("Note.md"),
        "[[Old]] [[Never]] [[sub/Moved]] [[Moved]]\n",
    )
    .unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "--no-verify", "-m", "init"]);
    std::fs::remove_file(vault.join("Old.md")).unwrap();
    std::fs::rename(vault.join("sub/Moved.md"), vault.join("Moved.md")).unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-q", "--no-verify", "-m", "delete and move"]);

    let report = check(vault, &Config::default(), &RunOptions::default()).unwrap();
    assert!(report.git_history_used);
    let kinds: Vec<String> = report
        .links
        .iter()
        .map(|l| format!("{}={}", l.raw, l.kind.as_str()))
        .collect();
    // `[[Moved]]` resolves by basename after the move; `[[sub/Moved]]` no longer matches a
    // path and its old path is in history, so it is a deleted note rather than unwritten.
    assert_eq!(
        kinds,
        vec![
            "[[Old]]=deleted_note",
            "[[Never]]=unwritten",
            "[[sub/Moved]]=deleted_note"
        ]
    );

    let report = check(vault, &Config::default(), &RunOptions { no_git: true }).unwrap();
    assert!(!report.git_history_used);
    assert!(report.links.iter().all(|l| l.kind.as_str() == "unwritten"));
}

#[test]
fn no_git_repository_is_a_warning() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("Note.md"), "[[Nope]]\n").unwrap();
    let report = check(dir.path(), &Config::default(), &RunOptions::default()).unwrap();
    assert!(!report.git_history_used);
    assert_eq!(report.warnings.len(), 1);
    assert_eq!(report.links[0].kind.as_str(), "unwritten");
}
