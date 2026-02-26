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

use crossterm::event::{Event, KeyEventKind};
use rat_salsa::poll::PollCrossterm;
use rat_salsa::{Control, RunConfig, SalsaAppContext, SalsaContext, run_tui};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Widget};

use crate::cache::TaskCache;
use crate::config::Config;
use crate::tui::keymap::{self, Action, Keymap, KeymapResult};
use crate::tui::sidebar::SidebarState;
use crate::tui::theme::Theme;

/// Application-wide state visible to all callbacks.
#[allow(dead_code)]
pub struct Global {
    ctx: SalsaAppContext<AppEvent, crate::Error>,
    pub config: Config,
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

/// UI widget state tree.
pub struct AppState {
    pub keymap: Keymap,
    pub theme: Theme,
    pub sidebar: SidebarState,
}

/// Enter the TUI event loop. Returns when the user quits.
pub fn run(config: Config) -> crate::Result<()> {
    let cache = TaskCache::open(&config.cache_dir.join("index.db"))?;
    let sidebar = SidebarState::from_cache(&cache)?;

    let mut global = Global {
        ctx: SalsaAppContext::default(),
        config,
    };
    let mut state = AppState {
        keymap: keymap::default_keymap(),
        theme: Theme::default(),
        sidebar,
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

    // Sidebar
    state.sidebar.render(theme, true, columns[0], buf);

    // Main area — placeholder
    let greeting = Paragraph::new(vec![
        Line::default(),
        Line::from_iter([
            Span::from("  gtr").style(theme.accent.add_modifier(Modifier::BOLD)),
            Span::from(" — Getting Things Rusty"),
        ]),
        Line::default(),
        Span::from("  TUI is loading...").style(theme.muted).into(),
    ])
    .block(Block::bordered().title(" gtr "));
    greeting.render(columns[1], buf);

    // Status bar
    Line::from_iter([
        Span::from(" q").style(theme.status_key),
        Span::from(" quit").style(theme.status_desc),
        Span::from("  g").style(theme.status_key),
        Span::from(" goto").style(theme.status_desc),
        Span::from("  ?").style(theme.status_key),
        Span::from(" help").style(theme.status_desc),
    ])
    .render(layout[1], buf);

    // Which-key popup when a prefix key is pending
    if let Some(node) = state.keymap.pending_node() {
        keymap::render_which_key(node, theme, area, buf);
    }

    Ok(())
}

fn handle_event(
    event: &AppEvent,
    state: &mut AppState,
    _ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    match event {
        AppEvent::Event(Event::Key(key)) if key.kind == KeyEventKind::Press => {
            match state.keymap.process(*key) {
                KeymapResult::Matched(Action::Quit) => Ok(Control::Quit),
                KeymapResult::Matched(_action) => {
                    // Other actions will be handled in later commits.
                    Ok(Control::Changed)
                }
                KeymapResult::Pending(_) => Ok(Control::Changed),
                KeymapResult::Cancelled | KeymapResult::NotFound => Ok(Control::Changed),
            }
        }
        _ => Ok(Control::Continue),
    }
}

fn handle_error(
    err: crate::Error,
    _state: &mut AppState,
    _ctx: &mut Global,
) -> Result<Control<AppEvent>, crate::Error> {
    Err(err)
}
