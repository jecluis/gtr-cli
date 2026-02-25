// SPDX-License-Identifier: AGPL-3.0-or-later
// gtr - CLI client for Getting Things Rusty
// Copyright (C) 2026 Joao Eduardo Luis <joao@abysmo.tech>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Wiki-link and URI-scheme parser for inline references.
//!
//! Extracts references from markdown content:
//! - `[[Title]]` — document by title
//! - `[[namespace:Title]]` — document by title within a namespace path
//! - `task://uuid` — task by UUID
//! - `doc://uuid` — document by UUID

use uuid::Uuid;

/// A reference parsed from inline content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRef {
    /// What the reference points to.
    pub target: RefTarget,
    /// The type of reference (always "inline" for parsed links).
    pub ref_type: String,
}

/// The target of a parsed reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefTarget {
    /// `[[Title]]` — a document identified by title.
    DocumentByTitle(String),
    /// `[[ns/path:Title]]` — a document identified by title within a namespace.
    DocumentByTitleInNamespace(String, String),
    /// `task://uuid` — a task identified by UUID.
    TaskById(Uuid),
    /// `doc://uuid` — a document identified by UUID.
    DocumentById(Uuid),
    /// `[[slug]]` — a document identified by slug.
    DocumentBySlug(String),
    /// `[[ns/path:slug]]` — a document by slug within a namespace.
    DocumentBySlugInNamespace(String, String),
}

/// Parse wiki-links and URI-scheme references from markdown content.
///
/// Ignores links inside fenced code blocks (``` or ~~~) and inline
/// code (`...`). Escaped brackets (`\[\[`) are also ignored.
pub fn parse_wiki_links(content: &str) -> Vec<ParsedRef> {
    let mut results = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut in_fenced_block = false;

    for line in &lines {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fenced_block = !in_fenced_block;
            continue;
        }
        if in_fenced_block {
            continue;
        }

        parse_line(line, &mut results);
    }

    results
}

/// Parse a single line for wiki-links and URI references, respecting
/// inline code spans.
fn parse_line(line: &str, results: &mut Vec<ParsedRef>) {
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Skip inline code spans
        if chars[i] == '`' {
            i += 1;
            while i < len && chars[i] != '`' {
                i += 1;
            }
            if i < len {
                i += 1; // skip closing backtick
            }
            continue;
        }

        // Check for escaped brackets: \[\[
        if chars[i] == '\\' && i + 1 < len && chars[i + 1] == '[' {
            i += 2;
            continue;
        }

        // Check for wiki-link: [[...]]
        if chars[i] == '[' && i + 1 < len && chars[i + 1] == '[' {
            i += 2; // skip [[
            let start = i;
            while i < len && !(chars[i] == ']' && i + 1 < len && chars[i + 1] == ']') {
                i += 1;
            }
            if i < len {
                let inner: String = chars[start..i].iter().collect();
                i += 2; // skip ]]
                if let Some(parsed) = parse_wiki_link_inner(&inner) {
                    results.push(parsed);
                }
            }
            continue;
        }

        // Check for task://uuid or doc://uuid
        if i + 7 < len {
            let rest: String = chars[i..].iter().collect();
            if let Some(parsed) = try_parse_uri_ref(&rest) {
                results.push(parsed);
                // Advance past the URI
                let prefix = if rest.starts_with("task://") {
                    "task://"
                } else {
                    "doc://"
                };
                i += prefix.len() + 36; // prefix + UUID length
                continue;
            }
        }

        i += 1;
    }
}

/// Check if text looks like a slug (ends with -{8 hex chars}).
fn is_slug(text: &str) -> bool {
    if text.len() < 10 {
        return false;
    }
    let bytes = text.as_bytes();
    if bytes[text.len() - 9] != b'-' {
        return false;
    }
    text[text.len() - 8..]
        .chars()
        .all(|c| c.is_ascii_hexdigit())
}

/// Parse the inner content of a `[[...]]` wiki-link.
fn parse_wiki_link_inner(inner: &str) -> Option<ParsedRef> {
    let inner = inner.trim();
    if inner.is_empty() {
        return None;
    }

    // Check for namespace:Target pattern
    if let Some(colon_pos) = inner.rfind(':') {
        let ns_part = inner[..colon_pos].trim();
        let target_part = inner[colon_pos + 1..].trim();
        if !ns_part.is_empty() && !target_part.is_empty() {
            let target = if is_slug(target_part) {
                RefTarget::DocumentBySlugInNamespace(ns_part.to_string(), target_part.to_string())
            } else {
                RefTarget::DocumentByTitleInNamespace(ns_part.to_string(), target_part.to_string())
            };
            return Some(ParsedRef {
                target,
                ref_type: "inline".to_string(),
            });
        }
    }

    // Plain slug or title reference
    let target = if is_slug(inner) {
        RefTarget::DocumentBySlug(inner.to_string())
    } else {
        RefTarget::DocumentByTitle(inner.to_string())
    };
    Some(ParsedRef {
        target,
        ref_type: "inline".to_string(),
    })
}

