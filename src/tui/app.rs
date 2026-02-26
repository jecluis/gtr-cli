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

//! rat-salsa application skeleton: Global state, event types, and the
//! four callback functions (init, render, event, error).

use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use rat_salsa::poll::PollCrossterm;
use rat_salsa::{Control, RunConfig, SalsaAppContext, SalsaContext, run_tui};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::cache::TaskCache;
use crate::config::Config;
use crate::storage::{StorageConfig, TaskStorage};
use crate::tui::keymap::{self, Action, Keymap, KeymapResult};
use crate::tui::sidebar::{SidebarState, TreeItemKind};
use crate::tui::task_detail::TaskDetailState;
use crate::tui::task_list::TaskListState;
use crate::tui::theme::Theme;

/// Application-wide state visible to all callbacks.
#[allow(dead_code)]
pub struct Global {
    ctx: SalsaAppContext<AppEvent, crate::Error>,
    pub config: Config,
    pub cache: TaskCache,
    pub storage: TaskStorage,
}

impl SalsaContext<AppEvent, crate::Error> for Global {
    fn set_salsa_ctx(&mut self, app_ctx: SalsaAppContext<AppEvent, crate::Error>) {
        self.ctx = app_ctx;
    }

    fn salsa_ctx(&self) -> &SalsaAppContext<AppEvent, crate::Error> {
        &self.ctx
    }
}

/// All events funnelled through the rat-salsa event loop.
#[derive(Debug)]
pub enum AppEvent {
    /// Terminal input (key press, mouse, resize).
    Event(Event),
}

impl From<Event> for AppEvent {
    fn from(value: Event) -> Self {
        Self::Event(value)
    }
}

/// Which panel currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPanel {
    Sidebar,
    Main,
}

/// What the main panel is currently showing.
pub enum MainView {
    Dashboard,
    TaskList(Box<TaskListState>),
    TaskDetail {
        detail: Box<TaskDetailState>,
        /// Preserved list state so we can go back.
        list: Box<TaskListState>,
    },
}

/// UI widget state tree.
pub struct AppState {
    pub keymap: Keymap,
    pub theme: Theme,
    pub sidebar: SidebarState,
    pub focus: FocusPanel,
    pub main_view: MainView,
}

/// Enter the TUI event loop. Returns when the user quits.
pub fn run(config: Config) -> crate::Result<()> {
    let cache = TaskCache::open(&config.cache_dir.join("index.db"))?;
    let sidebar = SidebarState::from_cache(&cache)?;
    let storage_config = StorageConfig::new(config.cache_dir.clone(), "default".to_string());
    let storage = TaskStorage::new(storage_config);

    let mut global = Global {
        ctx: SalsaAppContext::default(),
        config,
        cache,
        storage,
    };
    let mut state = AppState {
        keymap: keymap::default_keymap(),
        theme: Theme::default(),
        sidebar,
        focus: FocusPanel::Sidebar,
        main_view: MainView::Dashboard,
    };

    run_tui(
        init,
        render,
        handle_event,
        handle_error,
        &mut global,
        &mut state,
        RunConfig::default()?.poll(PollCrossterm),
    )
}

fn init(_state: &mut AppState, _ctx: &mut Global) -> Result<(), crate::Error> {
    Ok(())
}

fn render(
    area: Rect,
    buf: &mut Buffer,
    state: &mut AppState,
    _ctx: &mut Global,
) -> Result<(), crate::Error> {
    let layout = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(area);
    let columns =
        Layout::horizontal([Constraint::Length(28), Constraint::Fill(1)]).split(layout[0]);

    let theme = &state.theme;
    let sidebar_focused = state.focus == FocusPanel::Sidebar;
    let main_focused = state.focus == FocusPanel::Main;

    // Sidebar
    state
        .sidebar
        .render(theme, sidebar_focused, columns[0], buf);

    // Main area — dispatch based on current view
    match &state.main_view {
        MainView::Dashboard => render_dashboard(theme, main_focused, columns[1], buf),
        MainView::TaskList(task_list) => task_list.render(theme, main_focused, columns[1], buf),
        MainView::TaskDetail { detail, .. } => {
            detail.render(theme, main_focused, columns[1], buf);
        }
    }

    // Status bar
    render_status_bar(state, layout[1], buf);

    // Which-key popup when a prefix key is pending
    if let Some(node) = state.keymap.pending_node() {
        keymap::render_which_key(node, theme, area, buf);
    }

    Ok(())
}

