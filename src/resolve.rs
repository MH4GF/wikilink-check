use std::collections::{HashMap, HashSet};

/// Lookup structure implementing Obsidian's link resolution over a set of file paths.
///
/// Resolution is case-insensitive and tries the target both as written and with `.md`
/// appended. A target without `/` matches any file with that name anywhere in the vault.
/// A target with `/` must match a path from the vault root, a path relative to the linking
/// note (`./`, `../`), or the tail of some path (`[[dir/note]]` matches `a/b/dir/note.md`).
/// A bare filename match is not enough once the target contains a `/`, and a leading `/` is
/// kept as part of the path, so `[[/dir/note]]` only resolves when such a path exists.
#[derive(Debug, Default)]
pub struct FileIndex {
    exact: HashSet<String>,
    by_basename: HashMap<String, Vec<String>>,
}

impl FileIndex {
    pub fn new<I: IntoIterator<Item = String>>(paths: I) -> Self {
        let mut idx = Self::default();
        for p in paths {
            let lower = p.to_lowercase();
            let base = lower.rsplit('/').next().unwrap_or(&lower).to_string();
            idx.by_basename.entry(base).or_default().push(lower.clone());
            idx.exact.insert(lower);
        }
        idx
    }

    pub fn is_empty(&self) -> bool {
        self.exact.is_empty()
    }

    /// Whether `target`, written in note `source`, resolves to a file in this index.
    pub fn resolves(&self, source: &str, target: &str) -> bool {
        if target.is_empty() {
            return false;
        }
        candidates(target)
            .iter()
            .any(|c| self.resolves_candidate(source, c))
    }

    fn resolves_candidate(&self, source: &str, target: &str) -> bool {
        let lower = target.to_lowercase();
        if !lower.contains('/') {
            return self.by_basename.contains_key(&lower);
        }
        if let Some(n) = normalize(&lower)
            && self.exact.contains(&n)
        {
            return true;
        }
        let dir = source.rsplit_once('/').map(|(d, _)| d.to_lowercase());
        let joined = match dir {
            Some(d) => format!("{d}/{lower}"),
            None => lower.clone(),
        };
        if let Some(n) = normalize(&joined)
            && self.exact.contains(&n)
        {
            return true;
        }
        if lower.starts_with('/') || lower.split('/').any(|seg| seg == "." || seg == "..") {
            return false;
        }
        let base = lower.rsplit('/').next().unwrap_or(&lower);
        let suffix = format!("/{lower}");
        self.by_basename
            .get(base)
            .is_some_and(|paths| paths.iter().any(|p| p.ends_with(&suffix)))
    }
}

/// The target as written plus, unless it already ends in `.md`, the target with `.md` added.
fn candidates(target: &str) -> Vec<String> {
    if target.to_lowercase().ends_with(".md") {
        vec![target.to_string()]
    } else {
        vec![target.to_string(), format!("{target}.md")]
    }
}

/// Collapse `.` and `..` segments. Returns `None` when `..` would escape the root or the
/// path is absolute.
fn normalize(path: &str) -> Option<String> {
    if path.starts_with('/') {
        return None;
    }
    let mut out: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop()?;
            }
            s => out.push(s),
        }
    }
    Some(out.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index() -> FileIndex {
        FileIndex::new(
            [
                "notes/Alpha.md",
                "journal/daily/2026-01-01.md",
                "a/b/dir/note.md",
                "attachments/img.png",
                "Pricing - x.dev.md",
            ]
            .iter()
            .map(|s| s.to_string()),
        )
    }

    #[test]
    fn bare_basename_case_insensitive() {
        let i = index();
        assert!(i.resolves("x.md", "alpha"));
        assert!(i.resolves("x.md", "Alpha.md"));
        assert!(i.resolves("x.md", "img.png"));
        assert!(i.resolves("x.md", "Pricing - x.dev"));
        assert!(!i.resolves("x.md", "beta"));
    }

    #[test]
    fn slash_requires_path_match() {
        let i = index();
        assert!(i.resolves("x.md", "notes/Alpha"));
        assert!(i.resolves("x.md", "dir/note"));
        assert!(i.resolves("x.md", "b/dir/note.md"));
        assert!(!i.resolves("x.md", "other/Alpha"));
        assert!(!i.resolves("x.md", "wrong/note"));
        assert!(!i.resolves("x.md", "/dir/note"));
        assert!(!i.resolves("x.md", "/notes/Alpha"));
    }

    #[test]
    fn relative_to_source() {
        let i = index();
        assert!(i.resolves("journal/weekly/w.md", "../daily/2026-01-01"));
        assert!(i.resolves("notes/x.md", "./Alpha"));
        assert!(!i.resolves("notes/x.md", "../../Alpha"));
        assert!(!i.resolves("a/b/c.md", "../../../../notes/Alpha"));
    }
}