/// Try to parse a `task://uuid` or `doc://uuid` reference at the start
/// of the given string.
fn try_parse_uri_ref(s: &str) -> Option<ParsedRef> {
    for (prefix, make_target) in [
        ("task://", RefTarget::TaskById as fn(Uuid) -> RefTarget),
        ("doc://", RefTarget::DocumentById as fn(Uuid) -> RefTarget),
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            // UUID is 36 chars (8-4-4-4-12)
            if rest.len() >= 36 {
                let uuid_str = &rest[..36];
                if let Ok(uuid) = Uuid::parse_str(uuid_str) {
                    return Some(ParsedRef {
                        target: make_target(uuid),
                        ref_type: "inline".to_string(),
                    });
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_wiki_link() {
        let refs = parse_wiki_links("See [[My Document]] for details.");
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].target,
            RefTarget::DocumentByTitle("My Document".to_string())
        );
        assert_eq!(refs[0].ref_type, "inline");
    }

    #[test]
    fn parse_namespaced_wiki_link() {
        let refs = parse_wiki_links("Read [[research/papers:Attention Is All You Need]].");
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].target,
            RefTarget::DocumentByTitleInNamespace(
                "research/papers".to_string(),
                "Attention Is All You Need".to_string(),
            )
        );
    }

    #[test]
    fn parse_task_uri() {
        let id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let refs = parse_wiki_links("Related: task://550e8400-e29b-41d4-a716-446655440000");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].target, RefTarget::TaskById(id));
    }

    #[test]
    fn parse_doc_uri() {
        let id = Uuid::parse_str("a1b2c3d4-e5f6-7890-abcd-ef0123456789").unwrap();
        let refs = parse_wiki_links("See also doc://a1b2c3d4-e5f6-7890-abcd-ef0123456789.");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].target, RefTarget::DocumentById(id));
    }

    #[test]
    fn parse_multiple_refs_mixed() {
        let content = "Start [[Intro]] then task://550e8400-e29b-41d4-a716-446655440000 \
                        and [[ns:Title]] and doc://a1b2c3d4-e5f6-7890-abcd-ef0123456789.";
        let refs = parse_wiki_links(content);
        assert_eq!(refs.len(), 4);

        assert_eq!(
            refs[0].target,
            RefTarget::DocumentByTitle("Intro".to_string())
        );
        assert_eq!(
            refs[1].target,
            RefTarget::TaskById(Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap())
        );
        assert_eq!(
            refs[2].target,
            RefTarget::DocumentByTitleInNamespace("ns".to_string(), "Title".to_string())
        );
        assert_eq!(
            refs[3].target,
            RefTarget::DocumentById(
                Uuid::parse_str("a1b2c3d4-e5f6-7890-abcd-ef0123456789").unwrap()
            )
        );
    }

    #[test]
    fn ignores_wiki_links_in_fenced_code_blocks() {
        let content = "Before [[visible]].\n```\n[[hidden]]\n```\nAfter [[also visible]].";
        let refs = parse_wiki_links(content);
        assert_eq!(refs.len(), 2);
        assert_eq!(
            refs[0].target,
            RefTarget::DocumentByTitle("visible".to_string())
        );
        assert_eq!(
            refs[1].target,
            RefTarget::DocumentByTitle("also visible".to_string())
        );
    }

    #[test]
    fn ignores_wiki_links_in_tilde_fenced_code_blocks() {
        let content = "Before [[visible]].\n~~~\n[[hidden]]\n~~~\nAfter [[also visible]].";
        let refs = parse_wiki_links(content);
        assert_eq!(refs.len(), 2);
    }

    #[test]
    fn ignores_wiki_links_in_inline_code() {
        let content = "See `[[not a link]]` but [[real link]] is valid.";
        let refs = parse_wiki_links(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].target,
            RefTarget::DocumentByTitle("real link".to_string())
        );
    }

    #[test]
    fn ignores_escaped_brackets() {
        let content = r"Escaped \[\[not a link\]\] but [[real]] is fine.";
        let refs = parse_wiki_links(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].target,
            RefTarget::DocumentByTitle("real".to_string())
        );
    }

    #[test]
    fn empty_content_returns_empty() {
        let refs = parse_wiki_links("");
        assert!(refs.is_empty());
    }

    #[test]
    fn empty_wiki_link_is_ignored() {
        let refs = parse_wiki_links("Empty [[]] is ignored.");
        assert!(refs.is_empty());
    }

    #[test]
    fn invalid_uuid_in_uri_is_ignored() {
        let refs = parse_wiki_links("task://not-a-valid-uuid-here-at-all!!");
        assert!(refs.is_empty());
    }

    #[test]
    fn uri_refs_in_fenced_code_blocks_are_ignored() {
        let content = "```\ntask://550e8400-e29b-41d4-a716-446655440000\n```\n[[visible]]";
        let refs = parse_wiki_links(content);
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].target,
            RefTarget::DocumentByTitle("visible".to_string())
        );
    }

    #[test]
    fn uri_refs_in_inline_code_are_ignored() {
        let refs = parse_wiki_links("See `task://550e8400-e29b-41d4-a716-446655440000` here.");
        assert!(refs.is_empty());
    }

    #[test]
    fn parse_slug_reference() {
        let refs = parse_wiki_links("See [[research-a1b2c3d4]].");
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].target,
            RefTarget::DocumentBySlug("research-a1b2c3d4".to_string())
        );
    }

    #[test]
    fn parse_slug_with_namespace() {
        let refs = parse_wiki_links("See [[notes:faq-12345678]].");
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].target,
            RefTarget::DocumentBySlugInNamespace("notes".to_string(), "faq-12345678".to_string())
        );
    }

    #[test]
    fn title_not_mistaken_for_slug() {
        // Title ending with non-hex chars should NOT be treated as slug
        let refs = parse_wiki_links("See [[my-notes-zzzzzzzz]].");
        assert_eq!(refs.len(), 1);
        assert!(matches!(refs[0].target, RefTarget::DocumentByTitle(_)));
    }

    #[test]
    fn short_title_not_slug() {
        // Short title (< 10 chars) should not be treated as slug
        let refs = parse_wiki_links("See [[FAQ]].");
        assert_eq!(refs.len(), 1);
        assert_eq!(
            refs[0].target,
            RefTarget::DocumentByTitle("FAQ".to_string())
        );
    }
}