/// Render the dashboard placeholder in the main area.
fn render_dashboard(theme: &Theme, focused: bool, area: Rect, buf: &mut Buffer) {
    let border = if focused {
        theme.border_focused
    } else {
        theme.border_unfocused
    };
    let greeting = Paragraph::new(vec![
        Line::default(),
        Line::from_iter([
            Span::from("  gtr").style(theme.accent.add_modifier(Modifier::BOLD)),
            Span::from(" \u{2014} Getting Things Rusty"),
        ]),
        Line::default(),
        Span::from("  Dashboard coming soon...")
            .style(theme.muted)
            .into(),
    ])
    .block(Block::bordered().title(" dashboard ").border_style(border));
    greeting.render(area, buf);
}

fn handle_event(
    event: &AppEvent,
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let AppEvent::Event(Event::Key(key)) = event else {
        return Ok(Control::Continue);
    };
    if key.kind != KeyEventKind::Press {
        return Ok(Control::Continue);
    }

    // Ctrl-c always quits regardless of keymap state.
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Ok(Control::Quit);
    }

    // Tab toggles focus between sidebar and main panel.
    if key.code == KeyCode::Tab {
        state.focus = match state.focus {
            FocusPanel::Sidebar => FocusPanel::Main,
            FocusPanel::Main => FocusPanel::Sidebar,
        };
        return Ok(Control::Changed);
    }

    // Sidebar-local navigation when sidebar is focused.
    if state.focus == FocusPanel::Sidebar && !state.keymap.is_pending() {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                state.sidebar.select_prev();
                return Ok(Control::Changed);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                state.sidebar.select_next();
                return Ok(Control::Changed);
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                return handle_sidebar_select(state, ctx);
            }
            _ => {}
        }
    }

    // Main panel navigation when main is focused and not mid-chord.
    if state.focus == FocusPanel::Main && !state.keymap.is_pending() {
        match &mut state.main_view {
            MainView::TaskList(task_list) => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    task_list.select_prev();
                    return Ok(Control::Changed);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    task_list.select_next();
                    return Ok(Control::Changed);
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    return handle_task_list_select(state, ctx);
                }
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                    state.main_view = MainView::Dashboard;
                    return Ok(Control::Changed);
                }
                _ => {}
            },
            MainView::TaskDetail { detail, .. } => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    detail.scroll_up();
                    return Ok(Control::Changed);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    detail.scroll_down();
                    return Ok(Control::Changed);
                }
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                    return handle_back_from_detail(state);
                }
                _ => {}
            },
            MainView::Dashboard => {}
        }
    }

    // Global keymap (chords, actions).
    match state.keymap.process(*key) {
        KeymapResult::Matched(Action::Quit) => {
            // q navigates back through the view stack before quitting.
            match &state.main_view {
                MainView::TaskDetail { .. } => handle_back_from_detail(state),
                MainView::TaskList(_) => {
                    state.main_view = MainView::Dashboard;
                    Ok(Control::Changed)
                }
                MainView::Dashboard => Ok(Control::Quit),
            }
        }
        KeymapResult::Matched(_action) => {
            // Other actions will be handled in later commits.
            Ok(Control::Changed)
        }
        KeymapResult::Pending(_) => Ok(Control::Changed),
        KeymapResult::Cancelled | KeymapResult::NotFound => Ok(Control::Changed),
    }
}

