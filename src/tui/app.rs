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
use crate::tui::create_form::CreateFormState;
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
}

/// UI widget state tree.
pub struct AppState {
    pub keymap: Keymap,
    pub theme: Theme,
    pub sidebar: SidebarState,
    pub focus: FocusPanel,
    pub main_view: MainView,
    pub confirm: Option<ConfirmState>,
    pub create_form: Option<CreateFormState>,
    pub command_bar: Option<CommandBarState>,
    pub sync_status: SyncStatus,
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
        command_bar: None,
        sync_status: SyncStatus::Idle,
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

    // Sidebar
    state
        .sidebar
        .render(theme, sidebar_focused, columns[0], buf);

    // Main area — dispatch based on current view
    match &mut state.main_view {
        MainView::Dashboard => render_dashboard(theme, main_focused, columns[1], buf),
        MainView::TaskList(task_list) => task_list.render(theme, main_focused, columns[1], buf),
        MainView::TaskDetail { detail, .. } => {
            detail.render(theme, main_focused, columns[1], buf);
        }
    }

    // Status bar (or command bar when active)
    if let Some(ref cmd_bar) = state.command_bar {
        cmd_bar.render(theme, layout[1], buf);
    } else {
        render_status_bar(state, layout[1], buf);
    }

    // Overlay dialogs (above content, below which-key)
    if let Some(ref confirm) = state.confirm {
        confirm.render(theme, area, buf);
    }
    if let Some(ref form) = state.create_form {
        form.render(theme, area, buf);
    }

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
    if let AppEvent::SyncComplete(success) = event {
        state.sync_status = if *success {
            SyncStatus::Synced
        } else {
            SyncStatus::Failed
        };
        return Ok(Control::Changed);
    }

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

    // Confirmation dialog intercepts all input when active.
    if state.confirm.is_some() {
        return handle_confirm_input(key.code, state, ctx);
    }

    // Create form intercepts all input when active.
    if state.create_form.is_some() {
        return handle_create_form_input(key.code, state, ctx);
    }

    // Command bar intercepts all input when active.
    if state.command_bar.is_some() {
        return handle_command_bar_input(key.code, state, ctx);
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
                KeyCode::Char('n') => {
                    return handle_new_task(state);
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
                KeyCode::PageUp => {
                    detail.scroll_page_up(10);
                    return Ok(Control::Changed);
                }
                KeyCode::PageDown => {
                    detail.scroll_page_down(10);
                    return Ok(Control::Changed);
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
                KeyCode::Char('e') => {
                    return handle_editor_from_detail(state, ctx);
                }
                KeyCode::Esc | KeyCode::Char('h') | KeyCode::Left => {
                    return handle_back_from_detail(state);
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
            let task_list = TaskListState::from_cache(&ctx.cache, &id, &name, &ctx.config)?;
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
    run_editor_for_task(&task, ctx)?;
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
    run_editor_for_task(&task, ctx)?;
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

/// Suspend TUI, launch external editor, resume TUI.
fn run_editor_for_task(task: &crate::models::Task, ctx: &mut Global) -> crate::Result<()> {
    // Suspend TUI
    stdout().execute(LeaveAlternateScreen)?;
    disable_raw_mode()?;

    let result = crate::editor::edit_body(&ctx.config, &task.title, &task.body);

    // Resume TUI
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    // Force full repaint: ratatui's diff buffer is stale after the
    // editor used the terminal, so clear it to avoid a blank screen.
    ctx.clear_terminal();

    match result {
        Ok(crate::editor::EditorResult::Changed { title, body }) => {
            crate::mutations::update_body(&ctx.storage, &ctx.cache, &task.id, title, body)?;
        }
        Ok(crate::editor::EditorResult::Unchanged | crate::editor::EditorResult::Cancelled) => {}
        Err(_) => {
            // Editor errors are non-fatal in TUI context
        }
    }

    Ok(())
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
                MainView::Dashboard => return Ok(Control::Changed),
            };

            crate::mutations::create_task(
                &ctx.storage,
                &ctx.cache,
                &project_id,
                &title,
                "later",
                "M",
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
        Command::Unknown(_) => {
            // Silently ignore unknown commands
            Ok(Control::Changed)
        }
    }
}

// -- Create form handlers --

fn handle_new_task(state: &mut AppState) -> Result<Control<AppEvent>, crate::Error> {
    let MainView::TaskList(ref task_list) = state.main_view else {
        return Ok(Control::Continue);
    };
    let project_id = task_list.project_id.clone();
    let project_name = task_list.project_name.clone();
    state.create_form = Some(CreateFormState::new(project_id, project_name));
    Ok(Control::Changed)
}

fn handle_create_form_input(
    code: KeyCode,
    state: &mut AppState,
    ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    match code {
        KeyCode::Esc => {
            state.create_form = None;
            Ok(Control::Changed)
        }
        KeyCode::Enter => {
            let form = state.create_form.as_ref().unwrap();
            if !form.can_submit() {
                return Ok(Control::Continue);
            }
            let project_id = form.project_id.clone();
            let title = form.title().to_string();
            let priority = form.priority().to_string();
            let size = form.size().to_string();
            state.create_form = None;

            crate::mutations::create_task(
                &ctx.storage,
                &ctx.cache,
                &project_id,
                &title,
                &priority,
                &size,
            )?;

            if let MainView::TaskList(ref mut list) = state.main_view {
                list.refresh(&ctx.cache, &ctx.config);
            }
            trigger_background_sync(state, ctx);
            Ok(Control::Changed)
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
            }
            Ok(Control::Changed)
        }
        _ => Ok(Control::Continue),
    }
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
    }

    // If we're in detail view for the affected task, go back to list
    let affected_id = match &confirm.action {
        PendingAction::Done { task_id, .. } | PendingAction::Delete { task_id, .. } => {
            task_id.clone()
        }
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
        _ => {}
    }

    trigger_background_sync(state, ctx);
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

/// Spawn a background sync to push local mutations to the server.
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
        (MainView::TaskDetail { .. }, FocusPanel::Main) => {
            hints.extend([
                Span::styled(" Esc", theme.status_key),
                Span::styled(" back", theme.status_desc),
                Span::styled("  j/k", theme.status_key),
                Span::styled(" scroll", theme.status_desc),
                Span::styled("  s", theme.status_key),
                Span::styled(" start/stop", theme.status_desc),
                Span::styled("  p", theme.status_key),
                Span::styled(" priority", theme.status_desc),
                Span::styled("  d", theme.status_key),
                Span::styled(" done", theme.status_desc),
                Span::styled("  x", theme.status_key),
                Span::styled(" delete", theme.status_desc),
                Span::styled("  e", theme.status_key),
                Span::styled(" edit", theme.status_desc),
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
                Span::styled("  j/k", theme.status_key),
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
