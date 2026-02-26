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

//! Compound keybinding system with which-key style hint popup.
//!
//! A trie maps key sequences to actions. Single keys resolve immediately;
//! prefix keys enter a pending state and display available continuations.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Widget};

/// Actions the TUI can perform in response to key bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    GotoTasks,
    GotoDocuments,
    GotoProjects,
    Help,
}

/// A node in the keybinding trie.
#[derive(Debug, Clone)]
pub enum KeyTrie {
    /// Leaf: this key sequence maps to an action.
    Action(Action),
    /// Internal node: more keys are expected.
    Node(KeyTrieNode),
}

/// An internal trie node holding child bindings and display metadata.
#[derive(Debug, Clone)]
pub struct KeyTrieNode {
    pub name: &'static str,
    children: HashMap<KeyEvent, KeyTrie>,
    /// Insertion order for consistent popup rendering.
    order: Vec<KeyEvent>,
}

impl KeyTrieNode {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            children: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// Bind a single key to a trie entry.
    pub fn bind(mut self, key: KeyEvent, entry: KeyTrie) -> Self {
        if !self.children.contains_key(&key) {
            self.order.push(key);
        }
        self.children.insert(key, entry);
        self
    }

    fn get(&self, key: &KeyEvent) -> Option<&KeyTrie> {
        self.children.get(key)
    }

    /// Iterate children in insertion order, yielding (key, description).
    pub fn entries(&self) -> impl Iterator<Item = (&KeyEvent, &KeyTrie)> {
        self.order
            .iter()
            .filter_map(|k| self.children.get(k).map(|v| (k, v)))
    }
}

/// Result of processing a key event.
#[derive(Debug)]
pub enum KeymapResult {
    /// Key sequence matched an action.
    Matched(Action),
    /// Key is a prefix — more keys expected. The node holds continuations.
    Pending(KeyTrieNode),
    /// Key sequence doesn't match any binding.
    NotFound,
    /// Pending state was cancelled (Escape pressed).
    Cancelled,
}

/// Stateful keybinding processor.
pub struct Keymap {
    root: KeyTrieNode,
    /// When Some, we're mid-sequence waiting for the next key.
    pending: Option<KeyTrieNode>,
}

impl Keymap {
    pub fn new(root: KeyTrieNode) -> Self {
        Self {
            root,
            pending: None,
        }
    }

    /// Returns true if the keymap is waiting for more keys.
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Returns the pending node (for rendering the which-key popup).
    pub fn pending_node(&self) -> Option<&KeyTrieNode> {
        self.pending.as_ref()
    }

    /// Feed a key event and return the result.
    pub fn process(&mut self, key: KeyEvent) -> KeymapResult {
        // Escape always cancels pending state.
        if key.code == KeyCode::Esc {
            if self.pending.take().is_some() {
                return KeymapResult::Cancelled;
            }
            return KeymapResult::NotFound;
        }

        let node = self.pending.take().unwrap_or_else(|| self.root.clone());

        match node.get(&key) {
            Some(KeyTrie::Action(action)) => KeymapResult::Matched(*action),
            Some(KeyTrie::Node(child)) => {
                self.pending = Some(child.clone());
                KeymapResult::Pending(child.clone())
            }
            None => KeymapResult::NotFound,
        }
    }
}

/// Build the default keymap for the TUI.
pub fn default_keymap() -> Keymap {
    let goto = KeyTrieNode::new("goto")
        .bind(key('t'), KeyTrie::Action(Action::GotoTasks))
        .bind(key('d'), KeyTrie::Action(Action::GotoDocuments))
        .bind(key('p'), KeyTrie::Action(Action::GotoProjects));

    let root = KeyTrieNode::new("root")
        .bind(key('q'), KeyTrie::Action(Action::Quit))
        .bind(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            KeyTrie::Action(Action::Quit),
        )
        .bind(key('g'), KeyTrie::Node(goto))
        .bind(key('?'), KeyTrie::Action(Action::Help));

    Keymap::new(root)
}

/// Helper to create a plain key event with no modifiers.
fn key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
}

