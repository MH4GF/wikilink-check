# wikilink-check

Dead link checker for Obsidian vaults. The wikilink counterpart of `markdown-link-check`.

- Finds every `[[wikilink]]`, `![[embed]]` and `[text](path.md)` whose target does not exist, using the same resolution rules as Obsidian (verified against `obsidian unresolved` on a 1,500-note vault with zero difference)
- Classifies each unresolved link as **harmful**, **benign** or a **false positive** with deterministic rules, so the counts are reproducible and diffable
- Baseline mode for CI: fails only when a *new* harmful link appears, so an existing backlog never blocks a pull request
- Single static binary, no runtime dependencies. Git is optional and only used to tell deleted notes from notes that were never written

```
$ wikilink-check ~/vault
files scanned:   1566
links found:     7093
unresolved:      2333

by tier / kind
  harmful           1519
    templater                0
    missing_media           26
    broken_markdown_link    14
    deleted_note          1479
  benign             684
    placeholder            684
  false_positive       130
    explanatory             22
    immutable_source       108

by source directory
  journal                       2083
  wiki                           112
  ...

top targets
     17  2026-05-09
     15  2025-05-12
  ...
```

## Install

Download a binary from [GitHub Releases](https://github.com/MH4GF/wikilink-check/releases) (`x86_64-unknown-linux-gnu`, `aarch64-apple-darwin`), or build from source:

```
cargo install --git https://github.com/MH4GF/wikilink-check
```

## Usage

```
wikilink-check [PATH] [OPTIONS]

  PATH                       Vault root (default: current directory)
  -c, --config <FILE>        Config file (default: <vault>/.wikilink-check.toml if present)
  -f, --format <text|json>   Output format (default: text)
      --baseline <FILE>      Compare with a baseline; exit 1 on new failing links
      --update-baseline <FILE>  Write the current unresolved links as the baseline
      --top <N>              Number of most frequent targets to show (default: 10)
      --list                 List every unresolved link (text output)
      --no-git               Skip git history; deleted notes become placeholders
```

Exit codes: `0` no new failing links (or no baseline given), `1` new failing links, `2` error.

## How links are resolved

The rules were derived by comparing against Obsidian's own metadata cache (`obsidian unresolved verbose format=json`, Obsidian CLI 1.12+). `scripts/oracle-diff.py` reproduces that comparison.

- Hidden files and directories (`.obsidian`, `.git`, `.claude`, ...) are invisible: never scanned, never a valid target. `.gitignore` is not consulted, because Obsidian does not consult it either
- Matching is case-insensitive. A target is tried as written and with `.md` appended
- `[[note]]` without `/` matches a file with that name anywhere in the vault
- `[[dir/note]]` with `/` must match a path from the vault root, a path relative to the linking note (`./`, `../`), or the tail of some path (`[[dir/note]]` matches `a/b/dir/note.md`). A bare filename match is not enough, and a leading `/` is kept literally
- Aliases (`|`, `\|` inside tables), headings (`#`) and block references (`#^`) are stripped; `[[#heading]]` links to the current note are ignored
- Markdown link destinations are decoded like JavaScript's `decodeURI` (reserved escapes such as `%2F` stay encoded), and destinations with a URL scheme or a leading `#` are skipped
- Fenced and indented code blocks, inline code and raw HTML are skipped. Links in YAML frontmatter count

## Classification

Rules are evaluated in order; the first match decides.

| # | Rule | Tier | Kind |
|---|---|---|---|
| 1 | Source matches `explanatory_files`, or target matches `explanatory_patterns` | false_positive | `explanatory` |
| 2 | Source matches `immutable_sources` | false_positive | `immutable_source` |
| 3 | Target matches `templater_pattern` (`<% ... %>`) | harmful | `templater` |
| 4 | Target extension is in `media_extensions` | harmful | `missing_media` |
| 5 | Link uses markdown syntax `[text](path)` | harmful | `broken_markdown_link` |
| 6 | A file matching the target existed earlier in the history of `HEAD` | harmful | `deleted_note` |
| 7 | Everything else: a note that has not been written yet | benign | `placeholder` |

Rule 6 needs a full clone; on a shallow clone or outside git the tool warns and falls through to rule 7. `--no-git` does the same without the warning.

## Configuration

`.wikilink-check.toml` in the vault root, or `--config`. All keys are optional.

```toml
# Never scanned and never a link target. Hidden paths are always excluded.
exclude = ["node_modules/**", "rawdata/**"]

# Files that document the link syntax itself; every unresolved link in them is a false positive.
explanatory_files = ["claude/skills/**/SKILL.md", "journal/*/+template.md"]

# Regexes on the target. A match is a false positive.
explanatory_patterns = ['^\.\.\.$', '^<[^%].*>$']

# Immutable copies of external content; their broken links cannot be fixed in place.
immutable_sources = ["wiki/raw/articles/**"]

# Regex identifying an unexpanded template variable.
templater_pattern = '<%.*%>'

# Extensions treated as media. Defaults cover images, audio, video, PDF and canvas.
media_extensions = ["png", "jpg", "pdf"]

# Tiers whose new entries fail a --baseline run.
fail_on = ["harmful"]
```

## Baseline workflow

```
# Once: snapshot the current state and commit the file.
wikilink-check --update-baseline .wikilink-check-baseline.json

# In CI: fail only when a new harmful link appears.
wikilink-check --baseline .wikilink-check-baseline.json

# After fixing links: shrink the baseline and commit it.
wikilink-check --update-baseline .wikilink-check-baseline.json
```

Baseline entries are `(source, target, kind)` without line numbers, so editing unrelated lines does not churn the file. Links whose kind changes (a placeholder whose target turns out to have been deleted) count as new.

## GitHub Actions

```yaml
name: wikilink-check
on:
  pull_request:
    paths: ["**/*.md"]
  push:
    branches: [main]
    paths: ["**/*.md"]

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0 # full history for the deleted-note rule
      - name: install wikilink-check
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          gh release download v2026.09.23.120000 --repo MH4GF/wikilink-check \
            --pattern 'wikilink-check-x86_64-unknown-linux-gnu.tar.gz*'
          shasum -a 256 -c wikilink-check-x86_64-unknown-linux-gnu.tar.gz.sha256
          tar -xzf wikilink-check-x86_64-unknown-linux-gnu.tar.gz
      - run: ./wikilink-check --baseline .wikilink-check-baseline.json
```

## Development

```
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
scripts/oracle-diff.py ~/vault   # requires Obsidian running with the vault open
```

The fixture vault under `tests/fixtures/mini-vault` has one link per rule; `tests/cli.rs` pins the classification and the baseline exit codes.

## License

MIT
