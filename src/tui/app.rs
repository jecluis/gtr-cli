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

use std::io::stdout;

use crossterm::ExecutableCommand;
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use rat_salsa::poll::{PollCrossterm, PollTasks};
use rat_salsa::{Control, RunConfig, SalsaAppContext, SalsaContext, run_tui};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::cache::TaskCache;
use crate::config::Config;
use crate::storage::{StorageConfig, TaskStorage};
use crate::tui::command_bar::CommandBarState;
use crate::tui::confirm::ConfirmState;
use crate::tui::create_form::{FormField, TaskFormState};
use crate::tui::doc_detail::DocumentDetailState;
use crate::tui::doc_form::{DocFormField, DocFormState};
use crate::tui::doc_list::DocumentListState;
use crate::tui::feels::FeelsDialogState;
use crate::tui::help::HelpOverlayState;
use crate::tui::keymap::{self, Action, Keymap, KeymapResult};
use crate::tui::nav::NavTarget;
use crate::tui::search::{SearchFilter, SearchOverlayState, SearchResultKind};
use crate::tui::sidebar::{ActiveItem, SidebarState, TreeItemKind};
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
    /// Background sync completed (true = success).
    SyncComplete(bool),
}

impl From<Event> for AppEvent {
    fn from(value: Event) -> Self {
        Self::Event(value)
    }
}

/// Background sync status indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    Idle,
    Syncing,
    Synced,
    Failed,
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
    DocList(Box<DocumentListState>),
    DocDetail {
        detail: Box<DocumentDetailState>,
        /// Preserved list state so we can go back.
        list: Box<DocumentListState>,
    },
}

/// Maximum number of entries in the navigation history stack.
const NAV_HISTORY_LIMIT: usize = 20;

/// UI widget state tree.
pub struct AppState {
    pub keymap: Keymap,
    pub theme: Theme,
    pub sidebar: SidebarState,
    pub focus: FocusPanel,
    pub main_view: MainView,
    pub confirm: Option<ConfirmState>,
    pub create_form: Option<TaskFormState>,
    pub doc_form: Option<DocFormState>,
    pub command_bar: Option<CommandBarState>,
    pub search: Option<SearchOverlayState>,
    pub feels: Option<FeelsDialogState>,
    pub help: Option<HelpOverlayState>,
    pub sync_status: SyncStatus,
    /// Navigation history for detail-to-detail link following.
    pub nav_history: Vec<MainView>,
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
        confirm: None,
        create_form: None,
        doc_form: None,
        command_bar: None,
        search: None,
        feels: None,
        help: None,
        sync_status: SyncStatus::Idle,
        nav_history: Vec::new(),
    };

    run_tui(
        init,
        render,
        handle_event,
        handle_error,
        &mut global,
        &mut state,
        RunConfig::default()?
            .poll(PollCrossterm)
            .poll(PollTasks::default()),
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

    // Derive which sidebar item corresponds to the current main view.
    let active = derive_active_item(&state.main_view);

    // Sidebar
    state
        .sidebar
        .render(theme, sidebar_focused, &active, columns[0], buf);

    // Main area — dispatch based on current view
    match &mut state.main_view {
        MainView::Dashboard => render_dashboard(theme, main_focused, columns[1], buf),
        MainView::TaskList(task_list) => task_list.render(theme, main_focused, columns[1], buf),
        MainView::TaskDetail { detail, .. } => {
            detail.render(theme, main_focused, columns[1], buf);
        }
        MainView::DocList(doc_list) => doc_list.render(theme, main_focused, columns[1], buf),
        MainView::DocDetail { detail, .. } => {
            detail.render(theme, main_focused, columns[1], buf);
        }
    }

    // Status bar (or command bar when active)
    if let Some(ref cmd_bar) = state.command_bar {
        cmd_bar.render(theme, layout[1], buf);
    } else {
        render_status_bar(state, layout[1], buf);
    }

    // Search overlay (full-screen, above content)
    if let Some(ref mut search) = state.search {
        search.render(theme, area, buf);
    }

    // Feels dialog
    if let Some(ref feels) = state.feels {
        feels.render(theme, area, buf);
    }

    // Help overlay
    if let Some(ref help) = state.help {
        help.render(theme, area, buf);
    }

    // Overlay dialogs (above content, below which-key)
    if let Some(ref confirm) = state.confirm {
        confirm.render(theme, area, buf);
    }
    if let Some(ref form) = state.create_form {
        form.render(theme, area, buf);
    }
    if let Some(ref form) = state.doc_form {
        form.render(theme, area, buf);
    }

    // Which-key popup when a prefix key is pending
    if let Some(node) = state.keymap.pending_node() {
        keymap::render_which_key(node, theme, area, buf);
    }

    Ok(())
}

