# TUI (Terminal User Interface) Proposal for GTR CLI

## Executive Summary

Add an interactive TUI mode to complement the existing CLI commands, focusing
on task list browsing, quick operations (add/edit/delete), and real-time
updates. Uses industry-standard [ratatui](https://ratatui.rs/) framework with
vim-like keybindings.

---

## Recommended Technology Stack

### Core Dependencies

| Crate | Version | Purpose | Why This Choice |
|-------|---------|---------|-----------------|
| **ratatui** | ^0.30 | TUI framework | Industry standard, actively maintained, zero-cost abstractions |
| **crossterm** | ^0.28 | Terminal backend | Cross-platform, default for ratatui, excellent event handling |
| **tokio** | ^1.0 | Async runtime | Already in use, integrates well with existing async code |
| **tui-textarea** | ^0.7 | Multi-line text editing | Pre-built widget for editing task body/description |

### Optional Enhancement Crates

- **tui-input** - Single-line input fields (for title, filters)
- **fuzzy-matcher** - Fuzzy search filtering
- **notify** - File system watching (for real-time updates)

---

## UI Design & Layout

### Main View: Task List Browser

```
┌─ Getting Things Rusty ──────────────────────────────── [Offline ⊙] ─┐
│ Project: work (12 tasks) │ Filters: [for-now] [!deleted]            │
├──────────────────────────┴───────────────────────────────────────────┤
│ ■  abc123  Fix login bug             for-now   M   2h ago           │
│ ▶  def456  Add metrics endpoint      for-now   S   1d ago           │
│    ghi789  Refactor auth module      not-now   L   3d ago           │
│    jkl012  Update dependencies       for-now   XS  1w ago           │
│                                                                       │
│ [12 more tasks...]                                                   │
├───────────────────────────────────────────────────────────────────────┤
│ abc123 • Fix login bug                                               │
│ Priority: for-now │ Size: M │ Modified: 2h ago                       │
│                                                                       │
│ Users can't login when password contains special chars. Need to      │
│ properly escape SQL queries and validate input sanitization.         │
│                                                                       │
├───────────────────────────────────────────────────────────────────────┤
│ j/k: nav │ Enter: edit │ n: new │ d: done │ x: delete │ q: quit     │
└───────────────────────────────────────────────────────────────────────┘
```

### Layout Components

**Top Bar** (1 line)
- App title
- Current project name + task count
- Sync status indicator (✓ synced / ⊙ pending / ✗ offline)

**Filter Bar** (1 line)
- Active filters shown as badges
- Quick filter toggles

**Task List** (scrollable, ~60% of screen)
- Columns: Status icon, ID (short), Title, Priority, Size, Last modified
- Color coding: for-now (yellow), not-for-now (blue), done (green), deleted
  (dim)
- Selection highlight with cursor
- Vim-like j/k navigation

**Task Detail Pane** (~30% of screen)
- Full task ID
- Title, priority, size, timestamps
- Body/description preview
- Scrollable when content is long

**Status Line** (1 line)
- Keybinding hints
- Context-aware (changes based on mode)

---

## Navigation & Keybindings

### Vim-Like Navigation (List Mode)

| Key | Action | Notes |
|-----|--------|-------|
| `j` / `↓` | Move down | |
| `k` / `↑` | Move up | |
| `g` / `Home` | Jump to top | |
| `G` / `End` | Jump to bottom | |
| `Ctrl-d` | Scroll half-page down | |
| `Ctrl-u` | Scroll half-page up | |
| `/` | Start search/filter | Opens mini-prompt |
| `n` | Next search result | |
| `N` | Previous search result | |

### Task Operations

| Key | Action | Notes |
|-----|--------|-------|
| `Enter` | Edit task | Opens editor modal |
| `n` | New task | Opens creation form |
| `d` | Mark done | Toggle done/undone |
| `x` | Delete task | Requires confirmation |
| `r` | Restore deleted | Only visible when viewing deleted |
| `y` | Copy task ID | To clipboard (if supported) |
| `p` | Toggle priority | Cycles for-now ↔ not-for-now |
| `s` | Change size | Opens size selector |

### View Controls

| Key | Action | Notes |
|-----|--------|-------|
| `Tab` | Next project | Cycles through projects |
| `Shift-Tab` | Previous project | |
| `f` | Toggle filter menu | Show/hide deleted, by priority, etc. |
| `S` | Sync now | Force sync with server |
| `?` | Help modal | Show all keybindings |
| `q` | Quit TUI | Return to shell |
| `Ctrl-c` | Force quit | Emergency exit |

---

## Feature Scope (MVP)

### Phase 1: Read-Only Browser ✓

- [x] Display task list (using local cache)
- [x] Navigate with j/k
- [x] Show task details in preview pane
- [x] Filter by project
- [x] Search/filter by text
- [x] Sync status indicator

### Phase 2: Basic Operations ✓

- [x] Mark task as done (`d`)
- [x] Quick priority toggle (`p`)
- [x] Size change (`s`)
- [x] Delete task (`x` with confirmation)

### Phase 3: Task Creation & Editing ✓

- [x] New task form (`n`)
  - Title input (single-line)
  - Body editor (multi-line, using tui-textarea)
  - Priority selector
  - Size selector
  - Deadline picker (optional)
- [x] Edit existing task (`Enter`)
  - Same form as creation
  - Pre-populated with current values

### Phase 4: Real-Time Updates (Future)

- [ ] Watch local cache for changes
- [ ] Auto-refresh on sync completion
- [ ] Show notifications for updates

---

## Architecture Integration

### Command Structure

```
gtr
├── list              # Current: table output
├── show <id>         # Current: single task view
├── create ...        # Current: CLI args
├── update ...        # Current: CLI args
├── tui               # NEW: Launch interactive TUI
│   ├── --project <id>   # Optional: start in specific project
│   └── --filter <...>   # Optional: apply initial filters
└── ...
```

### Code Organization

```
src/
├── tui/
│   ├── mod.rs           # TUI entry point, event loop
│   ├── app.rs           # Application state management
│   ├── ui/
│   │   ├── mod.rs       # UI rendering logic
│   │   ├── task_list.rs # Task list widget
│   │   ├── detail.rs    # Detail pane widget
│   │   ├── editor.rs    # Task editor modal
│   │   └── filters.rs   # Filter selection UI
│   ├── events.rs        # Event handling (keyboard, tick, etc.)
│   ├── actions.rs       # User actions → state changes
│   └── keybindings.rs   # Key mapping configuration
├── commands/
│   ├── tui.rs           # NEW: TUI command handler
│   └── ...              # Existing commands
└── ...
```

### State Management Pattern

```rust
// App state (persists between frames)
struct App {
    // Data
    tasks: Vec<Task>,
    projects: Vec<Project>,

    // UI State
    selected_task_index: usize,
    scroll_offset: usize,
    current_project: String,
    active_filters: Filters,

    // Mode
    mode: AppMode,  // Normal, Editing, Searching, ConfirmDelete

    // Integration
    local_ctx: LocalContext,  // Reuse existing local storage
    sync_manager: SyncManager,
}

enum AppMode {
    Normal,
    Editing(TaskEditor),
    Searching(String),
    ConfirmDelete(TaskId),
    Help,
}
```

### Event Loop (Async with Tokio)

```rust
async fn run_tui(config: Config) -> Result<()> {
    let mut terminal = setup_terminal()?;
    let mut app = App::new(config).await?;

    // Event channels
    let (action_tx, mut action_rx) = mpsc::channel(100);
    let (event_tx, mut event_rx) = mpsc::channel(100);

    // Spawn event handler task
    tokio::spawn(async move {
        loop {
            if let Ok(event) = crossterm::event::read() {
                event_tx.send(event).await.ok();
            }
        }
    });

    // Main loop
    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        tokio::select! {
            Some(event) = event_rx.recv() => {
                if let Some(action) = app.handle_event(event) {
                    action_tx.send(action).await?;
                }
            }
            Some(action) = action_rx.recv() => {
                app.handle_action(action).await?;
                if matches!(action, Action::Quit) {
                    break;
                }
            }
        }
    }

    restore_terminal()?;
    Ok(())
}
```

---

## Implementation Approach

### Phased Development

**Phase 1: Foundation (Week 1)**
- Add ratatui + crossterm dependencies
- Create basic TUI skeleton with event loop
- Implement read-only task list display
- Basic navigation (j/k/q)

**Phase 2: Interactivity (Week 2)**
- Add task detail pane
- Implement quick operations (done, priority, size)
- Delete with confirmation
- Sync integration

**Phase 3: Editing (Week 3)**
- Add tui-textarea for multi-line editing
- Implement task creation form
- Implement task editing
- Input validation

**Phase 4: Polish (Week 4)**
- Add search/filter UI
- Implement help modal
- Color themes
- Error handling & notifications
- Performance optimization

### Testing Strategy

- Unit tests for state management
- Integration tests for actions
- Manual testing on different terminal emulators
- Test offline mode scenarios

---

## UX Inspirations

Projects to learn from:

- **gitui** - Excellent vim-like navigation, clear status indicators
- **taskwarrior-tui** - Good task management UX patterns
- **kabmat** - Clean kanban board interface with vim keybindings
- **lazygit** - Intuitive modal workflows

---

## Technical Considerations

### Offline-First Compatibility

✓ Reads from local cache (same as current CLI commands)
✓ Uses existing LocalContext for storage
✓ Sync happens in background, non-blocking
✓ Shows sync status visually

### Performance

- Ratatui uses zero-cost abstractions (no runtime overhead)
- Incremental rendering (only draws changes)
- Lazy loading for large task lists (pagination)
- Async operations don't block UI updates

### Terminal Compatibility

- Works on: Linux, macOS, Windows (via crossterm)
- Tested terminals: iTerm2, Alacritty, Kitty, GNOME Terminal, Windows
  Terminal
- Fallback rendering for limited terminals

---

## Open Questions

1. **Editor Integration**: Should `e` key open $EDITOR or use built-in
   tui-textarea?
   - Recommendation: Built-in for consistency, but allow `E` for external
     editor

2. **Multi-Project View**: Show all projects at once or switch between them?
   - Recommendation: Tab/Shift-Tab to cycle, `P` for project picker modal

3. **Theming**: Hardcode colors or allow customization?
   - Recommendation: Start with hardcoded theme, add config later

4. **Help System**: Modal or separate screen?
   - Recommendation: Modal overlay (`?` key) that doesn't lose context

---

## References

- [Ratatui Official Documentation](https://ratatui.rs/)
- [Ratatui Async Tutorial](https://ratatui.rs/tutorials/counter-async-app/)
- [taskwarrior-tui Source](https://github.com/kdheepak/taskwarrior-tui)
- [gitui Source](https://github.com/gitui-org/gitui)
- [Ratatui Best Practices Discussion](https://github.com/ratatui/ratatui/discussions/220)
- [Interactive Widgets Guide](https://ratatui.rs/concepts/widgets/)
