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

//! URL content extraction for `--from` and `--bookmark` task creation.
//!
//! Fetches a URL and extracts metadata (title, description, site name) to
//! auto-populate task title and body.  Uses a driver-based architecture so
//! future site-specific extractors (GitHub, YouTube, …) can be added.

mod generic;

use std::time::Duration;

use tracing::{debug, warn};

/// Extracted content from a URL.
pub struct UrlContent {
    /// Page title (from og:title or `<title>`).
    pub title: Option<String>,
    /// Page description (from og:description or `<meta name="description">`).
    pub description: Option<String>,
    /// Site name (from og:site_name or derived from host).
    pub site_name: Option<String>,
    /// Ready-to-use markdown body for the task.
    pub body_markdown: String,
}

/// Trait for site-specific content extractors.
///
/// Each driver declares which URLs it handles via [`matches`] and
/// extracts structured content from raw HTML via [`extract`].
pub trait UrlDriver: Send + Sync {
    fn matches(&self, url: &url::Url) -> bool;
    fn extract(&self, url: &url::Url, html: &str) -> UrlContent;
}

/// Fetch a URL and extract structured content.
///
/// Makes an HTTP GET with a 10-second timeout, then runs the HTML through
/// registered drivers (falling back to the generic HTML meta extractor).
/// Returns `None` if the fetch itself fails.
pub async fn fetch_url_content(url_str: &str) -> Option<UrlContent> {
    let parsed = url::Url::parse(url_str).ok()?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .ok()?;

    debug!("Fetching URL: {url_str}");
    let response = match client.get(url_str).send().await {
        Ok(resp) => resp,
        Err(e) => {
            warn!("Failed to fetch URL {url_str}: {e}");
            return None;
        }
    };

    if !response.status().is_success() {
        warn!("URL {url_str} returned status {}", response.status());
        return None;
    }

    let html = match response.text().await {
        Ok(text) => text,
        Err(e) => {
            warn!("Failed to read response body from {url_str}: {e}");
            return None;
        }
    };

    // Try drivers in order; only GenericDriver for now.
    let drivers: Vec<Box<dyn UrlDriver>> = vec![Box::new(generic::GenericDriver)];

    for driver in &drivers {
        if driver.matches(&parsed) {
            return Some(driver.extract(&parsed, &html));
        }
    }

    // GenericDriver matches everything, so we should never get here,
    // but just in case:
    Some(generic::GenericDriver.extract(&parsed, &html))
}

/// Build a fallback [`UrlContent`] when the fetch fails entirely.
///
/// Uses the raw URL as title and a minimal body.
pub fn fallback_content(url_str: &str) -> UrlContent {
    let host = url::Url::parse(url_str)
        .ok()
        .and_then(|u| u.host_str().map(String::from));
    let site = host.as_deref().unwrap_or("unknown");

    UrlContent {
        title: None,
        description: None,
        site_name: Some(site.to_string()),
        body_markdown: format!("Source: <{url_str}>"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_content_uses_url() {
        let content = fallback_content("https://example.com/page");
        assert!(content.title.is_none());
        assert_eq!(content.site_name.as_deref(), Some("example.com"));
        assert!(content.body_markdown.contains("https://example.com/page"));
    }

    #[test]
    fn fallback_content_handles_bad_url() {
        let content = fallback_content("not-a-url");
        assert_eq!(content.site_name.as_deref(), Some("unknown"));
    }
}
