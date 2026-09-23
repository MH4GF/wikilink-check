# wikilink-check

Dead link checker for Obsidian vaults. The wikilink counterpart of `markdown-link-check`.

- Finds every `[[wikilink]]`, `![[embed]]` and `[text](path.md)` whose target does not exist, using the same resolution rules as Obsidian (verified against `obsidian unresolved` on a 1,500-note vault with zero difference)
- Classifies each unresolved link as **broken**, **unwritten** or **ignored** with deterministic rules, so the counts are reproducible and diffable
- Baseline mode for CI: fails only when a *new* broken link appears, so an existing backlog never blocks a pull request
- Single static binary, no runtime dependencies. Git is optional and only used to tell deleted notes from notes that were never written

```
$ wikilink-check tests/fixtures/mini-vault
files scanned:   8
links found:     32
unresolved:      20

by tier / kind
  broken               6
    templater                2
    missing_media            2
    broken_markdown_link     2
    deleted_note             0
  unwritten            9
    unwritten                9
  ignored              5
    explanatory              3
    readonly_source          2

by source directory
  notes                           12
  .                                2
  docs                             2
  raw                              2
  templates                        2

top targets
      4  Missing Note
      1  ...
      1  ../../Beta
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
      --no-git               Skip git history; deleted notes become unwritten
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

Each unresolved link gets a tier (its state) and a kind (the rule that decided it).

- **broken**: the target should exist and does not. Fix the note. This is what `--baseline` fails on by default
- **unwritten**: the target is a note nobody has written yet. Obsidian treats such links as normal, and so does this tool
- **ignored**: excluded from judgement by configuration, because the file documents the link syntax or is read-only content

| # | Rule | Tier | Kind |
|---|---|---|---|
| 1 | Source matches `explanatory_files`, or target matches `explanatory_patterns` | ignored | `explanatory` |
| 2 | Source matches `readonly_sources` | ignored | `readonly_source` |
| 3 | Target matches `templater_pattern` (`<% ... %>`) | broken | `templater` |
| 4 | Target extension is in `media_extensions` | broken | `missing_media` |
| 5 | Link uses markdown syntax `[text](path)` | broken | `broken_markdown_link` |
| 6 | A file matching the target existed earlier in the history of `HEAD` | broken | `deleted_note` |
| 7 | Everything else | unwritten | `unwritten` |

Rule 6 needs a full clone; on a shallow clone or outside git the tool warns and falls through to rule 7. `--no-git` does the same without the warning.

## Configuration

`.wikilink-check.toml` in the vault root, or `--config`. All keys are optional.

```toml
# Never scanned and never a link target. Hidden paths are always excluded.
exclude = ["node_modules/**", "archive/**"]

# Files that document the link syntax itself; every unresolved link in them is ignored.
explanatory_files = ["docs/style-guide.md", "templates/**"]

# Regexes on the target. A match is ignored.
explanatory_patterns = ['^\.\.\.$', '^(note|page) name$']

# Verbatim copies of external content that are never edited; their links are ignored.
readonly_sources = ["clippings/**"]

# Regex identifying an unexpanded template variable (Templater syntax by default).
templater_pattern = '<%.*%>'

# Extensions treated as media. Defaults cover images, audio, video, PDF and canvas.
media_extensions = ["png", "jpg", "pdf"]

# Tiers whose new entries fail a --baseline run.
fail_on = ["broken"]
```

## Baseline workflow

```
# Once: snapshot the current state and commit the file.
wikilink-check --update-baseline .wikilink-check-baseline.json

# In CI: fail only when a new broken link appears.
wikilink-check --baseline .wikilink-check-baseline.json

# After fixing links: shrink the baseline and commit it.
wikilink-check --update-baseline .wikilink-check-baseline.json
```

Baseline entries are `(source, target, kind)` without line numbers, so editing unrelated lines does not churn the file. Links whose kind changes (an unwritten note whose target turns out to have been deleted) count as new.

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
          archive="wikilink-check-x86_64-unknown-linux-gnu.tar.gz"
          gh release download v2026.09.23.144905 --repo MH4GF/wikilink-check \
            --pattern "${archive}" --pattern "${archive}.sha256"
          shasum -a 256 -c "${archive}.sha256"
          tar -xzf "${archive}"
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