/// Format a key event for display in the which-key popup.
fn format_key(key: &KeyEvent) -> String {
    let mut s = String::new();
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        s.push_str("C-");
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        s.push_str("A-");
    }
    match key.code {
        KeyCode::Char(c) => s.push(c),
        KeyCode::Esc => s.push_str("Esc"),
        KeyCode::Enter => s.push_str("Enter"),
        KeyCode::Tab => s.push_str("Tab"),
        KeyCode::Backspace => s.push_str("BS"),
        _ => s.push('?'),
    }
    s
}

/// Label describing what a trie entry does.
fn entry_label(entry: &KeyTrie) -> &str {
    match entry {
        KeyTrie::Action(action) => match action {
            Action::Quit => "quit",
            Action::GotoTasks => "tasks",
            Action::GotoDocuments => "documents",
            Action::GotoProjects => "projects",
            Action::Help => "help",
        },
        KeyTrie::Node(node) => node.name,
    }
}

/// Render the which-key hint popup centred at the bottom of the given area.
pub fn render_which_key(node: &KeyTrieNode, area: Rect, buf: &mut Buffer) {
    let hints: Vec<(String, &str)> = node
        .entries()
        .map(|(k, v)| (format_key(k), entry_label(v)))
        .collect();

    if hints.is_empty() {
        return;
    }

    let lines: Vec<Line<'_>> = hints
        .iter()
        .map(|(k, desc)| {
            Line::from_iter([
                Span::from(format!("  {k}")).cyan().bold(),
                Span::from(format!("  {desc}")),
            ])
        })
        .collect();

    let height = (lines.len() as u16) + 2; // +2 for block border
    let width = lines.iter().map(|l| l.width() as u16).max().unwrap_or(10) + 4; // +4 for padding

    // Position popup at the bottom-centre of the area.
    let popup_area = popup_rect(area, width, height);

    Clear.render(popup_area, buf);

    let block = Block::bordered()
        .title(format!(" {} ", node.name))
        .padding(Padding::horizontal(1));
    let inner = block.inner(popup_area);
    block.render(popup_area, buf);

    for (i, line) in lines.into_iter().enumerate() {
        if i as u16 >= inner.height {
            break;
        }
        let row = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        line.render(row, buf);
    }
}

/// Compute a centred popup rect at the bottom of the area.
fn popup_rect(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);

    let vertical = Layout::vertical([Constraint::Fill(1), Constraint::Length(h)]).split(area);
    let horizontal = Layout::horizontal([Constraint::Length(w)])
        .flex(Flex::Center)
        .split(vertical[1]);
    horizontal[0]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_key_resolves_immediately() {
        let mut km = default_keymap();
        match km.process(key('q')) {
            KeymapResult::Matched(Action::Quit) => {}
            other => panic!("expected Matched(Quit), got {other:?}"),
        }
        assert!(!km.is_pending());
    }

    #[test]
    fn chord_enters_pending_then_resolves() {
        let mut km = default_keymap();
        match km.process(key('g')) {
            KeymapResult::Pending(node) => assert_eq!(node.name, "goto"),
            other => panic!("expected Pending, got {other:?}"),
        }
        assert!(km.is_pending());

        match km.process(key('d')) {
            KeymapResult::Matched(Action::GotoDocuments) => {}
            other => panic!("expected Matched(GotoDocuments), got {other:?}"),
        }
        assert!(!km.is_pending());
    }

    #[test]
    fn escape_cancels_pending() {
        let mut km = default_keymap();
        km.process(key('g'));
        assert!(km.is_pending());

        match km.process(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)) {
            KeymapResult::Cancelled => {}
            other => panic!("expected Cancelled, got {other:?}"),
        }
        assert!(!km.is_pending());
    }

    #[test]
    fn unknown_key_returns_not_found() {
        let mut km = default_keymap();
        match km.process(key('z')) {
            KeymapResult::NotFound => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
    }

    #[test]
    fn wrong_continuation_returns_not_found() {
        let mut km = default_keymap();
        km.process(key('g'));
        match km.process(key('z')) {
            KeymapResult::NotFound => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
        assert!(!km.is_pending());
    }
}
