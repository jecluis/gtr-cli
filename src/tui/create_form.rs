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

//! Centred overlay form for creating and editing tasks.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Tabs, Widget};

use crate::display;
use crate::icons::{Glyphs, IconTheme};
use crate::models::Task;
use crate::tui::theme::{LABEL_PALETTE, Theme};

const SIZES: [&str; 4] = ["S", "M", "L", "XL"];

/// Whether the form is creating a new task or editing an existing one.
pub enum FormMode {
    Create,
    Update { task_id: String },
}

/// Snapshot of original values for change detection in update mode.
struct OriginalValues {
    title: String,
    priority: String,
    size_idx: usize,
    impact: u8,
    joy: u8,
    labels: Vec<String>,
    parent_id: Option<String>,
}

/// Which page of the form is visible.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FormPage {
    Main,
    Properties,
}

impl FormPage {
    fn fields(self) -> &'static [FormField] {
        match self {
            FormPage::Main => &[FormField::Title, FormField::Priority, FormField::Size],
            FormPage::Properties => &[
                FormField::Impact,
                FormField::Joy,
                FormField::Labels,
                FormField::Parent,
            ],
        }
    }
}

/// Which field has focus.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Title,
    Priority,
    Size,
    Impact,
    Joy,
    Labels,
    Parent,
    Submit,
    Cancel,
}

/// State for the task creation/edit form overlay.
pub struct TaskFormState {
    pub project_id: String,
    pub project_name: String,
    mode: FormMode,
    original: Option<OriginalValues>,
    icon_theme: IconTheme,
    glyphs: Glyphs,
    page: FormPage,
    focused: FormField,
    // Main fields
    title: String,
    cursor_pos: usize,
    priority: String,
    size_idx: usize,
    // Properties fields
    impact: u8,
    joy: u8,
    labels: Vec<String>,
    label_input: String,
    label_cursor: usize,
    parent_input: String,
    parent_cursor: usize,
    resolved_parent_title: Option<String>,
    resolved_parent_id: Option<String>,
}

impl TaskFormState {
    pub fn new(project_id: String, project_name: String, icon_theme: IconTheme) -> Self {
        Self {
            project_id,
            project_name,
            mode: FormMode::Create,
            original: None,
            icon_theme,
            glyphs: Glyphs::new(icon_theme),
            page: FormPage::Main,
            focused: FormField::Title,
            title: String::new(),
            cursor_pos: 0,
            priority: "later".to_string(),
            size_idx: 1, // default M
            impact: 3,
            joy: 5,
            labels: Vec::new(),
            label_input: String::new(),
            label_cursor: 0,
            parent_input: String::new(),
            parent_cursor: 0,
            resolved_parent_title: None,
            resolved_parent_id: None,
        }
    }

