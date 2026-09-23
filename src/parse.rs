use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Syntax {
    /// `[[target]]`, `[[target|alias]]`, `[[target#heading]]`
    Wikilink,
    /// `![[target]]`
    Embed,
    /// `[text](target)` and `![alt](target)`
    Markdown,
}

/// A link as written in a note, before resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawLink {
    /// The link path: alias, heading and block references stripped, URI-decoded.
    pub target: String,
    /// The link exactly as it appears in the source.
    pub raw: String,
    /// 1-based line number.
    pub line: usize,
    /// Byte offset of the link in the document; links are returned in this order.
    pub offset: usize,
    pub syntax: Syntax,
}

/// Extract every internal link from a markdown document.
///
/// Block structure comes from a CommonMark parse so that fenced and indented code blocks,
/// inline code and raw HTML are skipped, which is what Obsidian's metadata cache does.
/// Wikilinks are then found in the remaining text spans; markdown links come from the
/// parser's link and image events. Links in the YAML frontmatter are included because
/// Obsidian resolves them as well.
pub fn extract_links(text: &str) -> Vec<RawLink> {
    let lines = LineIndex::new(text);
    let mut out = Vec::new();

    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let parser = Parser::new_ext(text, options).into_offset_iter();

    // Consecutive text events are merged when their source ranges touch, because the
    // parser splits text at brackets and a wikilink would otherwise straddle two events.
    let mut span: Option<std::ops::Range<usize>> = None;
    let mut in_code = false;
    let flush = |span: &mut Option<std::ops::Range<usize>>, out: &mut Vec<RawLink>| {
        if let Some(r) = span.take() {
            scan_wikilinks(text, r, &lines, out);
        }
    };

    for (event, range) in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                flush(&mut span, &mut out);
                in_code = true;
            }
            Event::End(TagEnd::CodeBlock) => in_code = false,
            Event::Text(_) if in_code => {}
            // Adjacent text events are merged into one source span. Escaped characters
            // such as `\|` arrive as their own event whose range skips the backslash, so
            // the merge tolerates that gap and keeps the escape visible to the scanner.
            Event::Text(_) => {
                span = match span.take() {
                    Some(prev)
                        if range.start >= prev.end
                            && text[prev.end..range.start].bytes().all(|b| b == b'\\') =>
                    {
                        Some(prev.start..range.end)
                    }
                    Some(prev) => {
                        scan_wikilinks(text, prev, &lines, &mut out);
                        Some(range)
                    }
                    None => Some(range),
                };
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            })
            | Event::Start(Tag::Image {
                link_type,
                dest_url,
                ..
            }) => {
                flush(&mut span, &mut out);
                if matches!(link_type, LinkType::Autolink | LinkType::Email) {
                    continue;
                }
                if let Some(target) = markdown_target(&dest_url) {
                    out.push(RawLink {
                        target,
                        raw: dest_url.to_string(),
                        line: lines.line_of(range.start),
                        offset: range.start,
                        syntax: Syntax::Markdown,
                    });
                }
            }
            _ => flush(&mut span, &mut out),
        }
    }
    flush(&mut span, &mut out);

    out.sort_by_key(|l| l.offset);
    out
}

fn scan_wikilinks(
    text: &str,
    range: std::ops::Range<usize>,
    lines: &LineIndex,
    out: &mut Vec<RawLink>,
) {
    let slice = &text[range.clone()];
    let bytes = slice.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] != b'[' || bytes[i + 1] != b'[' {
            i += 1;
            continue;
        }
        let abs = range.start + i;
        if abs > 0 && text.as_bytes()[abs - 1] == b'\\' {
            i += 2;
            continue;
        }
        let Some(close) = slice[i + 2..].find("]]") else {
            break;
        };
        let inner = &slice[i + 2..i + 2 + close];
        // A `[[` inside the candidate means the earlier one was stray.
        if let Some(nested) = inner.find("[[") {
            i += 2 + nested;
            continue;
        }
        let embed = i > 0 && bytes[i - 1] == b'!';
        let raw_start = if embed { i - 1 } else { i };
        let raw = &slice[raw_start..i + 2 + close + 2];
        if let Some(target) = wikilink_target(inner) {
            out.push(RawLink {
                target,
                raw: raw.to_string(),
                line: lines.line_of(range.start + i),
                offset: range.start + i,
                syntax: if embed {
                    Syntax::Embed
                } else {
                    Syntax::Wikilink
                },
            });
        }
        i += 2 + close + 2;
    }
}

