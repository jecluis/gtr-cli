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

//! Path-based name resolution for projects and namespaces.

use crate::Result;
use crate::cache::TaskCache;

/// Resolve a project identifier to its UUID.
///
/// Resolution strategy:
/// 1. Valid UUID -- return directly
/// 2. Contains `/` -- walk hierarchy path segments
/// 3. Bare name -- search for a unique match among all projects
/// 4. Ambiguous -- error listing the full paths of all matches
pub fn resolve_project(cache: &TaskCache, input: &str) -> Result<String> {
    // 1. Already a UUID?
    if uuid::Uuid::parse_str(input).is_ok() {
        return Ok(input.to_string());
    }

    let projects = cache.list_projects()?;

    // 2. Path-based resolution (e.g. "parent/child")
    if input.contains('/') {
        let segments: Vec<&str> = input.split('/').collect();
        return resolve_path_segments(cache, &projects, &segments, "project");
    }

    // 3. Bare name search
    let matches: Vec<_> = projects.iter().filter(|p| p.name == input).collect();

    match matches.len() {
        0 => Err(crate::Error::ProjectNotFound(format!(
            "no project named '{input}'"
        ))),
        1 => Ok(matches[0].id.clone()),
        _ => {
            let paths: Vec<String> = matches
                .iter()
                .filter_map(|p| {
                    let chain = cache.get_project_id_path(&p.id).ok()?;
                    let names: Vec<String> = chain
                        .iter()
                        .filter_map(|id| projects.iter().find(|pp| pp.id == *id))
                        .map(|pp| pp.name.clone())
                        .collect();
                    Some(names.join("/"))
                })
                .collect();
            Err(crate::Error::InvalidInput(format!(
                "ambiguous project name '{input}'; matches: {}",
                paths.join(", ")
            )))
        }
    }
}

/// Resolve a namespace identifier to its UUID.
///
/// Same strategy as `resolve_project` but walks the namespace hierarchy.
pub fn resolve_namespace(cache: &TaskCache, input: &str) -> Result<String> {
    // 1. Already a UUID?
    if uuid::Uuid::parse_str(input).is_ok() {
        return Ok(input.to_string());
    }

    let namespaces = cache.list_namespaces()?;

    // 2. Path-based resolution
    if input.contains('/') {
        let segments: Vec<&str> = input.split('/').collect();
        return resolve_namespace_path(cache, &namespaces, &segments);
    }

    // 3. Bare name search
    let matches: Vec<_> = namespaces.iter().filter(|ns| ns.name == input).collect();

    match matches.len() {
        0 => Err(crate::Error::InvalidInput(format!(
            "no namespace named '{input}'"
        ))),
        1 => Ok(matches[0].id.clone()),
        _ => {
            let paths: Vec<String> = matches
                .iter()
                .filter_map(|ns| {
                    let chain = cache.get_namespace_path(&ns.id).ok()?;
                    let names: Vec<String> = chain
                        .iter()
                        .filter_map(|id| namespaces.iter().find(|n| n.id == *id))
                        .map(|n| n.name.clone())
                        .collect();
                    Some(names.join("/"))
                })
                .collect();
            Err(crate::Error::InvalidInput(format!(
                "ambiguous namespace name '{input}'; matches: {}",
                paths.join(", ")
            )))
        }
    }
}

/// Walk project hierarchy path segments to find the target.
fn resolve_path_segments(
    _cache: &TaskCache,
    projects: &[crate::cache::CachedProject],
    segments: &[&str],
    _kind: &str,
) -> Result<String> {
    if segments.is_empty() {
        return Err(crate::Error::InvalidInput("empty path".to_string()));
    }

    // Find the root segment (no parent or parent is meta-root)
    let first = segments[0];
    let mut candidates: Vec<&crate::cache::CachedProject> = projects
        .iter()
        .filter(|p| {
            p.name == first
                && (p.parent_id.is_none()
                    || p.parent_id.as_deref() == Some(TaskCache::meta_root_id()))
        })
        .collect();

    if candidates.is_empty() {
        return Err(crate::Error::ProjectNotFound(format!(
            "no root project named '{first}'"
        )));
    }

    // Walk remaining segments
    for &seg in &segments[1..] {
        let parent_ids: Vec<String> = candidates.iter().map(|c| c.id.clone()).collect();
        candidates = projects
            .iter()
            .filter(|p| {
                p.name == seg
                    && p.parent_id
                        .as_ref()
                        .is_some_and(|pid| parent_ids.contains(pid))
            })
            .collect();

        if candidates.is_empty() {
            return Err(crate::Error::ProjectNotFound(format!(
                "no project named '{seg}' under the given path"
            )));
        }
    }

    match candidates.len() {
        1 => Ok(candidates[0].id.clone()),
        _ => {
            let ids: Vec<String> = candidates.iter().map(|c| c.id.clone()).collect();
            Err(crate::Error::InvalidInput(format!(
                "ambiguous path; {} matches: {}",
                ids.len(),
                ids.join(", ")
            )))
        }
    }
}

/// Walk namespace hierarchy path segments.
fn resolve_namespace_path(
    _cache: &TaskCache,
    namespaces: &[crate::cache::CachedNamespace],
    segments: &[&str],
) -> Result<String> {
    if segments.is_empty() {
        return Err(crate::Error::InvalidInput("empty path".to_string()));
    }

    let first = segments[0];
    let mut candidates: Vec<&crate::cache::CachedNamespace> = namespaces
        .iter()
        .filter(|ns| ns.name == first && ns.parent_id.is_none())
        .collect();

    if candidates.is_empty() {
        // Try matching any root-level (could have been reparented)
        candidates = namespaces.iter().filter(|ns| ns.name == first).collect();
        if candidates.is_empty() {
            return Err(crate::Error::InvalidInput(format!(
                "no namespace named '{first}'"
            )));
        }
    }

    for &seg in &segments[1..] {
        let parent_ids: Vec<String> = candidates.iter().map(|c| c.id.clone()).collect();
        candidates = namespaces
            .iter()
            .filter(|ns| {
                ns.name == seg
                    && ns
                        .parent_id
                        .as_ref()
                        .is_some_and(|pid| parent_ids.contains(pid))
            })
            .collect();

        if candidates.is_empty() {
            return Err(crate::Error::InvalidInput(format!(
                "no namespace named '{seg}' under the given path"
            )));
        }
    }

    match candidates.len() {
        1 => Ok(candidates[0].id.clone()),
        _ => {
            let ids: Vec<String> = candidates.iter().map(|c| c.id.clone()).collect();
            Err(crate::Error::InvalidInput(format!(
                "ambiguous path; {} matches: {}",
                ids.len(),
                ids.join(", ")
            )))
        }
    }
}
