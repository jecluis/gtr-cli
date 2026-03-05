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

//! Help overlay showing all keybindings in an adaptive multi-column grid.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Widget};

use crate::tui::theme::Theme;

/// Minimum character width for a single column (keys + description + pad).
const MIN_COL_WIDTH: u16 = 38;

/// A logical group of related keybindings.
struct HelpSection {
    title: &'static str,
    bindings: Vec<(&'static str, &'static str)>,
}

impl HelpSection {
    /// Height in rows: 1 title + N bindings.
    fn height(&self) -> usize {
        1 + self.bindings.len()
    }
}

/// State for the help overlay.
pub struct HelpOverlayState {
    page: usize,
}

impl Default for HelpOverlayState {
    fn default() -> Self {
        Self::new()
    }
}

impl HelpOverlayState {
    pub fn new() -> Self {
        Self { page: 0 }
    }

    pub fn next_page(&mut self) {
        self.page += 1;
    }

    pub fn prev_page(&mut self) {
        self.page = self.page.saturating_sub(1);
    }

    pub fn render(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let popup = centered_rect(area, 60, 80);
        Clear.render(popup, buf);

        let sections = help_sections();

        // Reserve 1 row at the bottom for the footer hint line.
        let block_inner_height = popup.height.saturating_sub(2); // border top + bottom
        let content_height = block_inner_height.saturating_sub(1) as usize; // footer
        let inner_width = popup.width.saturating_sub(4); // border + padding

        let num_cols = (inner_width / MIN_COL_WIDTH).max(1) as usize;
        let pages = paginate(&sections, num_cols, content_height);
        let total_pages = pages.len().max(1);
        let current_page = self.page.min(total_pages - 1);

        let title = if total_pages > 1 {
            format!(" help [{}/{}] ", current_page + 1, total_pages)
        } else {
            " help ".to_string()
        };

        let block = Block::bordered()
            .title(title)
            .border_style(theme.border_focused)
            .padding(Padding::horizontal(1));
        let inner = block.inner(popup);
        block.render(popup, buf);

        if inner.height == 0 || inner.width == 0 {
            return;
        }

        // The usable area for columns (everything except the footer row).
        let col_area_height = inner.height.saturating_sub(1);
        let col_area = Rect::new(inner.x, inner.y, inner.width, col_area_height);

        if let Some(page) = pages.get(current_page) {
            render_columns(page, num_cols, theme, col_area, buf);
        }

        // Footer hint line at the bottom of inner area.
        let footer_y = inner.y + inner.height - 1;
        let footer_rect = Rect::new(inner.x, footer_y, inner.width, 1);
        let footer = if total_pages > 1 {
            Line::from(vec![
                Span::styled("← → ", theme.accent),
                Span::raw("page  "),
                Span::styled("Esc ", theme.accent),
                Span::raw("close"),
            ])
        } else {
            Line::from(vec![Span::styled("Esc ", theme.accent), Span::raw("close")])
        };
        footer.render(footer_rect, buf);
    }
}

/// All help sections with their keybindings.
fn help_sections() -> Vec<HelpSection> {
    vec![
        HelpSection {
            title: "Global",
            bindings: vec![
                ("Ctrl-q / Ctrl-c", "Quit immediately"),
                ("Tab", "Toggle sidebar / main"),
                ("?", "Show this help"),
                ("f", "Feels"),
                (":", "Open command bar"),
            ],
        },
        HelpSection {
            title: "Navigation",
            bindings: vec![
                ("j / Down", "Move down"),
                ("k / Up", "Move up"),
                ("Enter / l / Right", "Open / select"),
                ("Esc / h / Left", "Back / cancel"),
                ("PageUp / PageDown", "Scroll page"),
            ],
        },
        HelpSection {
            title: "Compound Keys",
            bindings: vec![
                ("g t", "Go to tasks"),
                ("g d", "Go to documents"),
                ("g p", "Go to projects"),
                ("S s", "Search all"),
                ("S t", "Search tasks"),
                ("S d", "Search documents"),
            ],
        },
        HelpSection {
            title: "Dashboard",
            bindings: vec![
                ("j / k", "Navigate next up"),
                ("Enter", "Open task"),
                ("s", "Start/stop task"),
                ("d", "Mark done"),
            ],
        },
        HelpSection {
            title: "Task List",
            bindings: vec![
                ("/", "Filter by title"),
                ("r", "Toggle recursive"),
                ("n", "New task"),
                ("s", "Toggle start/stop"),
                ("p", "Cycle priority"),
                ("P", "Set progress"),
                ("d", "Mark done"),
                ("x", "Delete"),
                ("u", "Update fields"),
                ("e", "Inline edit"),
                ("E", "Edit in $EDITOR"),
            ],
        },
        HelpSection {
            title: "Detail Views",
            bindings: vec![
                ("] / [", "Next / previous link"),
                ("Enter", "Follow selected link"),
                ("P", "Set progress"),
                ("e", "Inline edit"),
                ("E", "Edit in $EDITOR"),
            ],
        },
        HelpSection {
            title: "Document List",
            bindings: vec![
                ("/", "Filter by title"),
                ("r", "Toggle recursive"),
                ("n", "New document"),
                ("u", "Update fields"),
                ("e", "Inline edit"),
                ("E", "Edit in $EDITOR"),
                ("x", "Delete"),
            ],
        },
        HelpSection {
            title: "Document Detail",
            bindings: vec![
                ("e", "Inline edit"),
                ("E", "Edit in $EDITOR"),
                ("u", "Update fields"),
                ("x", "Delete"),
                ("m", "Move to namespace"),
            ],
        },
        HelpSection {
            title: "Document Editor",
            bindings: vec![
                ("Ctrl-s", "Save"),
                ("Esc", "Cancel / close"),
                ("Tab", "Toggle title / body"),
                ("[[", "Wiki-link picker"),
            ],
        },
        HelpSection {
            title: "Sidebar",
            bindings: vec![
                ("j / k", "Move down / up"),
                ("Enter / l", "Open / select"),
                ("n", "New project/namespace"),
            ],
        },
        HelpSection {
            title: "Commands (:)",
            bindings: vec![
                (":q / :quit", "Quit"),
                (":new <title>", "Create task"),
                (":new project <name>", "Create project"),
                (":new ns <name>", "Create namespace"),
                (":doc new <title>", "Create document"),
                (":search <query>", "Search (or :s)"),
                (":sync", "Sync with server"),
                (":feels [E F]", "Set feels or dialog"),
                (":progress [N]", "Set progress or dialog"),
            ],
        },
    ]
}