/// Handle Enter/l/Right on a sidebar item.
fn handle_sidebar_select(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let kind = state.sidebar.selected_kind();
    let id = state.sidebar.selected_id().to_string();
    let name = state.sidebar.selected_name().to_string();

    match kind {
        Some(TreeItemKind::Project) if !id.is_empty() => {
            let icon_theme = ctx.config.effective_icon_theme();
            let task_list = TaskListState::from_cache(&ctx.cache, &id, &name, icon_theme)?;
            state.main_view = MainView::TaskList(Box::new(task_list));
            state.focus = FocusPanel::Main;
            Ok(Control::Changed)
        }
        _ => Ok(Control::Continue),
    }
}

/// Handle Enter/l/Right on a task in the task list.
fn handle_task_list_select(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    // We need to take ownership of the current TaskList to move it into TaskDetail.
    let MainView::TaskList(ref task_list) = state.main_view else {
        return Ok(Control::Continue);
    };

    let Some(task_id) = task_list.selected_task_id() else {
        return Ok(Control::Continue);
    };
    let task_id = task_id.to_string();
    let project_name = task_list.project_name.clone();

    // Load full task from CRDT storage.
    let task = match ctx.storage.load_task(&task_id) {
        Ok(task) => task,
        Err(_) => return Ok(Control::Continue),
    };

    let detail = Box::new(TaskDetailState::new(task, project_name));

    // Move the list state into the detail variant for back-navigation.
    let prev = std::mem::replace(&mut state.main_view, MainView::Dashboard);
    let MainView::TaskList(list) = prev else {
        unreachable!();
    };
    state.main_view = MainView::TaskDetail { detail, list };

    Ok(Control::Changed)
}

/// Navigate back from task detail to the task list.
fn handle_back_from_detail(state: &mut AppState) -> Result<Control<AppEvent>, crate::Error> {
    let prev = std::mem::replace(&mut state.main_view, MainView::Dashboard);
    if let MainView::TaskDetail { list, .. } = prev {
        state.main_view = MainView::TaskList(list);
    }
    Ok(Control::Changed)
}

/// Render the status bar with context-aware key hints.
fn render_status_bar(state: &AppState, area: Rect, buf: &mut Buffer) {
    let theme = &state.theme;

    let mut hints: Vec<Span<'_>> = Vec::new();

    match (&state.main_view, state.focus) {
        (MainView::TaskDetail { .. }, FocusPanel::Main) => {
            hints.extend([
                Span::styled(" Esc", theme.status_key),
                Span::styled(" back", theme.status_desc),
                Span::styled("  j/k", theme.status_key),
                Span::styled(" scroll", theme.status_desc),
            ]);
        }
        (MainView::TaskList(tl), FocusPanel::Main) => {
            hints.extend([
                Span::styled(" Esc", theme.status_key),
                Span::styled(" back", theme.status_desc),
                Span::styled("  j/k", theme.status_key),
                Span::styled(" nav", theme.status_desc),
            ]);
            if !tl.is_empty() {
                hints.extend([
                    Span::styled("  Enter", theme.status_key),
                    Span::styled(" open", theme.status_desc),
                ]);
            }
        }
        _ => {
            hints.extend([
                Span::styled(" q", theme.status_key),
                Span::styled(" quit", theme.status_desc),
                Span::styled("  g", theme.status_key),
                Span::styled(" goto", theme.status_desc),
            ]);
        }
    }

    hints.extend([
        Span::styled("  Tab", theme.status_key),
        Span::styled(" focus", theme.status_desc),
    ]);

    if state.focus == FocusPanel::Sidebar {
        hints.extend([
            Span::styled("  j/k", theme.status_key),
            Span::styled(" nav", theme.status_desc),
            Span::styled("  Enter", theme.status_key),
            Span::styled(" open", theme.status_desc),
        ]);
    }

    hints.extend([
        Span::styled("  ?", theme.status_key),
        Span::styled(" help", theme.status_desc),
    ]);

    Line::from(hints).style(theme.status_bar).render(area, buf);
}

fn handle_error(
    err: crate::Error,
    _state: &mut AppState,
    _ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    Err(err)
}