/// Map the current main view to the corresponding sidebar active item.
fn derive_active_item(view: &MainView) -> ActiveItem<'_> {
    match view {
        MainView::Dashboard => ActiveItem::Dashboard,
        MainView::TaskList(tl) => {
            if tl.project_id == TaskCache::meta_root_id() {
                ActiveItem::AllProjects
            } else {
                ActiveItem::Project(&tl.project_id)
            }
        }
        MainView::TaskDetail { list, .. } => {
            if list.project_id == TaskCache::meta_root_id() {
                ActiveItem::AllProjects
            } else {
                ActiveItem::Project(&list.project_id)
            }
        }
        MainView::DocList(dl) => {
            if dl.namespace_id.is_empty() {
                ActiveItem::AllNamespaces
            } else {
                ActiveItem::Namespace(&dl.namespace_id)
            }
        }
        MainView::DocDetail { list, .. } => {
            if list.namespace_id.is_empty() {
                ActiveItem::AllNamespaces
            } else {
                ActiveItem::Namespace(&list.namespace_id)
            }
        }
    }
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
    if let AppEvent::SyncComplete(success) = event {
        state.sync_status = if *success {
            SyncStatus::Synced
        } else {
            SyncStatus::Failed
        };
        if *success {
            refresh_current_view(state, ctx);
        }
        return Ok(Control::Changed);
    }

    let AppEvent::Event(Event::Key(key)) = event else {
        return Ok(Control::Continue);
    };
    if key.kind != KeyEventKind::Press {
        return Ok(Control::Continue);
    }

    // Ctrl-c / Ctrl-q always quit regardless of keymap state.
    if (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('q'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
    {
        return Ok(Control::Quit);
    }

    // Confirmation dialog intercepts all input when active.
    if state.confirm.is_some() {
        return handle_confirm_input(key.code, state, ctx);
    }

    // Create form intercepts all input when active.
    if state.create_form.is_some() {
        return handle_create_form_input(key, state, ctx);
    }

    // Document form intercepts all input when active.
    if state.doc_form.is_some() {
        return handle_doc_form_input(key, state, ctx);
    }

    // Command bar intercepts all input when active.
    if state.command_bar.is_some() {
        return handle_command_bar_input(key.code, state, ctx);
    }

    // Search overlay intercepts all input when active.
    if state.search.is_some() {
        return handle_search_input(key.code, state, ctx);
    }

    // Feels dialog intercepts all input when active.
    if state.feels.is_some() {
        return handle_feels_input(key.code, state, ctx);
    }

    // Help overlay intercepts all input when active.
    if state.help.is_some() {
        return handle_help_input(key.code, state);
    }

    // Tab toggles focus between sidebar and main panel.
    if key.code == KeyCode::Tab {
        state.focus = match state.focus {
            FocusPanel::Sidebar => {
                // Snap sidebar selection back to the active item.
                let active = derive_active_item(&state.main_view);
                state.sidebar.select_active(&active);
                FocusPanel::Main
            }
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
        // Filter input mode: intercept all keys when search is active.
        if let MainView::TaskList(ref mut task_list) = state.main_view
            && task_list.is_filtering()
        {
            match key.code {
                KeyCode::Esc => {
                    task_list.cancel_filter();
                    return Ok(Control::Changed);
                }
                KeyCode::Enter => {
                    // Confirm filter — just dismiss the filter input bar.
                    return Ok(Control::Changed);
                }
                KeyCode::Backspace => {
                    task_list.filter_pop();
                    return Ok(Control::Changed);
                }
                KeyCode::Char(c) => {
                    task_list.filter_push(c);
                    return Ok(Control::Changed);
                }
                KeyCode::Up => {
                    task_list.select_prev();
                    return Ok(Control::Changed);
                }
                KeyCode::Down => {
                    task_list.select_next();
                    return Ok(Control::Changed);
                }
                KeyCode::PageUp => {
                    task_list.select_page_up(10);
                    return Ok(Control::Changed);
                }
                KeyCode::PageDown => {
                    task_list.select_page_down(10);
                    return Ok(Control::Changed);
                }
                _ => return Ok(Control::Continue),
            }
        }

        // Document list filter mode.
        if let MainView::DocList(ref mut doc_list) = state.main_view
            && doc_list.is_filtering()
        {
            match key.code {
                KeyCode::Esc => {
                    doc_list.cancel_filter();
                    return Ok(Control::Changed);
                }
                KeyCode::Enter => return Ok(Control::Changed),
                KeyCode::Backspace => {
                    doc_list.filter_pop();
                    return Ok(Control::Changed);
                }
                KeyCode::Char(c) => {
                    doc_list.filter_push(c);
                    return Ok(Control::Changed);
                }
                KeyCode::Up => {
                    doc_list.select_prev();
                    return Ok(Control::Changed);
                }
                KeyCode::Down => {
                    doc_list.select_next();
                    return Ok(Control::Changed);
                }
                KeyCode::PageUp => {
                    doc_list.select_page_up(10);
                    return Ok(Control::Changed);
                }
                KeyCode::PageDown => {
                    doc_list.select_page_down(10);
                    return Ok(Control::Changed);
                }
                _ => return Ok(Control::Continue),
            }
        }

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
                KeyCode::PageUp => {
                    task_list.select_page_up(10);
                    return Ok(Control::Changed);
                }
                KeyCode::PageDown => {
                    task_list.select_page_down(10);
                    return Ok(Control::Changed);
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    return handle_task_list_select(state, ctx);
                }
                KeyCode::Char('/') => {
                    task_list.start_filter();
                    return Ok(Control::Changed);
                }
                KeyCode::Char('r') => {
                    task_list.toggle_recursive(&ctx.cache, &ctx.config);
                    return Ok(Control::Changed);
                }
                KeyCode::Char('s') => {
                    return handle_toggle_work_state_from_list(state, ctx);
                }
                KeyCode::Char('p') => {
                    return handle_toggle_priority_from_list(state, ctx);
                }
                KeyCode::Char('d') => {
                    return handle_done_from_list(state, ctx);
                }
                KeyCode::Char('x') => {
                    return handle_delete_from_list(state, ctx);
                }
                KeyCode::Char('e') => {
                    return handle_editor_from_list(state, ctx);
                }
                KeyCode::Char('u') => {
                    return handle_update_task_from_list(state, ctx);
                }
                KeyCode::Char('n') => {
                    return handle_new_task(state, ctx);
                }
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                    state.main_view = MainView::Dashboard;
                    state.nav_history.clear();
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
                KeyCode::PageUp => {
                    detail.scroll_page_up(10);
                    return Ok(Control::Changed);
                }
                KeyCode::PageDown => {
                    detail.scroll_page_down(10);
                    return Ok(Control::Changed);
                }
                KeyCode::Char(']') => {
                    detail.select_next_link();
                    return Ok(Control::Changed);
                }
                KeyCode::Char('[') => {
                    detail.select_prev_link();
                    return Ok(Control::Changed);
                }
                KeyCode::Enter => {
                    return handle_follow_link_from_task_detail(state, ctx);
                }
                KeyCode::Char('s') => {
                    return handle_toggle_work_state_from_detail(state, ctx);
                }
                KeyCode::Char('p') => {
                    return handle_toggle_priority_from_detail(state, ctx);
                }
                KeyCode::Char('d') => {
                    return handle_done_from_detail(state, ctx);
                }
                KeyCode::Char('x') => {
                    return handle_delete_from_detail(state, ctx);
                }
                KeyCode::Char('u') => {
                    return handle_update_task_from_detail(state, ctx);
                }
                KeyCode::Char('e') => {
                    return handle_editor_from_detail(state, ctx);
                }
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                    return handle_back_from_detail(state);
                }
                _ => {}
            },
            MainView::DocList(doc_list) => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    doc_list.select_prev();
                    return Ok(Control::Changed);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    doc_list.select_next();
                    return Ok(Control::Changed);
                }
                KeyCode::PageUp => {
                    doc_list.select_page_up(10);
                    return Ok(Control::Changed);
                }
                KeyCode::PageDown => {
                    doc_list.select_page_down(10);
                    return Ok(Control::Changed);
                }
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    return handle_doc_list_select(state, ctx);
                }
                KeyCode::Char('/') => {
                    doc_list.start_filter();
                    return Ok(Control::Changed);
                }
                KeyCode::Char('r') => {
                    doc_list.toggle_recursive(&ctx.cache);
                    return Ok(Control::Changed);
                }
                KeyCode::Char('e') => {
                    return handle_editor_from_doc_list(state, ctx);
                }
                KeyCode::Char('x') => {
                    return handle_delete_document_from_list(state, ctx);
                }
                KeyCode::Char('u') => {
                    return handle_update_document_from_list(state, ctx);
                }
                KeyCode::Char('n') => {
                    return handle_new_document(state);
                }
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                    state.main_view = MainView::Dashboard;
                    state.nav_history.clear();
                    return Ok(Control::Changed);
                }
                _ => {}
            },
            MainView::DocDetail { detail, .. } => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    detail.scroll_up();
                    return Ok(Control::Changed);
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    detail.scroll_down();
                    return Ok(Control::Changed);
                }
                KeyCode::PageUp => {
                    detail.scroll_page_up(10);
                    return Ok(Control::Changed);
                }
                KeyCode::PageDown => {
                    detail.scroll_page_down(10);
                    return Ok(Control::Changed);
                }
                KeyCode::Char(']') => {
                    detail.select_next_link();
                    return Ok(Control::Changed);
                }
                KeyCode::Char('[') => {
                    detail.select_prev_link();
                    return Ok(Control::Changed);
                }
                KeyCode::Enter => {
                    return handle_follow_link_from_doc_detail(state, ctx);
                }
                KeyCode::Char('e') => {
                    return handle_editor_from_doc_detail(state, ctx);
                }
                KeyCode::Char('x') => {
                    return handle_delete_document_from_detail(state, ctx);
                }
                KeyCode::Char('u') => {
                    return handle_update_document(state, ctx);
                }
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                    return handle_back_from_doc_detail(state);
                }
                _ => {}
            },
            MainView::Dashboard => {}
        }
    }

    // ':' opens command bar from anywhere (not during filter/overlay/mid-chord).
    if key.code == KeyCode::Char(':') && !state.keymap.is_pending() {
        state.command_bar = Some(CommandBarState::new());
        return Ok(Control::Changed);
    }

    // 'f' opens the feels dialog from anywhere (not mid-chord).
    if key.code == KeyCode::Char('f') && !state.keymap.is_pending() {
        let today = chrono::Local::now().date_naive();
        let dialog = if let Ok(Some(row)) = ctx.cache.get_today_feels(&today) {
            FeelsDialogState::with_values(row.energy, row.focus)
        } else {
            FeelsDialogState::new()
        };
        state.feels = Some(dialog);
        return Ok(Control::Changed);
    }

    // Global keymap (chords, actions).
    match state.keymap.process(*key) {
        KeymapResult::Matched(Action::Quit) => {
            // q navigates back through the view stack before quitting.
            match &state.main_view {
                MainView::TaskDetail { .. } => handle_back_from_detail(state),
                MainView::DocDetail { .. } => handle_back_from_doc_detail(state),
                MainView::TaskList(_) | MainView::DocList(_) => {
                    state.main_view = MainView::Dashboard;
                    state.nav_history.clear();
                    Ok(Control::Changed)
                }
                MainView::Dashboard => Ok(Control::Quit),
            }
        }
        KeymapResult::Matched(Action::GotoTasks) => {
            let meta_root = TaskCache::meta_root_id();
            let mut task_list =
                TaskListState::from_cache(&ctx.cache, meta_root, "All Projects", &ctx.config)?;
            task_list.toggle_recursive(&ctx.cache, &ctx.config);
            state.main_view = MainView::TaskList(Box::new(task_list));
            state.focus = FocusPanel::Main;
            state.nav_history.clear();
            Ok(Control::Changed)
        }
        KeymapResult::Matched(Action::GotoDocuments) => {
            let doc_list = DocumentListState::from_all_namespaces(&ctx.cache, &ctx.config)?;
            state.main_view = MainView::DocList(Box::new(doc_list));
            state.focus = FocusPanel::Main;
            state.nav_history.clear();
            Ok(Control::Changed)
        }
        KeymapResult::Matched(Action::GotoProjects) => {
            state.focus = FocusPanel::Sidebar;
            Ok(Control::Changed)
        }
        KeymapResult::Matched(Action::SearchAll) => {
            open_search(state, ctx, SearchFilter::All, "");
            Ok(Control::Changed)
        }
        KeymapResult::Matched(Action::SearchTasks) => {
            open_search(state, ctx, SearchFilter::Tasks, "");
            Ok(Control::Changed)
        }
        KeymapResult::Matched(Action::SearchDocuments) => {
            open_search(state, ctx, SearchFilter::Documents, "");
            Ok(Control::Changed)
        }
        KeymapResult::Matched(Action::Help) => {
            state.help = Some(HelpOverlayState::new());
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
        Some(TreeItemKind::Dashboard) => {
            state.main_view = MainView::Dashboard;
            state.focus = FocusPanel::Main;
            state.nav_history.clear();
            Ok(Control::Changed)
        }
        Some(TreeItemKind::Project) if !id.is_empty() => {
            let task_list = TaskListState::from_cache(&ctx.cache, &id, &name, &ctx.config)?;
            state.main_view = MainView::TaskList(Box::new(task_list));
            state.focus = FocusPanel::Main;
            state.nav_history.clear();
            Ok(Control::Changed)
        }
        Some(TreeItemKind::Namespace) if !id.is_empty() => {
            let doc_list = DocumentListState::from_cache(&ctx.cache, &id, &name, &ctx.config)?;
            state.main_view = MainView::DocList(Box::new(doc_list));
            state.focus = FocusPanel::Main;
            state.nav_history.clear();
            Ok(Control::Changed)
        }
        Some(TreeItemKind::SectionHeader) if name == "Projects" => {
            let meta_root = TaskCache::meta_root_id();
            let mut task_list =
                TaskListState::from_cache(&ctx.cache, meta_root, "All Projects", &ctx.config)?;
            task_list.toggle_recursive(&ctx.cache, &ctx.config);
            state.main_view = MainView::TaskList(Box::new(task_list));
            state.focus = FocusPanel::Main;
            state.nav_history.clear();
            Ok(Control::Changed)
        }
        Some(TreeItemKind::SectionHeader) if name == "Namespaces" => {
            let doc_list = DocumentListState::from_all_namespaces(&ctx.cache, &ctx.config)?;
            state.main_view = MainView::DocList(Box::new(doc_list));
            state.focus = FocusPanel::Main;
            state.nav_history.clear();
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

    let detail = Box::new(TaskDetailState::new(
        task,
        project_name,
        &ctx.cache,
        &ctx.config,
    ));

    // Move the list state into the detail variant for back-navigation.
    let prev = std::mem::replace(&mut state.main_view, MainView::Dashboard);
    let MainView::TaskList(list) = prev else {
        unreachable!();
    };
    state.main_view = MainView::TaskDetail { detail, list };

    Ok(Control::Changed)
}

// -- Task list mutation handlers --

fn handle_toggle_work_state_from_list(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskList(ref task_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(task_id) = task_list.selected_task_id().map(String::from) else {
        return Ok(Control::Continue);
    };
    crate::mutations::toggle_work_state(&ctx.storage, &ctx.cache, &task_id)?;
    if let MainView::TaskList(ref mut list) = state.main_view {
        list.refresh(&ctx.cache, &ctx.config);
    }
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

fn handle_toggle_priority_from_list(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskList(ref task_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(task_id) = task_list.selected_task_id().map(String::from) else {
        return Ok(Control::Continue);
    };
    crate::mutations::toggle_priority(&ctx.storage, &ctx.cache, &task_id)?;
    if let MainView::TaskList(ref mut list) = state.main_view {
        list.refresh(&ctx.cache, &ctx.config);
    }
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

fn handle_done_from_list(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    use crate::tui::confirm::PendingAction;

    let MainView::TaskList(ref task_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(task_id) = task_list.selected_task_id().map(String::from) else {
        return Ok(Control::Continue);
    };
    let title = task_list.selected_task_title().unwrap_or("").to_string();
    let descendant_count = ctx
        .cache
        .get_all_descendants(&task_id)
        .map(|d| d.len())
        .unwrap_or(0);
    state.confirm = Some(ConfirmState::new(PendingAction::Done {
        task_id,
        title,
        descendant_count,
    }));
    Ok(Control::Changed)
}

fn handle_delete_from_list(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    use crate::tui::confirm::PendingAction;

    let MainView::TaskList(ref task_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(task_id) = task_list.selected_task_id().map(String::from) else {
        return Ok(Control::Continue);
    };
    let title = task_list.selected_task_title().unwrap_or("").to_string();
    let child_count = ctx
        .cache
        .get_children(&task_id)
        .map(|c| c.len())
        .unwrap_or(0);
    state.confirm = Some(ConfirmState::new(PendingAction::Delete {
        task_id,
        title,
        child_count,
    }));
    Ok(Control::Changed)
}

// -- Editor handlers --

fn handle_editor_from_list(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskList(ref task_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(task_id) = task_list.selected_task_id().map(String::from) else {
        return Ok(Control::Continue);
    };
    let task = ctx.storage.load_task(&task_id)?;
    let result = run_editor(&task.title, &task.body, ctx)?;
    if let crate::editor::EditorResult::Changed { title, body } = result {
        crate::mutations::update_body(&ctx.storage, &ctx.cache, &task_id, title, body)?;
    }
    if let MainView::TaskList(ref mut list) = state.main_view {
        list.refresh(&ctx.cache, &ctx.config);
    }
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

fn handle_editor_from_detail(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskDetail { ref detail, .. } = state.main_view else {
        return Ok(Control::Continue);
    };
    let task_id = detail.task.id.clone();
    let task = ctx.storage.load_task(&task_id)?;
    let result = run_editor(&task.title, &task.body, ctx)?;
    if let crate::editor::EditorResult::Changed { title, body } = result {
        crate::mutations::update_body(&ctx.storage, &ctx.cache, &task_id, title, body)?;
    }
    if let MainView::TaskDetail {
        ref mut detail,
        ref mut list,
    } = state.main_view
    {
        detail.refresh(&ctx.storage, &ctx.cache, &ctx.config);
        list.refresh(&ctx.cache, &ctx.config);
    }
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

fn handle_editor_from_doc_detail(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::DocDetail { ref detail, .. } = state.main_view else {
        return Ok(Control::Continue);
    };
    let doc_id = detail.doc_id().to_string();
    let doc = ctx.storage.load_document(&doc_id)?;

    let result = run_editor(&doc.title, &doc.content, ctx)?;
    if let crate::editor::EditorResult::Changed { title, body } = result {
        crate::mutations::update_document_content(&ctx.storage, &ctx.cache, &doc_id, title, body)?;
    }

    refresh_current_view(state, ctx);
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

fn handle_new_document(state: &mut AppState) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::DocList(ref doc_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let namespace_id = doc_list.namespace_id.clone();
    let namespace_name = doc_list.namespace_name.clone();
    state.doc_form = Some(DocFormState::new(namespace_id, namespace_name));
    Ok(Control::Changed)
}

fn handle_update_document_from_list(
    state: &mut AppState,
    ctx: &Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::DocList(ref doc_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(doc_id) = doc_list.selected_id().map(String::from) else {
        return Ok(Control::Continue);
    };
    let namespace_name = doc_list.namespace_name.clone();
    open_update_doc_form(state, ctx, &doc_id, &namespace_name)
}

fn handle_update_document(
    state: &mut AppState,
    ctx: &Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::DocDetail {
        ref detail,
        ref list,
    } = state.main_view
    else {
        return Ok(Control::Continue);
    };
    let doc_id = detail.doc_id().to_string();
    let namespace_name = list.namespace_name.clone();
    open_update_doc_form(state, ctx, &doc_id, &namespace_name)
}

fn open_update_doc_form(
    state: &mut AppState,
    ctx: &Global,
    doc_id: &str,
    namespace_name: &str,
) -> Result<Control<AppEvent>, crate::Error> {
    let doc = ctx.storage.load_document(doc_id)?;
    state.doc_form = Some(DocFormState::for_update(doc, namespace_name.to_string()));
    Ok(Control::Changed)
}

fn handle_editor_from_doc_list(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::DocList(ref doc_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(doc_id) = doc_list.selected_id() else {
        return Ok(Control::Continue);
    };
    let doc_id = doc_id.to_string();
    let doc = ctx.storage.load_document(&doc_id)?;

    let result = run_editor(&doc.title, &doc.content, ctx)?;
    if let crate::editor::EditorResult::Changed { title, body } = result {
        crate::mutations::update_document_content(&ctx.storage, &ctx.cache, &doc_id, title, body)?;
    }

    refresh_current_view(state, ctx);
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

fn handle_delete_document_from_detail(
    state: &mut AppState,
    ctx: &Global,
) -> Result<Control<AppEvent>, crate::Error> {
    use crate::tui::confirm::PendingAction;

    let MainView::DocDetail { ref detail, .. } = state.main_view else {
        return Ok(Control::Continue);
    };
    let doc_id = detail.doc_id().to_string();
    let doc = ctx.storage.load_document(&doc_id)?;
    let child_count = ctx.cache.get_document_children(&doc_id)?.len();

    state.confirm = Some(ConfirmState::new(PendingAction::DeleteDocument {
        doc_id,
        title: doc.title,
        child_count,
    }));
    Ok(Control::Changed)
}

fn handle_delete_document_from_list(
    state: &mut AppState,
    ctx: &Global,
) -> Result<Control<AppEvent>, crate::Error> {
    use crate::tui::confirm::PendingAction;

    let MainView::DocList(ref doc_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(doc_id) = doc_list.selected_id() else {
        return Ok(Control::Continue);
    };
    let doc_id = doc_id.to_string();
    let doc = ctx.storage.load_document(&doc_id)?;
    let child_count = ctx.cache.get_document_children(&doc_id)?.len();

    state.confirm = Some(ConfirmState::new(PendingAction::DeleteDocument {
        doc_id,
        title: doc.title,
        child_count,
    }));
    Ok(Control::Changed)
}

/// Suspend the TUI, launch $EDITOR, resume TUI.
///
/// Returns the editor result (Changed/Unchanged/Cancelled).
/// Callers handle persistence based on entity type.
fn run_editor(
    title: &str,
    body: &str,
    ctx: &mut Global,
) -> crate::Result<crate::editor::EditorResult> {
    // Suspend TUI
    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;

    let result = crate::editor::edit_body(&ctx.config, title, body);

    // Resume TUI
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    // Force full repaint: ratatui's diff buffer is stale after the
    // editor used the terminal, so clear it to avoid a blank screen.
    ctx.clear_terminal();

    result
}

// -- Task detail mutation handlers --

fn handle_toggle_work_state_from_detail(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskDetail { ref detail, .. } = state.main_view else {
        return Ok(Control::Continue);
    };
    let task_id = detail.task.id.clone();
    crate::mutations::toggle_work_state(&ctx.storage, &ctx.cache, &task_id)?;
    if let MainView::TaskDetail {
        ref mut detail,
        ref mut list,
    } = state.main_view
    {
        detail.refresh(&ctx.storage, &ctx.cache, &ctx.config);
        list.refresh(&ctx.cache, &ctx.config);
    }
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

fn handle_toggle_priority_from_detail(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskDetail { ref detail, .. } = state.main_view else {
        return Ok(Control::Continue);
    };
    let task_id = detail.task.id.clone();
    crate::mutations::toggle_priority(&ctx.storage, &ctx.cache, &task_id)?;
    if let MainView::TaskDetail {
        ref mut detail,
        ref mut list,
    } = state.main_view
    {
        detail.refresh(&ctx.storage, &ctx.cache, &ctx.config);
        list.refresh(&ctx.cache, &ctx.config);
    }
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

fn handle_done_from_detail(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    use crate::tui::confirm::PendingAction;

    let MainView::TaskDetail { ref detail, .. } = state.main_view else {
        return Ok(Control::Continue);
    };
    let task_id = detail.task.id.clone();
    let title = detail.task.title.clone();
    let descendant_count = ctx
        .cache
        .get_all_descendants(&task_id)
        .map(|d| d.len())
        .unwrap_or(0);
    state.confirm = Some(ConfirmState::new(PendingAction::Done {
        task_id,
        title,
        descendant_count,
    }));
    Ok(Control::Changed)
}

fn handle_delete_from_detail(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    use crate::tui::confirm::PendingAction;

    let MainView::TaskDetail { ref detail, .. } = state.main_view else {
        return Ok(Control::Continue);
    };
    let task_id = detail.task.id.clone();
    let title = detail.task.title.clone();
    let child_count = ctx
        .cache
        .get_children(&task_id)
        .map(|c| c.len())
        .unwrap_or(0);
    state.confirm = Some(ConfirmState::new(PendingAction::Delete {
        task_id,
        title,
        child_count,
    }));
    Ok(Control::Changed)
}

// -- Command bar handlers --

fn handle_command_bar_input(
    code: KeyCode,
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    match code {
        KeyCode::Esc => {
            state.command_bar = None;
            Ok(Control::Changed)
        }
        KeyCode::Enter => {
            let cmd = state.command_bar.as_ref().unwrap().parse();
            state.command_bar = None;
            execute_command(cmd, state, ctx)
        }
        KeyCode::Backspace => {
            let bar = state.command_bar.as_mut().unwrap();
            if bar.is_empty() {
                state.command_bar = None;
            } else {
                bar.backspace();
            }
            Ok(Control::Changed)
        }
        KeyCode::Char(c) => {
            if let Some(ref mut bar) = state.command_bar {
                bar.char_input(c);
            }
            Ok(Control::Changed)
        }
        _ => Ok(Control::Continue),
    }
}

fn execute_command(
    cmd: crate::tui::command_bar::Command,
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    use crate::tui::command_bar::Command;

    match cmd {
        Command::Quit => Ok(Control::Quit),
        Command::New { title } => {
            // Determine project from current task list view
            let project_id = match &state.main_view {
                MainView::TaskList(list) => list.project_id.clone(),
                MainView::TaskDetail { list, .. } => list.project_id.clone(),
                MainView::Dashboard | MainView::DocList(_) | MainView::DocDetail { .. } => {
                    return Ok(Control::Changed);
                }
            };

            crate::mutations::create_task(
                &ctx.storage,
                &ctx.cache,
                &project_id,
                &title,
                "later",
                "M",
                None,
                None,
                vec![],
                None,
            )?;

            match &mut state.main_view {
                MainView::TaskList(list) => list.refresh(&ctx.cache, &ctx.config),
                MainView::TaskDetail { list, detail } => {
                    list.refresh(&ctx.cache, &ctx.config);
                    detail.refresh(&ctx.storage, &ctx.cache, &ctx.config);
                }
                _ => {}
            }
            trigger_background_sync(state, ctx);
            Ok(Control::Changed)
        }
        Command::Search { query } => {
            open_search(state, ctx, SearchFilter::All, &query);
            Ok(Control::Changed)
        }
        Command::Sync => {
            trigger_full_sync(state, ctx);
            Ok(Control::Changed)
        }
        Command::Feels { energy, focus } => {
            if let (Some(e), Some(f)) = (energy, focus) {
                save_feels(e, f, ctx);
            } else {
                let today = chrono::Local::now().date_naive();
                let dialog = if let Ok(Some(row)) = ctx.cache.get_today_feels(&today) {
                    FeelsDialogState::with_values(row.energy, row.focus)
                } else {
                    FeelsDialogState::new()
                };
                state.feels = Some(dialog);
            }
            Ok(Control::Changed)
        }
        Command::Help => {
            state.help = Some(HelpOverlayState::new());
            Ok(Control::Changed)
        }
        Command::DocNew { title } => {
            let namespace_id = match &state.main_view {
                MainView::DocList(dl) => dl.namespace_id.clone(),
                MainView::DocDetail { list, .. } => list.namespace_id.clone(),
                _ => return Ok(Control::Changed),
            };
            crate::mutations::create_document(
                &ctx.storage,
                &ctx.cache,
                &namespace_id,
                &title,
                String::new(),
                vec![],
                None,
            )?;
            refresh_current_view(state, ctx);
            trigger_background_sync(state, ctx);
            Ok(Control::Changed)
        }
        Command::Unknown(_) => {
            // Silently ignore unknown commands
            Ok(Control::Changed)
        }
    }
}

// -- Create form handlers --

fn handle_new_task(state: &mut AppState, ctx: &Global) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskList(ref task_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let project_id = task_list.project_id.clone();
    let project_name = task_list.project_name.clone();
    state.create_form = Some(TaskFormState::new(
        project_id,
        project_name,
        ctx.config.icon_theme,
    ));
    Ok(Control::Changed)
}

fn handle_update_task_from_list(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskList(ref task_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(task_id) = task_list.selected_task_id().map(String::from) else {
        return Ok(Control::Continue);
    };
    let project_name = task_list.project_name.clone();
    open_update_form(state, ctx, &task_id, &project_name)
}

fn handle_update_task_from_detail(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskDetail { ref detail, .. } = state.main_view else {
        return Ok(Control::Continue);
    };
    let task_id = detail.task.id.clone();
    let project_name = detail.project_name.clone();
    open_update_form(state, ctx, &task_id, &project_name)
}

fn open_update_form(
    state: &mut AppState,
    ctx: &mut Global,
    task_id: &str,
    project_name: &str,
) -> Result<Control<AppEvent>, crate::Error> {
    let task = ctx.storage.load_task(task_id)?;
    let mut form =
        TaskFormState::for_update(&task, project_name.to_string(), ctx.config.icon_theme);

    // Resolve existing parent if present
    if let Some(ref pid) = task.parent_id {
        let title = ctx
            .cache
            .get_task_summary(pid)
            .ok()
            .flatten()
            .map(|s| s.title);
        form.set_resolved_parent(title);
    }

    state.create_form = Some(form);
    Ok(Control::Changed)
}

fn submit_create_form(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let form = state.create_form.as_ref().unwrap();
    let project_id = form.project_id.clone();
    let title = form.title().to_string();
    let priority = form.priority().to_string();
    let size = form.size().to_string();
    let impact = form.impact();
    let joy = form.joy();
    let labels = form.labels().to_vec();
    let parent_id = form.parent_id().map(String::from);
    state.create_form = None;

    auto_register_labels(&labels, &project_id, &ctx.cache)?;

    crate::mutations::create_task(
        &ctx.storage,
        &ctx.cache,
        &project_id,
        &title,
        &priority,
        &size,
        Some(impact),
        Some(joy),
        labels,
        parent_id,
    )?;

    refresh_current_view(state, ctx);
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

fn submit_update_form(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let form = state.create_form.as_ref().unwrap();
    let task_id = form.task_id().unwrap().to_string();
    let project_id = form.project_id.clone();

    if !form.has_changes() {
        state.create_form = None;
        return Ok(Control::Changed);
    }

    // Validate parent before submitting
    if let Some(Some(ref pid)) = form.changed_fields().parent_id {
        if pid == &task_id {
            return Ok(Control::Continue); // can't parent to self
        }
        if ctx.cache.would_create_cycle(&task_id, pid)? {
            return Ok(Control::Continue); // would create cycle
        }
    }

    let changes = form.changed_fields();
    let labels_for_registry = changes.labels.clone();
    state.create_form = None;

    // Auto-register new labels
    if let Some(ref new_labels) = labels_for_registry {
        auto_register_labels(new_labels, &project_id, &ctx.cache)?;
    }

    crate::mutations::update_task(
        &ctx.storage,
        &ctx.cache,
        &task_id,
        changes.title,
        changes.priority,
        changes.size,
        changes.impact,
        changes.joy,
        changes.labels,
        changes.parent_id,
    )?;

    refresh_current_view(state, ctx);
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

/// Register any new labels in the project label registry.
fn auto_register_labels(
    labels: &[String],
    project_id: &str,
    cache: &TaskCache,
) -> crate::Result<()> {
    if labels.is_empty() {
        return Ok(());
    }
    let mut project_labels = cache.get_project_labels(project_id)?;
    let mut changed = false;
    for label in labels {
        if !project_labels.contains(label) {
            project_labels.push(label.clone());
            changed = true;
        }
    }
    if changed {
        cache.set_project_labels(project_id, &project_labels)?;
    }
    Ok(())
}

/// Refresh the current view (task list and/or detail) after a mutation.
fn refresh_current_view(state: &mut AppState, ctx: &mut Global) {
    match &mut state.main_view {
        MainView::TaskList(list) => list.refresh(&ctx.cache, &ctx.config),
        MainView::TaskDetail { detail, list } => {
            detail.refresh(&ctx.storage, &ctx.cache, &ctx.config);
            list.refresh(&ctx.cache, &ctx.config);
        }
        MainView::DocList(list) => list.refresh(&ctx.cache, &ctx.config),
        MainView::DocDetail { detail, list } => {
            detail.refresh(&ctx.storage, &ctx.cache, &ctx.config);
            list.refresh(&ctx.cache, &ctx.config);
        }
        MainView::Dashboard => {}
    }
}

fn handle_create_form_input(
    key: &crossterm::event::KeyEvent,
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let code = key.code;
    let mods = key.modifiers;

    // Ctrl-Right/Left for page switching (always available)
    if mods.contains(KeyModifiers::CONTROL) {
        match code {
            KeyCode::Right => {
                if let Some(ref mut form) = state.create_form {
                    form.next_page();
                }
                return Ok(Control::Changed);
            }
            KeyCode::Left => {
                if let Some(ref mut form) = state.create_form {
                    form.prev_page();
                }
                return Ok(Control::Changed);
            }
            _ => {}
        }
    }

    match code {
        KeyCode::Esc => {
            state.create_form = None;
            Ok(Control::Changed)
        }
        KeyCode::Enter => {
            let focused = state.create_form.as_ref().unwrap().focused();

            // Cancel button
            if focused == FormField::Cancel {
                state.create_form = None;
                return Ok(Control::Changed);
            }

            // When focused on Labels with pending input, commit the label
            if let Some(ref mut form) = state.create_form
                && focused == FormField::Labels
                && form.has_pending_label()
            {
                form.commit_label();
                return Ok(Control::Changed);
            }

            let form = state.create_form.as_ref().unwrap();
            if !form.can_submit() {
                return Ok(Control::Continue);
            }

            match form.task_id() {
                Some(_) => submit_update_form(state, ctx),
                None => submit_create_form(state, ctx),
            }
        }
        KeyCode::Tab => {
            if let Some(ref mut form) = state.create_form {
                form.focus_next();
            }
            Ok(Control::Changed)
        }
        KeyCode::BackTab => {
            if let Some(ref mut form) = state.create_form {
                form.focus_prev();
            }
            Ok(Control::Changed)
        }
        KeyCode::Backspace => {
            if let Some(ref mut form) = state.create_form {
                form.backspace();
                resolve_parent_if_needed(form, &ctx.cache);
            }
            Ok(Control::Changed)
        }
        KeyCode::Left | KeyCode::Right => {
            if let Some(ref mut form) = state.create_form {
                match form.focused() {
                    // Text fields: Left/Right would move cursor — not
                    // implemented yet, so ignore.
                    FormField::Title | FormField::Labels | FormField::Parent => {}
                    // Numeric/toggle fields: adjust value
                    _ => {
                        let delta = if code == KeyCode::Right { 1 } else { -1 };
                        form.adjust_field(delta);
                    }
                }
            }
            Ok(Control::Changed)
        }
        KeyCode::Char(' ') => {
            if let Some(ref mut form) = state.create_form {
                form.toggle_or_space();
            }
            Ok(Control::Changed)
        }
        KeyCode::Char(c) => {
            if let Some(ref mut form) = state.create_form {
                form.char_input(c);
                resolve_parent_if_needed(form, &ctx.cache);
            }
            Ok(Control::Changed)
        }
        _ => Ok(Control::Continue),
    }
}

/// Attempt to resolve a parent ID after typing in the parent field.
fn resolve_parent_if_needed(form: &mut TaskFormState, cache: &TaskCache) {
    if form.focused() != FormField::Parent {
        return;
    }
    let input = form.parent_input();
    if input.is_empty() {
        form.set_resolved_parent(None);
        form.set_parent_id(None);
        return;
    }
    match crate::utils::resolve_task_id_from_cache(cache, input) {
        Ok(full_id) => {
            let title = cache
                .get_task_summary(&full_id)
                .ok()
                .flatten()
                .map(|s| s.title);
            form.set_resolved_parent(title);
            form.set_parent_id(Some(full_id));
        }
        Err(_) => {
            form.set_resolved_parent(None);
            form.set_parent_id(None);
        }
    }
}

// -- Document form handlers --

fn handle_doc_form_input(
    key: &crossterm::event::KeyEvent,
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let code = key.code;

    match code {
        KeyCode::Esc => {
            state.doc_form = None;
            Ok(Control::Changed)
        }
        KeyCode::Enter => {
            let focused = state.doc_form.as_ref().unwrap().focused();

            // Cancel button
            if focused == DocFormField::Cancel {
                state.doc_form = None;
                return Ok(Control::Changed);
            }

            // When focused on Labels with pending input, commit the label
            if let Some(ref mut form) = state.doc_form
                && focused == DocFormField::Labels
                && form.has_pending_label()
            {
                form.commit_label();
                return Ok(Control::Changed);
            }

            let form = state.doc_form.as_ref().unwrap();
            if !form.can_submit() {
                return Ok(Control::Continue);
            }

            match form.doc_id() {
                Some(_) => submit_doc_update_form(state, ctx),
                None => submit_doc_create_form(state, ctx),
            }
        }
        KeyCode::Tab => {
            if let Some(ref mut form) = state.doc_form {
                form.focus_next();
            }
            Ok(Control::Changed)
        }
        KeyCode::BackTab => {
            if let Some(ref mut form) = state.doc_form {
                form.focus_prev();
            }
            Ok(Control::Changed)
        }
        KeyCode::Backspace => {
            if let Some(ref mut form) = state.doc_form {
                form.backspace();
                resolve_doc_parent_if_needed(form, &ctx.cache);
            }
            Ok(Control::Changed)
        }
        KeyCode::Char(c) => {
            if let Some(ref mut form) = state.doc_form {
                form.char_input(c);
                resolve_doc_parent_if_needed(form, &ctx.cache);
            }
            Ok(Control::Changed)
        }
        _ => Ok(Control::Continue),
    }
}

fn submit_doc_create_form(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let form = state.doc_form.as_ref().unwrap();
    let namespace_id = match form.mode() {
        crate::tui::doc_form::DocFormMode::Create { namespace_id } => namespace_id.clone(),
        crate::tui::doc_form::DocFormMode::Update { .. } => unreachable!(),
    };
    let title = form.title().to_string();
    let labels = form.labels().to_vec();
    let parent_id = form.parent_id().map(String::from);
    state.doc_form = None;

    crate::mutations::create_document(
        &ctx.storage,
        &ctx.cache,
        &namespace_id,
        &title,
        String::new(),
        labels,
        parent_id,
    )?;

    refresh_current_view(state, ctx);
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

fn submit_doc_update_form(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let form = state.doc_form.as_ref().unwrap();
    let doc_id = form.doc_id().unwrap().to_string();

    if !form.has_changes() {
        state.doc_form = None;
        return Ok(Control::Changed);
    }

    let changed_title = form.changed_title();
    let changed_labels = form.changed_labels();
    let changed_parent_id = form.changed_parent_id();
    state.doc_form = None;

    crate::mutations::update_document_metadata(
        &ctx.storage,
        &ctx.cache,
        &doc_id,
        changed_title,
        changed_labels,
        changed_parent_id,
    )?;

    refresh_current_view(state, ctx);
    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

/// Attempt to resolve a document parent ID after typing in the parent field.
fn resolve_doc_parent_if_needed(form: &mut DocFormState, cache: &TaskCache) {
    if form.focused() != DocFormField::Parent {
        return;
    }
    let input = form.parent_input();
    if input.is_empty() {
        form.set_resolved_parent(None);
        form.set_parent_id(None);
        return;
    }
    match crate::utils::resolve_document_id(cache, input) {
        Ok(full_id) => {
            let title = cache.get_document(&full_id).ok().flatten().map(|d| d.title);
            form.set_resolved_parent(title);
            form.set_parent_id(Some(full_id));
        }
        Err(_) => {
            form.set_resolved_parent(None);
            form.set_parent_id(None);
        }
    }
}

// -- Search overlay handlers --

/// Open the search overlay with the given filter and optional initial query.
fn open_search(state: &mut AppState, ctx: &Global, filter: SearchFilter, query: &str) {
    let mut search = SearchOverlayState::new(filter, query);
    if !query.is_empty() {
        search.update_results(&ctx.cache);
    }
    state.search = Some(search);
}

/// Handle key input while the search overlay is active.
fn handle_search_input(
    code: KeyCode,
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    match code {
        KeyCode::Esc => {
            state.search = None;
            Ok(Control::Changed)
        }
        KeyCode::Enter => {
            let result = state
                .search
                .as_ref()
                .and_then(|s| s.selected_result())
                .cloned();
            state.search = None;

            if let Some(result) = result {
                let target = match result.kind {
                    SearchResultKind::Task => NavTarget::Task { id: result.id },
                    SearchResultKind::Document => NavTarget::Document { id: result.id },
                };
                state.focus = FocusPanel::Main;
                navigate_to_entity(state, ctx, &target)
            } else {
                Ok(Control::Changed)
            }
        }
        KeyCode::Up => {
            if let Some(ref mut search) = state.search {
                search.select_prev();
            }
            Ok(Control::Changed)
        }
        KeyCode::Down => {
            if let Some(ref mut search) = state.search {
                search.select_next();
            }
            Ok(Control::Changed)
        }
        KeyCode::Backspace => {
            let should_close = state.search.as_ref().map(|s| s.is_empty()).unwrap_or(false);
            if should_close {
                state.search = None;
            } else if let Some(ref mut search) = state.search {
                search.backspace();
                search.update_results(&ctx.cache);
            }
            Ok(Control::Changed)
        }
        KeyCode::Char(c) => {
            if let Some(ref mut search) = state.search {
                search.char_input(c);
                search.update_results(&ctx.cache);
            }
            Ok(Control::Changed)
        }
        _ => Ok(Control::Continue),
    }
}

// -- Help overlay handlers --

/// Handle key input while the help overlay is active.
fn handle_help_input(
    code: KeyCode,
    state: &mut AppState,
) -> Result<Control<AppEvent>, crate::Error> {
    match code {
        KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
            state.help = None;
            Ok(Control::Changed)
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(ref mut help) = state.help {
                help.prev_page();
            }
            Ok(Control::Changed)
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(ref mut help) = state.help {
                help.next_page();
            }
            Ok(Control::Changed)
        }
        _ => Ok(Control::Continue),
    }
}

// -- Feels dialog handlers --

/// Handle key input while the feels dialog is active.
fn handle_feels_input(
    code: KeyCode,
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    match code {
        KeyCode::Esc => {
            state.feels = None;
            Ok(Control::Changed)
        }
        KeyCode::Enter => {
            if let Some(feels) = state.feels.take() {
                save_feels(feels.energy, feels.focus, ctx);
            }
            Ok(Control::Changed)
        }
        KeyCode::Tab | KeyCode::Down | KeyCode::Char('j') => {
            if let Some(ref mut feels) = state.feels {
                feels.next_field();
            }
            Ok(Control::Changed)
        }
        KeyCode::BackTab | KeyCode::Up | KeyCode::Char('k') => {
            if let Some(ref mut feels) = state.feels {
                feels.prev_field();
            }
            Ok(Control::Changed)
        }
        KeyCode::Right | KeyCode::Char('l') => {
            if let Some(ref mut feels) = state.feels {
                feels.increment();
            }
            Ok(Control::Changed)
        }
        KeyCode::Left | KeyCode::Char('h') => {
            if let Some(ref mut feels) = state.feels {
                feels.decrement();
            }
            Ok(Control::Changed)
        }
        _ => Ok(Control::Continue),
    }
}

/// Save feels to the cache.
fn save_feels(energy: u8, focus: u8, ctx: &Global) {
    let today = chrono::Local::now().date_naive();
    let _ = ctx.cache.upsert_feels(&today, energy, focus);
}

/// Handle key input while the confirmation dialog is active.
fn handle_confirm_input(
    code: KeyCode,
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    match code {
        KeyCode::Char('n') | KeyCode::Esc => {
            state.confirm = None;
            Ok(Control::Changed)
        }
        KeyCode::Char('y') => {
            let confirm = state.confirm.take().unwrap();
            execute_confirmed_action(confirm, state, ctx)
        }
        KeyCode::Enter => {
            if state.confirm.as_ref().unwrap().is_confirmed() {
                let confirm = state.confirm.take().unwrap();
                execute_confirmed_action(confirm, state, ctx)
            } else {
                state.confirm = None;
                Ok(Control::Changed)
            }
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Tab | KeyCode::Char('h') | KeyCode::Char('l') => {
            if let Some(ref mut confirm) = state.confirm {
                confirm.toggle();
            }
            Ok(Control::Changed)
        }
        _ => Ok(Control::Continue),
    }
}

/// Execute the confirmed destructive action and refresh the view.
fn execute_confirmed_action(
    confirm: ConfirmState,
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    use crate::mutations;
    use crate::tui::confirm::PendingAction;

    match confirm.action {
        PendingAction::Done { ref task_id, .. } => {
            mutations::mark_done(&ctx.storage, &ctx.cache, task_id)?;
        }
        PendingAction::Delete { ref task_id, .. } => {
            mutations::delete_task(&ctx.storage, &ctx.cache, task_id)?;
        }
        PendingAction::DeleteDocument { ref doc_id, .. } => {
            mutations::delete_document(&ctx.storage, &ctx.cache, doc_id)?;
        }
    }

    // If we're in detail view for the affected task, go back to list
    let affected_id = match &confirm.action {
        PendingAction::Done { task_id, .. } | PendingAction::Delete { task_id, .. } => {
            task_id.clone()
        }
        PendingAction::DeleteDocument { doc_id, .. } => doc_id.clone(),
    };

    match &state.main_view {
        MainView::TaskDetail { detail, .. } if detail.task.id == affected_id => {
            // Navigate back and refresh
            let prev = std::mem::replace(&mut state.main_view, MainView::Dashboard);
            if let MainView::TaskDetail { mut list, .. } = prev {
                list.refresh(&ctx.cache, &ctx.config);
                state.main_view = MainView::TaskList(list);
            }
        }
        MainView::TaskList(_) => {
            if let MainView::TaskList(ref mut list) = state.main_view {
                list.refresh(&ctx.cache, &ctx.config);
            }
        }
        MainView::TaskDetail { .. } => {
            // Detail for a different task — refresh detail and list
            if let MainView::TaskDetail {
                ref mut detail,
                ref mut list,
            } = state.main_view
            {
                detail.refresh(&ctx.storage, &ctx.cache, &ctx.config);
                list.refresh(&ctx.cache, &ctx.config);
            }
        }
        MainView::DocDetail { detail, .. } if detail.doc_id() == affected_id => {
            let prev = std::mem::replace(&mut state.main_view, MainView::Dashboard);
            if let MainView::DocDetail { mut list, .. } = prev {
                list.refresh(&ctx.cache, &ctx.config);
                state.main_view = MainView::DocList(list);
            }
        }
        MainView::DocList(_) => {
            if let MainView::DocList(ref mut list) = state.main_view {
                list.refresh(&ctx.cache, &ctx.config);
            }
        }
        MainView::DocDetail { .. } => {}
        _ => {}
    }

    trigger_background_sync(state, ctx);
    Ok(Control::Changed)
}

/// Navigate back from task detail: deselect link, pop history, or restore list.
fn handle_back_from_detail(state: &mut AppState) -> Result<Control<AppEvent>, crate::Error> {
    // 1. If a link is selected, deselect it first.
    if let MainView::TaskDetail { ref mut detail, .. } = state.main_view
        && detail.deselect_link()
    {
        return Ok(Control::Changed);
    }

    // 2. If nav_history has entries, pop and restore.
    if let Some(prev_view) = state.nav_history.pop() {
        state.main_view = prev_view;
        return Ok(Control::Changed);
    }

    // 3. Fall back to restoring the list.
    let prev = std::mem::replace(&mut state.main_view, MainView::Dashboard);
    if let MainView::TaskDetail { list, .. } = prev {
        state.main_view = MainView::TaskList(list);
    }
    Ok(Control::Changed)
}

/// Handle Enter on a document in the document list.
fn handle_doc_list_select(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::DocList(ref doc_list) = state.main_view else {
        return Ok(Control::Continue);
    };

    let Some(doc_id) = doc_list.selected_id() else {
        return Ok(Control::Continue);
    };
    let doc_id = doc_id.to_string();
    let ns_name = doc_list.namespace_name.clone();

    let doc = match ctx.storage.load_document(&doc_id) {
        Ok(doc) => doc,
        Err(_) => return Ok(Control::Continue),
    };

    let detail = Box::new(DocumentDetailState::new(
        doc,
        ns_name,
        &ctx.cache,
        &ctx.config,
    ));

    let prev = std::mem::replace(&mut state.main_view, MainView::Dashboard);
    let MainView::DocList(list) = prev else {
        unreachable!();
    };
    state.main_view = MainView::DocDetail { detail, list };

    Ok(Control::Changed)
}

/// Navigate back from document detail: deselect link, pop history, or restore list.
fn handle_back_from_doc_detail(state: &mut AppState) -> Result<Control<AppEvent>, crate::Error> {
    // 1. If a link is selected, deselect it first.
    if let MainView::DocDetail { ref mut detail, .. } = state.main_view
        && detail.deselect_link()
    {
        return Ok(Control::Changed);
    }

    // 2. If nav_history has entries, pop and restore.
    if let Some(prev_view) = state.nav_history.pop() {
        state.main_view = prev_view;
        return Ok(Control::Changed);
    }

    // 3. Fall back to restoring the list.
    let prev = std::mem::replace(&mut state.main_view, MainView::Dashboard);
    if let MainView::DocDetail { list, .. } = prev {
        state.main_view = MainView::DocList(list);
    }
    Ok(Control::Changed)
}

/// Follow the selected link in the task detail view.
fn handle_follow_link_from_task_detail(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskDetail { ref detail, .. } = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(target) = detail.selected_nav_target().cloned() else {
        return Ok(Control::Continue);
    };
    navigate_to_entity(state, ctx, &target)
}

/// Follow the selected link in the document detail view.
fn handle_follow_link_from_doc_detail(
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::DocDetail { ref detail, .. } = state.main_view else {
        return Ok(Control::Continue);
    };
    let Some(target) = detail.selected_nav_target().cloned() else {
        return Ok(Control::Continue);
    };
    navigate_to_entity(state, ctx, &target)
}

/// Navigate to a target entity, pushing the current view onto the history stack.
fn navigate_to_entity(
    state: &mut AppState,
    ctx: &mut Global,
    target: &NavTarget,
) -> Result<Control<AppEvent>, crate::Error> {
    match target {
        NavTarget::Task { id } => {
            let task = match ctx.storage.load_task(id) {
                Ok(t) => t,
                Err(_) => return Ok(Control::Continue),
            };

            // Resolve project name for the detail view.
            let project_name = ctx
                .cache
                .get_project(&task.project_id)
                .ok()
                .flatten()
                .map(|p| p.name)
                .unwrap_or_else(|| task.project_id.clone());

            let detail = Box::new(TaskDetailState::new(
                task.clone(),
                project_name.clone(),
                &ctx.cache,
                &ctx.config,
            ));

            // Build a minimal list for back-navigation context.
            let list = Box::new(TaskListState::from_cache(
                &ctx.cache,
                &task.project_id,
                &project_name,
                &ctx.config,
            )?);

            // Push current view onto history (bounded).
            let current =
                std::mem::replace(&mut state.main_view, MainView::TaskDetail { detail, list });
            state.nav_history.push(current);
            if state.nav_history.len() > NAV_HISTORY_LIMIT {
                state.nav_history.remove(0);
            }

            Ok(Control::Changed)
        }
        NavTarget::Document { id } => {
            let doc = match ctx.storage.load_document(id) {
                Ok(d) => d,
                Err(_) => return Ok(Control::Continue),
            };

            // Resolve namespace name for the detail view.
            let namespace_name = ctx
                .cache
                .get_namespace(&doc.namespace_id)
                .ok()
                .flatten()
                .map(|n| n.name)
                .unwrap_or_else(|| doc.namespace_id.clone());

            let detail = Box::new(DocumentDetailState::new(
                doc.clone(),
                namespace_name.clone(),
                &ctx.cache,
                &ctx.config,
            ));

            // Build a minimal list for back-navigation context.
            let list = Box::new(DocumentListState::from_cache(
                &ctx.cache,
                &doc.namespace_id,
                &namespace_name,
                &ctx.config,
            )?);

            // Push current view onto history (bounded).
            let current =
                std::mem::replace(&mut state.main_view, MainView::DocDetail { detail, list });
            state.nav_history.push(current);
            if state.nav_history.len() > NAV_HISTORY_LIMIT {
                state.nav_history.remove(0);
            }

            Ok(Control::Changed)
        }
    }
}

/// Spawn a background sync to push local mutations to the server.
/// Bidirectional sync (push + pull) for the `:sync` command.
fn trigger_full_sync(state: &mut AppState, ctx: &mut Global) {
    state.sync_status = SyncStatus::Syncing;
    let config = ctx.config.clone();
    let cache_path = ctx.config.cache_dir.join("index.db");
    let storage_config = ctx.storage.config().clone();
    let _ = ctx.spawn(move || {
        let success = (|| -> crate::Result<bool> {
            let cache = crate::cache::TaskCache::open(&cache_path)?;
            let storage = crate::storage::TaskStorage::new(storage_config);
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(crate::Error::Io)?;
            rt.block_on(async {
                let sync = crate::sync::SyncManager::new(&config, storage, cache)?;
                sync.sync_full().await?;
                Ok::<bool, crate::Error>(true)
            })
        })()
        .unwrap_or(false);
        Ok(Control::Event(AppEvent::SyncComplete(success)))
    });
}

/// Push-only sync for automatic sync after local mutations.
fn trigger_background_sync(state: &mut AppState, ctx: &mut Global) {
    state.sync_status = SyncStatus::Syncing;
    let config = ctx.config.clone();
    let cache_path = ctx.config.cache_dir.join("index.db");
    let storage_config = ctx.storage.config().clone();
    let _ = ctx.spawn(move || {
        let success = (|| -> crate::Result<bool> {
            let cache = crate::cache::TaskCache::open(&cache_path)?;
            let storage = crate::storage::TaskStorage::new(storage_config);
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(crate::Error::Io)?;
            rt.block_on(async {
                let sync = crate::sync::SyncManager::new(&config, storage, cache)?;
                Ok::<bool, crate::Error>(sync.try_sync(std::time::Duration::from_secs(3)).await)
            })
        })()
        .unwrap_or(false);
        Ok(Control::Event(AppEvent::SyncComplete(success)))
    });
}

/// Render the status bar with context-aware key hints.
fn render_status_bar(state: &AppState, area: Rect, buf: &mut Buffer) {
    let theme = &state.theme;

    let mut hints: Vec<Span<'_>> = Vec::new();

    match (&state.main_view, state.focus) {
        (MainView::TaskDetail { detail, .. }, FocusPanel::Main) => {
            hints.extend([
                Span::styled(" Esc", theme.status_key),
                Span::styled(" back", theme.status_desc),
                Span::styled("  j", theme.status_key),
                Span::styled("/", theme.status_desc),
                Span::styled("k", theme.status_key),
                Span::styled(" scroll", theme.status_desc),
            ]);
            if detail.has_nav_links() {
                hints.extend([
                    Span::styled("  ]", theme.status_key),
                    Span::styled("/", theme.status_desc),
                    Span::styled("[", theme.status_key),
                    Span::styled(" links", theme.status_desc),
                    Span::styled("  Enter", theme.status_key),
                    Span::styled(" follow", theme.status_desc),
                ]);
            }
            hints.extend([
                Span::styled("  s", theme.status_key),
                Span::styled(" start/stop", theme.status_desc),
                Span::styled("  p", theme.status_key),
                Span::styled(" priority", theme.status_desc),
                Span::styled("  d", theme.status_key),
                Span::styled(" done", theme.status_desc),
                Span::styled("  x", theme.status_key),
                Span::styled(" delete", theme.status_desc),
                Span::styled("  u", theme.status_key),
                Span::styled(" update", theme.status_desc),
                Span::styled("  e", theme.status_key),
                Span::styled(" edit", theme.status_desc),
            ]);
        }
        (MainView::DocDetail { detail, .. }, FocusPanel::Main) => {
            hints.extend([
                Span::styled(" Esc", theme.status_key),
                Span::styled(" back", theme.status_desc),
                Span::styled("  j", theme.status_key),
                Span::styled("/", theme.status_desc),
                Span::styled("k", theme.status_key),
                Span::styled(" scroll", theme.status_desc),
            ]);
            if detail.has_nav_links() {
                hints.extend([
                    Span::styled("  ]", theme.status_key),
                    Span::styled("/", theme.status_desc),
                    Span::styled("[", theme.status_key),
                    Span::styled(" links", theme.status_desc),
                    Span::styled("  Enter", theme.status_key),
                    Span::styled(" follow", theme.status_desc),
                ]);
            }
            hints.extend([
                Span::styled("  u", theme.status_key),
                Span::styled(" update", theme.status_desc),
                Span::styled("  e", theme.status_key),
                Span::styled(" edit", theme.status_desc),
                Span::styled("  x", theme.status_key),
                Span::styled(" delete", theme.status_desc),
            ]);
        }
        (MainView::DocList(dl), FocusPanel::Main) if dl.is_filtering() => {
            hints.extend([
                Span::styled(" Esc", theme.status_key),
                Span::styled(" cancel", theme.status_desc),
                Span::styled("  type to filter", theme.status_desc),
            ]);
        }
        (MainView::DocList(_), FocusPanel::Main) => {
            hints.extend([
                Span::styled(" Esc", theme.status_key),
                Span::styled(" back", theme.status_desc),
                Span::styled("  j", theme.status_key),
                Span::styled("/", theme.status_desc),
                Span::styled("k", theme.status_key),
                Span::styled(" nav", theme.status_desc),
                Span::styled("  Enter", theme.status_key),
                Span::styled(" open", theme.status_desc),
                Span::styled("  /", theme.status_key),
                Span::styled(" filter", theme.status_desc),
                Span::styled("  r", theme.status_key),
                Span::styled(" recursive", theme.status_desc),
                Span::styled("  n", theme.status_key),
                Span::styled(" new", theme.status_desc),
                Span::styled("  u", theme.status_key),
                Span::styled(" update", theme.status_desc),
                Span::styled("  e", theme.status_key),
                Span::styled(" edit", theme.status_desc),
                Span::styled("  x", theme.status_key),
                Span::styled(" delete", theme.status_desc),
            ]);
        }
        (MainView::TaskList(tl), FocusPanel::Main) if tl.is_filtering() => {
            hints.extend([
                Span::styled(" Esc", theme.status_key),
                Span::styled(" cancel", theme.status_desc),
                Span::styled("  type to filter", theme.status_desc),
            ]);
        }
        (MainView::TaskList(tl), FocusPanel::Main) => {
            hints.extend([
                Span::styled(" Esc", theme.status_key),
                Span::styled(" back", theme.status_desc),
                Span::styled("  j", theme.status_key),
                Span::styled("/", theme.status_desc),
                Span::styled("k", theme.status_key),
                Span::styled(" nav", theme.status_desc),
            ]);
            if !tl.is_empty() {
                hints.extend([
                    Span::styled("  Enter", theme.status_key),
                    Span::styled(" open", theme.status_desc),
                ]);
            }
            hints.extend([
                Span::styled("  /", theme.status_key),
                Span::styled(" filter", theme.status_desc),
                Span::styled("  r", theme.status_key),
                Span::styled(" recursive", theme.status_desc),
                Span::styled("  s", theme.status_key),
                Span::styled(" start/stop", theme.status_desc),
                Span::styled("  p", theme.status_key),
                Span::styled(" priority", theme.status_desc),
                Span::styled("  d", theme.status_key),
                Span::styled(" done", theme.status_desc),
                Span::styled("  x", theme.status_key),
                Span::styled(" delete", theme.status_desc),
                Span::styled("  u", theme.status_key),
                Span::styled(" update", theme.status_desc),
                Span::styled("  e", theme.status_key),
                Span::styled(" edit", theme.status_desc),
                Span::styled("  n", theme.status_key),
                Span::styled(" new", theme.status_desc),
            ]);
        }
        _ => {
            hints.extend([
                Span::styled(" q", theme.status_key),
                Span::styled(" quit", theme.status_desc),
                Span::styled("  g", theme.status_key),
                Span::styled(" goto", theme.status_desc),
                Span::styled("  S", theme.status_key),
                Span::styled(" search", theme.status_desc),
            ]);
        }
    }

    hints.extend([
        Span::styled("  Tab", theme.status_key),
        Span::styled(" focus", theme.status_desc),
    ]);

    if state.focus == FocusPanel::Sidebar {
        hints.extend([
            Span::styled("  j", theme.status_key),
            Span::styled("/", theme.status_desc),
            Span::styled("k", theme.status_key),
            Span::styled(" nav", theme.status_desc),
            Span::styled("  Enter", theme.status_key),
            Span::styled(" open", theme.status_desc),
        ]);
    }

    hints.extend([
        Span::styled("  f", theme.status_key),
        Span::styled(" feels", theme.status_desc),
        Span::styled("  ?", theme.status_key),
        Span::styled(" help", theme.status_desc),
    ]);

    match state.sync_status {
        SyncStatus::Syncing => hints.push(Span::styled("  syncing", theme.muted)),
        SyncStatus::Synced => hints.push(Span::styled("  synced", theme.success)),
        SyncStatus::Failed => hints.push(Span::styled("  sync failed", theme.danger)),
        SyncStatus::Idle => {}
    }

    Line::from(hints).style(theme.status_bar).render(area, buf);
}

fn handle_error(
    err: crate::Error,
    _state: &mut AppState,
    _ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    Err(err)
}
