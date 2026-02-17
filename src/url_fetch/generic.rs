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

//! Generic HTML meta-tag extractor.
//!
//! Extracts `og:title`, `og:description`, `og:site_name`, and falls back
//! to `<title>` and `<meta name="description">` when OG tags are absent.

use scraper::{Html, Selector};

use super::{UrlContent, UrlDriver};

/// Generic driver that matches any URL and extracts HTML meta tags.
pub struct GenericDriver;

impl UrlDriver for GenericDriver {
    fn matches(&self, _url: &url::Url) -> bool {
        true
    }

    fn extract(&self, url: &url::Url, html: &str) -> UrlContent {
        let document = Html::parse_document(html);

        let title = extract_og(&document, "og:title")
            .or_else(|| extract_title(&document))
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty());

        let description = extract_og(&document, "og:description")
            .or_else(|| extract_meta_description(&document))
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty());

        let site_name = extract_og(&document, "og:site_name")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| url.host_str().map(String::from));

        let body_markdown = build_body(url.as_str(), &title, &description, &site_name);

        UrlContent {
            title,
            description,
            site_name,
            body_markdown,
        }
    }
}

/// Extract an OpenGraph meta tag value by property name.
fn extract_og(document: &Html, property: &str) -> Option<String> {
    let selector = Selector::parse(&format!("meta[property=\"{property}\"]")).ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(String::from)
}

/// Extract the `<title>` element text.
fn extract_title(document: &Html) -> Option<String> {
    let selector = Selector::parse("title").ok()?;
    document
        .select(&selector)
        .next()
        .map(|el| el.text().collect::<String>())
}

/// Extract `<meta name="description">` content.
fn extract_meta_description(document: &Html) -> Option<String> {
    let selector = Selector::parse("meta[name=\"description\"]").ok()?;
    document
        .select(&selector)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(String::from)
}

/// Build a markdown body from extracted metadata.
fn build_body(
    url: &str,
    title: &Option<String>,
    description: &Option<String>,
    site_name: &Option<String>,
) -> String {
    let mut parts = Vec::new();

    // Heading with link
    let heading = match title {
        Some(t) => format!("## [{t}]({url})"),
        None => format!("## <{url}>"),
    };
    parts.push(heading);

    // Description as blockquote
    if let Some(desc) = description {
        parts.push(String::new());
        parts.push(format!("> {desc}"));
    }

    // Source line
    let source = match site_name {
        Some(s) => format!("Source: {s}"),
        None => format!("Source: <{url}>"),
    };
    parts.push(String::new());
    parts.push(source);

    parts.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_url(s: &str) -> url::Url {
        url::Url::parse(s).unwrap()
    }

    #[test]
    fn extracts_og_tags() {
        let html = r#"
            <html><head>
                <meta property="og:title" content="My Article">
                <meta property="og:description" content="A great article about Rust.">
                <meta property="og:site_name" content="RustBlog">
                <title>My Article - RustBlog</title>
            </head><body></body></html>
        "#;
        let url = parse_url("https://rustblog.com/article");
        let content = GenericDriver.extract(&url, html);

        assert_eq!(content.title.as_deref(), Some("My Article"));
        assert_eq!(
            content.description.as_deref(),
            Some("A great article about Rust.")
        );
        assert_eq!(content.site_name.as_deref(), Some("RustBlog"));
        assert!(content.body_markdown.contains("[My Article]"));
        assert!(content.body_markdown.contains("> A great article"));
        assert!(content.body_markdown.contains("Source: RustBlog"));
    }

    #[test]
    fn falls_back_to_title_and_meta() {
        let html = r#"
            <html><head>
                <title>Fallback Title</title>
                <meta name="description" content="Fallback description.">
            </head><body></body></html>
        "#;
        let url = parse_url("https://example.com/page");
        let content = GenericDriver.extract(&url, html);

        assert_eq!(content.title.as_deref(), Some("Fallback Title"));
        assert_eq!(
            content.description.as_deref(),
            Some("Fallback description.")
        );
        assert_eq!(content.site_name.as_deref(), Some("example.com"));
    }

    #[test]
    fn handles_no_metadata() {
        let html = "<html><head></head><body><p>Hello</p></body></html>";
        let url = parse_url("https://bare.example.org/");
        let content = GenericDriver.extract(&url, html);

        assert!(content.title.is_none());
        assert!(content.description.is_none());
        assert_eq!(content.site_name.as_deref(), Some("bare.example.org"));
        assert!(
            content
                .body_markdown
                .contains("## <https://bare.example.org/>")
        );
    }

    #[test]
    fn trims_whitespace_from_title() {
        let html = "<html><head><title>  Spaced Out  </title></head><body></body></html>";
        let url = parse_url("https://example.com/");
        let content = GenericDriver.extract(&url, html);

        assert_eq!(content.title.as_deref(), Some("Spaced Out"));
    }

    #[test]
    fn empty_title_treated_as_none() {
        let html = "<html><head><title>   </title></head><body></body></html>";
        let url = parse_url("https://example.com/");
        let content = GenericDriver.extract(&url, html);

        assert!(content.title.is_none());
    }

    #[test]
    fn body_format_without_description() {
        let html = "<html><head><title>No Desc</title></head><body></body></html>";
        let url = parse_url("https://example.com/nodesc");
        let content = GenericDriver.extract(&url, html);

        assert!(!content.body_markdown.contains('>'));
        assert!(content.body_markdown.contains("[No Desc]"));
        assert!(content.body_markdown.contains("Source: example.com"));
    }
}