    /// Create a form pre-filled with an existing task's values.
    pub fn for_update(task: &Task, project_name: String, icon_theme: IconTheme) -> Self {
        let size_idx = SIZES
            .iter()
            .position(|s| s.eq_ignore_ascii_case(&task.size))
            .unwrap_or(1);

        let parent_input = task
            .parent_id
            .as_ref()
            .map(|id| id[..8.min(id.len())].to_string())
            .unwrap_or_default();

        let original = OriginalValues {
            title: task.title.clone(),
            priority: task.priority.clone(),
            size_idx,
            impact: task.impact,
            joy: task.joy,
            labels: task.labels.clone(),
            parent_id: task.parent_id.clone(),
        };

        Self {
            project_id: task.project_id.clone(),
            project_name,
            mode: FormMode::Update {
                task_id: task.id.clone(),
            },
            original: Some(original),
            icon_theme,
            glyphs: Glyphs::new(icon_theme),
            page: FormPage::Main,
            focused: FormPage::Main.fields()[0],
            title: task.title.clone(),
            cursor_pos: task.title.len(),
            priority: task.priority.clone(),
            size_idx,
            impact: task.impact,
            joy: task.joy,
            labels: task.labels.clone(),
            label_input: String::new(),
            label_cursor: 0,
            parent_input,
            parent_cursor: 0,
            resolved_parent_title: None,
            resolved_parent_id: task.parent_id.clone(),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn priority(&self) -> &str {
        &self.priority
    }

    pub fn size(&self) -> &str {
        SIZES[self.size_idx]
    }

    pub fn impact(&self) -> u8 {
        self.impact
    }

    pub fn joy(&self) -> u8 {
        self.joy
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn parent_input(&self) -> &str {
        &self.parent_input
    }

    pub fn parent_id(&self) -> Option<&str> {
        self.resolved_parent_id.as_deref()
    }

    pub fn page(&self) -> FormPage {
        self.page
    }

    pub fn focused(&self) -> FormField {
        self.focused
    }

    /// Whether the title is non-empty (ready to submit).
    pub fn can_submit(&self) -> bool {
        !self.title.trim().is_empty()
    }

    /// Whether the label input is currently non-empty.
    pub fn has_pending_label(&self) -> bool {
        !self.label_input.is_empty()
    }

    /// Move focus to the next field (page fields → Submit → Cancel → wrap).
    pub fn focus_next(&mut self) {
        let fields = self.page.fields();
        match self.focused {
            FormField::Cancel => self.focused = fields[0],
            FormField::Submit => self.focused = FormField::Cancel,
            _ => {
                if let Some(pos) = fields.iter().position(|f| *f == self.focused) {
                    if pos + 1 < fields.len() {
                        self.focused = fields[pos + 1];
                    } else {
                        self.focused = FormField::Submit;
                    }
                }
            }
        }
    }

    /// Move focus to the previous field (wrap → Cancel → Submit → page fields).
    pub fn focus_prev(&mut self) {
        let fields = self.page.fields();
        match self.focused {
            FormField::Submit => self.focused = *fields.last().unwrap(),
            FormField::Cancel => self.focused = FormField::Submit,
            _ => {
                if let Some(pos) = fields.iter().position(|f| *f == self.focused) {
                    if pos > 0 {
                        self.focused = fields[pos - 1];
                    } else {
                        self.focused = FormField::Cancel;
                    }
                }
            }
        }
    }

    /// Switch to the next page.
    pub fn next_page(&mut self) {
        match self.page {
            FormPage::Main => {
                self.page = FormPage::Properties;
                self.focused = FormPage::Properties.fields()[0];
            }
            FormPage::Properties => {} // already on last page
        }
    }

    /// Switch to the previous page.
    pub fn prev_page(&mut self) {
        match self.page {
            FormPage::Main => {} // already on first page
            FormPage::Properties => {
                self.page = FormPage::Main;
                self.focused = FormPage::Main.fields()[0];
            }
        }
    }

    /// Handle character input for text fields.
    pub fn char_input(&mut self, c: char) {
        match self.focused {
            FormField::Title => {
                self.title.insert(self.cursor_pos, c);
                self.cursor_pos += c.len_utf8();
            }
            FormField::Labels => {
                if c == ',' {
                    self.commit_label();
                } else {
                    self.label_input.insert(self.label_cursor, c);
                    self.label_cursor += c.len_utf8();
                }
            }
            FormField::Parent => {
                self.parent_input.insert(self.parent_cursor, c);
                self.parent_cursor += c.len_utf8();
            }
            _ => {}
        }
    }

    /// Handle backspace.
    pub fn backspace(&mut self) {
        match self.focused {
            FormField::Title => {
                if self.cursor_pos > 0 {
                    let prev = self.title[..self.cursor_pos]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.title.remove(prev);
                    self.cursor_pos = prev;
                }
            }
            FormField::Labels => {
                if self.label_cursor > 0 {
                    let prev = self.label_input[..self.label_cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.label_input.remove(prev);
                    self.label_cursor = prev;
                } else {
                    // Backspace on empty input removes last label
                    self.labels.pop();
                }
            }
            FormField::Parent => {
                if self.parent_cursor > 0 {
                    let prev = self.parent_input[..self.parent_cursor]
                        .char_indices()
                        .next_back()
                        .map(|(i, _)| i)
                        .unwrap_or(0);
                    self.parent_input.remove(prev);
                    self.parent_cursor = prev;
                }
            }
            _ => {}
        }
    }

    /// Handle space or toggle for non-text fields.
    pub fn toggle_or_space(&mut self) {
        match self.focused {
            FormField::Title => self.char_input(' '),
            FormField::Priority => {
                self.priority = if self.priority == "now" {
                    "later".to_string()
                } else {
                    "now".to_string()
                };
            }
            FormField::Size => {
                self.size_idx = (self.size_idx + 1) % SIZES.len();
            }
            FormField::Impact => {
                self.impact = (self.impact % 5) + 1;
            }
            FormField::Joy => {
                self.joy = (self.joy % 10) + 1;
            }
            FormField::Labels => self.char_input(' '),
            FormField::Parent => self.char_input(' '),
            FormField::Submit | FormField::Cancel => {}
        }
    }

    /// Adjust a numeric field by delta (for Left/Right on Impact/Joy).
    pub fn adjust_field(&mut self, delta: i8) {
        match self.focused {
            FormField::Impact => {
                let new = self.impact as i8 + delta;
                self.impact = new.clamp(1, 5) as u8;
            }
            FormField::Joy => {
                let new = self.joy as i8 + delta;
                self.joy = new.clamp(1, 10) as u8;
            }
            FormField::Priority => {
                // Left/Right cycles same as Space
                self.priority = if self.priority == "now" {
                    "later".to_string()
                } else {
                    "now".to_string()
                };
            }
            FormField::Size => {
                let new = self.size_idx as i8 + delta;
                self.size_idx = new.clamp(0, SIZES.len() as i8 - 1) as usize;
            }
            _ => {}
        }
    }

    /// Commit the current label input if valid.
    pub fn commit_label(&mut self) -> bool {
        let trimmed = self.label_input.trim().to_string();
        if trimmed.is_empty() {
            return false;
        }
        if crate::labels::validate_label(&trimmed).is_err() {
            return false;
        }
        if self.labels.contains(&trimmed) {
            self.label_input.clear();
            self.label_cursor = 0;
            return false;
        }
        self.labels.push(trimmed);
        self.label_input.clear();
        self.label_cursor = 0;
        true
    }

    /// Set the resolved parent title for display feedback.
    pub fn set_resolved_parent(&mut self, title: Option<String>) {
        self.resolved_parent_title = title;
    }

    /// Store the resolved full parent UUID.
    pub fn set_parent_id(&mut self, full_id: Option<String>) {
        self.resolved_parent_id = full_id;
    }

    /// Whether this form is in update mode.
    pub fn is_update(&self) -> bool {
        matches!(self.mode, FormMode::Update { .. })
    }

    /// The task ID when in update mode.
    pub fn task_id(&self) -> Option<&str> {
        match &self.mode {
            FormMode::Update { task_id } => Some(task_id),
            FormMode::Create => None,
        }
    }

    /// Compute which fields changed compared to the original values.
    pub fn changed_fields(&self) -> ChangedFields {
        let Some(orig) = &self.original else {
            return ChangedFields::default();
        };
        ChangedFields {
            title: (self.title != orig.title).then(|| self.title.clone()),
            priority: (self.priority != orig.priority).then(|| self.priority.clone()),
            size: (self.size_idx != orig.size_idx).then(|| SIZES[self.size_idx].to_string()),
            impact: (self.impact != orig.impact).then_some(self.impact),
            joy: (self.joy != orig.joy).then_some(self.joy),
            labels: (self.labels != orig.labels).then(|| self.labels.clone()),
            parent_id: (self.resolved_parent_id != orig.parent_id)
                .then(|| self.resolved_parent_id.clone()),
        }
    }

    /// Whether any field has been modified from the original values.
    pub fn has_changes(&self) -> bool {
        let f = self.changed_fields();
        f.title.is_some()
            || f.priority.is_some()
            || f.size.is_some()
            || f.impact.is_some()
            || f.joy.is_some()
            || f.labels.is_some()
            || f.parent_id.is_some()
    }

    pub fn render(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let popup_width = 56u16.min(area.width.saturating_sub(4));
        let popup_height = 15;
        let popup = centered_rect(popup_width, popup_height, area);

        Clear.render(popup, buf);

        let border_style = theme.accent;
        let title = if self.is_update() {
            format!(" edit task in {} ", self.project_name)
        } else {
            format!(" new task in {} ", self.project_name)
        };
        let block = Block::bordered().title(title).border_style(border_style);
        let inner = block.inner(popup);
        block.render(popup, buf);

        let field_rows = Layout::vertical([
            Constraint::Length(1), // tab bar
            Constraint::Length(1), // divider
            Constraint::Length(1), // field 1 label
            Constraint::Length(1), // field 1 input (or field 2)
            Constraint::Length(1), // padding / field 2
            Constraint::Length(1), // field 2 / field 3
            Constraint::Length(1), // field 3 / field 4
            Constraint::Length(1), // padding
            Constraint::Length(1), // padding
            Constraint::Length(1), // padding
            Constraint::Length(1), // submit buttons
        ])
        .split(inner);

        self.render_tab_bar(theme, field_rows[0], buf);
        self.render_divider(theme, field_rows[1], buf);

        match self.page {
            FormPage::Main => self.render_main_page(theme, &field_rows, buf),
            FormPage::Properties => self.render_properties_page(theme, &field_rows, buf),
        }

        self.render_submit_buttons(theme, field_rows[10], buf);
    }

    fn render_tab_bar(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let selected = match self.page {
            FormPage::Main => 0,
            FormPage::Properties => 1,
        };

        let [tabs_area, hint_area] =
            Layout::horizontal([Constraint::Min(20), Constraint::Length(16)]).areas(area);

        Tabs::new(vec![" Main ", " Properties "])
            .select(selected)
            .style(Style::default())
            .highlight_style(theme.accent.add_modifier(Modifier::BOLD))
            .divider(symbols::DOT)
            .padding("", "")
            .render(tabs_area, buf);

        Line::from(Span::styled(
            "(Ctrl-\u{2190}/\u{2192})",
            Style::default().fg(Color::Gray),
        ))
        .alignment(Alignment::Right)
        .render(hint_area, buf);
    }

    fn render_divider(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let line = "\u{2500}".repeat(area.width as usize);
        Line::from(Span::styled(line, theme.divider)).render(area, buf);
    }

    fn render_main_page(&self, theme: &Theme, rows: &[Rect], buf: &mut Buffer) {
        // Title field
        let title_style = field_label_style(self.focused == FormField::Title, theme);
        Line::from(vec![Span::raw("  "), Span::styled("Title: ", title_style)])
            .render(rows[2], buf);

        render_text_input(
            &self.title,
            self.cursor_pos,
            self.focused == FormField::Title,
            theme,
            rows[3],
            buf,
        );

        // Priority field
        let pri_style = field_label_style(self.focused == FormField::Priority, theme);
        let now_style = if self.priority == "now" {
            theme.danger.add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let later_style = if self.priority == "later" {
            theme.success.add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        Line::from(vec![
            Span::raw("  "),
            Span::styled("Priority: ", pri_style),
            Span::styled("now", now_style),
            Span::raw(" / "),
            Span::styled("later", later_style),
        ])
        .render(rows[5], buf);

        // Size field
        let size_style = field_label_style(self.focused == FormField::Size, theme);
        let mut size_spans = vec![Span::raw("  "), Span::styled("Size:     ", size_style)];
        for (i, s) in SIZES.iter().enumerate() {
            let style = if i == self.size_idx {
                theme.accent.add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            if i > 0 {
                size_spans.push(Span::raw(" / "));
            }
            size_spans.push(Span::styled(*s, style));
        }
        Line::from(size_spans).render(rows[6], buf);
    }

    fn render_properties_page(&self, theme: &Theme, rows: &[Rect], buf: &mut Buffer) {
        // Impact field — 5 circles + icon
        self.render_circles(
            "Impact:  ",
            self.impact,
            5,
            FormField::Impact,
            theme,
            rows[2],
            buf,
        );

        // Joy field — 10 circles + icon
        self.render_circles(
            "Joy:     ",
            self.joy,
            10,
            FormField::Joy,
            theme,
            rows[3],
            buf,
        );

        // Labels field
        self.render_labels_field(theme, rows[5], buf);

        // Parent field
        self.render_parent_field(theme, rows[6], buf);
    }

    #[allow(clippy::too_many_arguments)]
    fn render_circles(
        &self,
        label: &str,
        value: u8,
        max: u8,
        field: FormField,
        theme: &Theme,
        area: Rect,
        buf: &mut Buffer,
    ) {
        let label_style = field_label_style(self.focused == field, theme);

        let mut spans = vec![Span::raw("  "), Span::styled(label, label_style)];
        for i in 1..=max {
            let circle = if i <= value { "\u{25cf}" } else { "\u{25cb}" };
            let style = if i <= value {
                theme.accent
            } else {
                Style::default()
            };
            spans.push(Span::styled(circle, style));
        }
        spans.push(Span::raw(format!(" {value}")));

        // Append the appropriate icon
        match field {
            FormField::Impact => {
                let (glyph, style) = match display::impact_level(value) {
                    display::ImpactLevel::Critical => (self.glyphs.impact_critical, theme.danger),
                    display::ImpactLevel::Significant => {
                        (self.glyphs.impact_significant, Style::new().fg(Color::Blue))
                    }
                    display::ImpactLevel::Normal => ("", Style::default()),
                };
                if !glyph.is_empty() {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(glyph, style));
                }
            }
            FormField::Joy => {
                let glyph = self.glyphs.joy_icon(value);
                if !glyph.is_empty() {
                    let joy_style = match (value, self.icon_theme) {
                        (8..=10, IconTheme::Nerd) => Style::default().fg(Color::Yellow),
                        (0..=4, IconTheme::Nerd) => Style::default().fg(Color::Blue),
                        _ => Style::default(),
                    };
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(glyph, joy_style));
                }
            }
            _ => {}
        }

        Line::from(spans).render(area, buf);
    }

    fn render_labels_field(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        use crate::display::LABEL_PALETTE_LEN;

        let label_style = field_label_style(self.focused == FormField::Labels, theme);

        let mut spans: Vec<Span<'_>> =
            vec![Span::raw("  "), Span::styled("Labels:  ", label_style)];

        for (i, label) in self.labels.iter().enumerate() {
            let color = LABEL_PALETTE[i % LABEL_PALETTE_LEN];
            if i > 0 {
                spans.push(Span::raw(" "));
            }
            spans.push(Span::styled(label.clone(), Style::default().fg(color)));
        }

        if self.focused == FormField::Labels {
            if !self.labels.is_empty() {
                spans.push(Span::raw(" "));
            }
            if self.label_input.is_empty() {
                spans.push(Span::styled("\u{2588}", theme.selected));
                if self.labels.is_empty() {
                    spans.push(Span::styled(" (type to add)", theme.muted));
                }
            } else {
                render_cursor_spans(&self.label_input, self.label_cursor, theme, &mut spans);
            }
        } else if self.labels.is_empty() {
            spans.push(Span::styled("(none)", theme.muted));
        }

        Line::from(spans).render(area, buf);
    }

    fn render_parent_field(&self, theme: &Theme, area: Rect, buf: &mut Buffer) {
        let label_style = field_label_style(self.focused == FormField::Parent, theme);

        let mut spans: Vec<Span<'_>> =
            vec![Span::raw("  "), Span::styled("Parent:  ", label_style)];

        if self.focused == FormField::Parent {
            if self.parent_input.is_empty() {
                spans.push(Span::styled("\u{2588}", theme.selected));
                spans.push(Span::styled(" (task ID)", theme.muted));
            } else {
                render_cursor_spans(&self.parent_input, self.parent_cursor, theme, &mut spans);
            }
            if let Some(ref title) = self.resolved_parent_title {
                spans.push(Span::styled(format!(" \u{2192} {title}"), theme.muted));
            }
        } else if self.parent_input.is_empty() {
            spans.push(Span::styled("(none)", theme.muted));
        } else {
            spans.push(Span::raw(self.parent_input.clone()));
            if let Some(ref title) = self.resolved_parent_title {
                spans.push(Span::styled(format!(" \u{2192} {title}"), theme.muted));
            }
        }

        Line::from(spans).render(area, buf);
    }

    fn render_submit_buttons(&self, _theme: &Theme, area: Rect, buf: &mut Buffer) {
        let action_text = if self.is_update() {
            " Save "
        } else {
            " Create "
        };

        let save_focused = self.focused == FormField::Submit;
        let cancel_focused = self.focused == FormField::Cancel;

        let save_style = if !self.can_submit() {
            Style::default().fg(Color::DarkGray)
        } else if save_focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };

        let cancel_style = if cancel_focused {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Red)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Red)
        };

        let hint = Line::from(vec![
            Span::raw("  "),
            Span::styled(action_text, save_style),
            Span::raw("  "),
            Span::styled(" Cancel ", cancel_style),
        ]);
        hint.alignment(Alignment::Left).render(area, buf);
    }
}

/// Set of fields that changed from original values in update mode.
#[derive(Default)]
pub struct ChangedFields {
    pub title: Option<String>,
    pub priority: Option<String>,
    pub size: Option<String>,
    pub impact: Option<u8>,
    pub joy: Option<u8>,
    pub labels: Option<Vec<String>>,
    /// `Some(None)` means parent was cleared; `Some(Some(id))` means
    /// parent was changed to a new ID.
    pub parent_id: Option<Option<String>>,
}

/// Style for a field label: accent+bold when focused, default otherwise.
fn field_label_style(focused: bool, theme: &Theme) -> Style {
    if focused {
        theme.accent.add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    }
}

/// Append cursor-highlighted spans for a text field to the span list.
fn render_cursor_spans<'a>(text: &str, cursor: usize, theme: &Theme, spans: &mut Vec<Span<'a>>) {
    let (before, after) = text.split_at(cursor);
    spans.push(Span::raw(before.to_string()));
    spans.push(Span::styled(
        if after.is_empty() {
            "\u{2588}".to_string()
        } else {
            after.chars().next().unwrap().to_string()
        },
        theme.selected,
    ));
    if after.len() > 1 {
        spans.push(Span::raw(
            after[after.chars().next().unwrap().len_utf8()..].to_string(),
        ));
    }
}

/// Render a text input with cursor at the given position.
fn render_text_input(
    text: &str,
    cursor: usize,
    focused: bool,
    theme: &Theme,
    area: Rect,
    buf: &mut Buffer,
) {
    let mut spans = vec![Span::raw("  ")];
    if focused {
        if text.is_empty() {
            spans.push(Span::styled("\u{2588}", theme.selected));
        } else {
            render_cursor_spans(text, cursor, theme, &mut spans);
        }
    } else {
        spans.push(Span::raw(text.to_string()));
    }
    Line::from(spans).render(area, buf);
}

/// Return a centred `Rect` of the given width and height within `area`.
fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let v = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(area);
    let h = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .split(v[1]);
    h[1]
}