/// The path part of a wikilink body: `note|alias`, `note#heading`, `note#^block`,
/// `note\|alias` (inside tables). Returns `None` for links to the current note (`#heading`).
fn wikilink_target(inner: &str) -> Option<String> {
    let path = inner.split("\\|").next().unwrap_or("");
    let path = path.split('|').next().unwrap_or("");
    let path = path.split('#').next().unwrap_or("");
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Some(path.nfc().collect())
}

/// The path part of a markdown link destination, or `None` when it is external.
fn markdown_target(dest: &str) -> Option<String> {
    let dest = dest.trim();
    if dest.is_empty() || dest.starts_with('#') || has_scheme(dest) {
        return None;
    }
    let path = dest.split('#').next().unwrap_or("");
    let decoded = decode_uri(path);
    let path = decoded.trim();
    if path.is_empty() {
        return None;
    }
    Some(path.nfc().collect())
}

/// Percent-decoding with the semantics of JavaScript's `decodeURI`, which Obsidian applies
/// to markdown link destinations: escapes of reserved characters (`;/?:@&=+$,#`) stay
/// encoded, everything else is decoded.
fn decode_uri(s: &str) -> String {
    const RESERVED: &[u8] = b";/?:@&=+$,#";
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            let b = h << 4 | l;
            if RESERVED.contains(&b) {
                out.extend_from_slice(&bytes[i..i + 3]);
            } else {
                out.push(b);
            }
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn has_scheme(dest: &str) -> bool {
    let Some(colon) = dest.find(':') else {
        return false;
    };
    let scheme = &dest[..colon];
    let mut chars = scheme.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

struct LineIndex {
    starts: Vec<usize>,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Self { starts }
    }

    fn line_of(&self, offset: usize) -> usize {
        self.starts.partition_point(|&s| s <= offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets(text: &str) -> Vec<(String, Syntax, usize)> {
        extract_links(text)
            .into_iter()
            .map(|l| (l.target, l.syntax, l.line))
            .collect()
    }

    #[test]
    fn wikilink_forms() {
        let got = targets("[[a]] [[b|alias]] [[c#h]] [[d#^blk]] ![[e.png]] [[#only-heading]]");
        assert_eq!(
            got,
            vec![
                ("a".into(), Syntax::Wikilink, 1),
                ("b".into(), Syntax::Wikilink, 1),
                ("c".into(), Syntax::Wikilink, 1),
                ("d".into(), Syntax::Wikilink, 1),
                ("e.png".into(), Syntax::Embed, 1),
            ]
        );
    }

    #[test]
    fn table_cell_alias() {
        let got = targets("| a | b |\n|---|---|\n| [[x/y\\|Y]] | z |\n");
        assert_eq!(got, vec![("x/y".into(), Syntax::Wikilink, 3)]);
    }

    #[test]
    fn code_is_skipped() {
        let text =
            "before\n\n```\n[[fenced]]\n```\n\n`[[inline]]` and [[real]]\n\n    [[indented]]\n";
        assert_eq!(targets(text), vec![("real".into(), Syntax::Wikilink, 7)]);
    }

    #[test]
    fn markdown_links() {
        let text = "[a](note.md) [b](<sp ace.md>) [c](enc%20oded.md#h) [d](https://x) [e](#h) ![f](img.png) <https://auto>";
        assert_eq!(
            targets(text),
            vec![
                ("note.md".into(), Syntax::Markdown, 1),
                ("sp ace.md".into(), Syntax::Markdown, 1),
                ("enc oded.md".into(), Syntax::Markdown, 1),
                ("img.png".into(), Syntax::Markdown, 1),
            ]
        );
    }

    #[test]
    fn decode_uri_keeps_reserved_escapes() {
        assert_eq!(decode_uri("a%20b%E3%81%82"), "a bあ");
        assert_eq!(
            decode_uri("/login?to=%2Fx%2Fy&z=%3F"),
            "/login?to=%2Fx%2Fy&z=%3F"
        );
        assert_eq!(decode_uri("100%"), "100%");
    }

    #[test]
    fn frontmatter_links() {
        let text = "---\nrelated: \"[[fm-note]]\"\n---\n\nbody [[body-note]]\n";
        assert_eq!(
            targets(text),
            vec![
                ("fm-note".into(), Syntax::Wikilink, 2),
                ("body-note".into(), Syntax::Wikilink, 5)
            ]
        );
    }

    #[test]
    fn escaped_and_stray_brackets() {
        assert_eq!(
            targets("\\[[not-a-link]] [[ [[real]]"),
            vec![("real".into(), Syntax::Wikilink, 1)]
        );
    }
}