/// Split sections into pages, each fitting within the given column count
/// and height constraint.
fn paginate(sections: &[HelpSection], num_cols: usize, max_height: usize) -> Vec<Vec<Vec<usize>>> {
    // Each page is a Vec of columns, each column is a Vec of section indices.
    let mut pages: Vec<Vec<Vec<usize>>> = Vec::new();
    let mut remaining: Vec<usize> = (0..sections.len()).collect();

    while !remaining.is_empty() {
        let mut columns: Vec<Vec<usize>> = (0..num_cols).map(|_| Vec::new()).collect();
        let mut col_heights: Vec<usize> = vec![0; num_cols];
        let mut placed = Vec::new();

        for &idx in &remaining {
            // Section height + 1 blank line gap (except first in column).
            let shortest_col = col_heights
                .iter()
                .enumerate()
                .min_by_key(|(_, h)| *h)
                .map(|(i, _)| i)
                .unwrap_or(0);

            let gap = if columns[shortest_col].is_empty() {
                0
            } else {
                1
            };
            let needed = gap + sections[idx].height();

            if col_heights[shortest_col] + needed <= max_height {
                col_heights[shortest_col] += needed;
                columns[shortest_col].push(idx);
                placed.push(idx);
            }
        }

        // If nothing could be placed, force the first remaining section
        // onto its own page to avoid an infinite loop.
        if placed.is_empty() {
            columns[0].push(remaining[0]);
            placed.push(remaining[0]);
        }

        remaining.retain(|idx| !placed.contains(idx));
        pages.push(columns);
    }

    pages
}

/// Render a page's columns into the given area.
fn render_columns(
    page: &[Vec<usize>],
    num_cols: usize,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let sections = help_sections();
    let constraints: Vec<Constraint> = (0..num_cols)
        .map(|_| Constraint::Ratio(1, num_cols as u32))
        .collect();
    let col_rects = Layout::horizontal(constraints).spacing(2).split(area);

    for (col_idx, section_indices) in page.iter().enumerate() {
        if col_idx >= col_rects.len() {
            break;
        }
        let col_rect = col_rects[col_idx];
        let mut y = col_rect.y;

        for (i, &sec_idx) in section_indices.iter().enumerate() {
            let sec = &sections[sec_idx];

            // Blank gap between sections (not before the first).
            if i > 0 {
                y += 1;
            }

            if y >= col_rect.y + col_rect.height {
                break;
            }

            // Section title — bold only, no underline.
            let title_line = Line::from(Span::styled(sec.title, theme.emphasis));
            let title_rect = Rect::new(col_rect.x, y, col_rect.width, 1);
            title_line.render(title_rect, buf);
            y += 1;

            // Bindings.
            for (keys, desc) in &sec.bindings {
                if y >= col_rect.y + col_rect.height {
                    break;
                }
                let binding_line = Line::from(vec![
                    Span::styled(format!("  {keys:<20}"), theme.accent),
                    Span::raw(*desc),
                ]);
                let row_rect = Rect::new(col_rect.x, y, col_rect.width, 1);
                binding_line.render(row_rect, buf);
                y += 1;
            }
        }
    }
}

/// Compute a centred popup rectangle as a percentage of the outer area.
fn centered_rect(area: Rect, percent_width: u16, percent_height: u16) -> Rect {
    let w = (area.width as u32 * percent_width as u32 / 100) as u16;
    let h = (area.height as u32 * percent_height as u32 / 100) as u16;

    let vertical = Layout::vertical([Constraint::Length(h)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(w)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
