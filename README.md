# Getting Things Rusty CLI

Command-line client for Getting Things Rusty - an ADHD-friendly task tracker
with offline-first CRDT synchronization.

## Status

**Alpha** - Full offline-first operation with CRDT synchronization is
implemented. All commands work locally-first with automatic background sync to
the server.

## Installation

```bash
cargo install --path .
```

## Quick Start

### 1. Initialize Configuration

```bash
gtr init --server http://localhost:3000 --token your-auth-token
```

This creates `~/.config/gtr/config.toml` with your server URL and
authentication token. The CLI will automatically create a local cache directory
at `~/.local/share/gtr/` for offline storage.

### 2. List Tasks

```bash
# List all tasks (requires project ID)
gtr list --project my-project

# List with filters
gtr list --project my-project --priority for-now --size M

# List all projects
gtr list --projects
```

### 3. Create a Task

```bash
gtr create --project my-project "My task title" \
  --body "Task description" \
  --priority for-now \
  --size M \
  --impact 1
```

### 4. Show a Task

```bash
gtr show <task-id>
```

Shows task with pretty markdown rendering.

### 5. Update a Task

```bash
# Update specific fields
gtr update <task-id> --title "New title" --priority for-now

# Update impact level
gtr update <task-id> --impact 2

# Edit body in your $EDITOR
gtr update <task-id> --body
```

#### Editing Task Body

When using `--body`, the editor opens with the task title as a markdown H1
header:

```markdown
# Your Task Title

Your task body content here...
```

**Title Editing in Body:**

- **Change title:** Edit the `# Title` line to update the task title
- **Remove title:** Delete the `# Title` line to keep the original title
  unchanged
- **Precedence:** The `--title` flag always takes precedence over title changes
  in the editor

**Example:**

```bash
# Edit both title and body in one command
gtr update abc123 --body
# (Edit the "# Title" line in editor to change both)

# Override with explicit --title flag
gtr update abc123 --body --title "Explicit title"
# (Editor changes to title are ignored, flag wins)
```

**Editor Configuration:**

The editor is resolved in this order:

1. `gtr config editor --set "nvim -c 'set ft=markdown'"` (config file)
2. `$EDITOR` environment variable
3. `$VISUAL` environment variable
4. Default: `vi`

**Text Wrapping in Neovim:**

For automatic hard-wrapping at 80 columns in nvim, set:

```bash
gtr config editor --set "nvim -c 'set ft=markdown tw=79 fo+=t'"
```

This configures:

- `ft=markdown` - Enable markdown syntax highlighting
- `tw=79` - Set text width to 79 characters (79 + newline = 80 columns)
- `fo+=t` - Auto-wrap text using textwidth

Or add to your `~/.config/nvim/ftplugin/markdown.vim`:

```vim
setlocal textwidth=79
setlocal formatoptions+=t
```

#### Setting Deadlines

Deadlines can be set when creating or updating tasks using the `--deadline` (or
`-d`) flag. Both strict formats and natural language are supported.

**ISO 8601 / RFC3339 (strict formats):**

```bash
# Full RFC3339 with timezone
gtr update <task-id> -d "2026-02-15T08:00:00Z"
gtr update <task-id> -d "2026-02-15T08:00:00-05:00"

# Date and time (assumes UTC)
gtr update <task-id> -d "2026-02-15 08:00:00"

# Date only (assumes midnight UTC)
gtr update <task-id> -d "2026-02-15"

# Clear deadline
gtr update <task-id> -d "none"
```

**Natural Language (keywords and weekdays):**

```bash
# Keywords
gtr update <task-id> -d "tomorrow"
gtr update <task-id> -d "today"
gtr update <task-id> -d "yesterday"

# Weekdays
gtr update <task-id> -d "next friday"
gtr update <task-id> -d "last monday"
gtr update <task-id> -d "friday"          # Next occurring Friday

# With time of day
gtr update <task-id> -d "tomorrow 8am"
gtr update <task-id> -d "next fri 6pm"
gtr update <task-id> -d "18:30"           # Today at 18:30
gtr update <task-id> -d "8.45pm"          # Today at 20:45

# Month names
gtr update <task-id> -d "next april"      # Next April 1st
gtr update <task-id> -d "1 April 2026"
gtr update <task-id> -d "April 1, 2026"
```

**Duration Expressions (relative to now):**

```bash
# Simple durations
gtr update <task-id> -d "3 days"
gtr update <task-id> -d "2 weeks"
gtr update <task-id> -d "5 hours"
gtr update <task-id> -d "30 minutes"

# Decimal durations (hours/minutes/seconds only, not days/weeks)
gtr update <task-id> -d "2.5 hours"       # 2 hours 30 minutes
gtr update <task-id> -d "1.5h"            # 1 hour 30 minutes

# Chained units
gtr update <task-id> -d "1 week 2 days"
gtr update <task-id> -d "2 days 3 hours"
gtr update <task-id> -d "1 hour 30 minutes"

# Past dates with "ago"
gtr update <task-id> -d "2 days ago"
gtr update <task-id> -d "3 hours ago"
gtr update <task-id> -d "1 week 2 days ago"

# Compact notation
gtr update <task-id> -d "3d"              # 3 days
gtr update <task-id> -d "2h30m"           # 2 hours 30 minutes
gtr update <task-id> -d "1w4d"            # 1 week 4 days
```

**Examples:**

```bash
# Set deadline when creating a task (natural language)
gtr new --project my-project "Important task" -d "tomorrow 8am"

# Update existing task with weekday deadline
gtr update abc123 -d "next friday"

# Set deadline 2 weeks from now
gtr update abc123 -d "2 weeks"

# Set deadline with precise time
gtr update abc123 -d "2026-03-01 09:00:00"

# Remove deadline from task
gtr update abc123 -d "none"
```

**What's NOT supported:**

```bash
# ❌ These will NOT work:
gtr update abc123 -d "the day after tomorrow"  # Too complex
gtr update abc123 -d "christmas"               # No holiday names
gtr update abc123 -d "Q1 2026"                 # No quarters
gtr update abc123 -d "02/15/2026"              # Use YYYY-MM-DD instead
```

#### Impact Levels

Tasks carry an impact level (1-5) that affects how urgently they get promoted
from "later" to "now" as their deadline approaches.

| Level | Label        | Multiplier | Effect                          |
| ----- | ------------ | ---------- | ------------------------------- |
| 1     | Catastrophic | 2.0x       | Promotes with 2x lead time      |
| 2     | Significant  | 1.5x       | Promotes with 1.5x lead time    |
| 3     | Neutral      | 1.0x       | Default behavior                |
| 4     | Minor        | 0.5x       | Promotes with half lead time    |
| 5     | Negligible   | 0.25x      | Promotes with quarter lead time |

```bash
# Set impact when creating
gtr new "Critical bug" -d "3 days" --impact 1

# Update impact
gtr update <task-id> -i 5
```

In `gtr list`, high-impact tasks show emoji indicators in the priority column:

- Impact 1: 🔥 (fire)
- Impact 2: ⚡ (lightning)
- Impact 3-5: no indicator

In `gtr list`, high-impact tasks are sorted by priority, then impact, then
deadline.

#### Joy

Tasks carry a joy score (0–10, default 5) that captures how much enjoyment or
energy a task brings — designed for ADHD brains that thrive on dopamine.

```bash
# Set joy when creating
gtr new "Fun refactor" --joy 9

# Update joy
gtr update <task-id> --joy 2
```

In `gtr list` and `gtr next`, joy is shown with emoji indicators:

- Joy 8–10: 🌟 (high energy / fun)
- Joy 0–4: 💤 (low energy / draining)
- Joy 5–7: no indicator (neutral)

#### `gtr next` — ADHD-Friendly Urgency Scoring

`gtr next` suggests the best task to work on right now. Instead of a rigid
"sort by deadline, then impact, then everything else" approach, it uses a
**composite urgency score** that blends all factors into a single number.
This is designed around how ADHD brains actually work:

**Priority is the only hard boundary.** "Now" tasks always rank above "later"
tasks. Within the same priority, everything else is blended:

1. **Overdue decay** — Being overdue doesn't make a task infinitely urgent.
   If you haven't done a task in 2 days, the deadline was clearly soft.
   Overdue urgency uses logarithmic diminishing returns: the first hour
   overdue matters a lot, but 48h overdue is barely more urgent than 24h.

2. **Impact scales time** — High-impact tasks perceive deadlines as closer.
   A catastrophic (1) task due in 24h is treated as if it's due in 12h. A
   negligible (5) task due in 24h feels like 48h away. This also applies to
   overdue tasks: a catastrophic overdue task stays urgent, while a neutral
   overdue task loses its edge.

3. **Joy × impact bonus** — Joy alone gives only a small nudge (prevents
   "productive procrastination" toward fun-but-unimportant tasks). But joy
   combined with high impact gives a substantial boost — up to ±30 hours
   for extreme values. This surfaces the "golden tasks": important work
   your brain actually _wants_ to do. **For overdue tasks, the joy effect
   is heavily attenuated (90% reduction)** — when you've already missed a
   deadline, "do I feel like it?" matters far less than impact. A boring
   but catastrophic overdue task will still rank above a neutral one.

4. **Size bonus for quick wins** — Small tasks (XS, S) get a nudge up.
   Large tasks (L, XL) nudge down. ADHD brains build momentum from quick
   wins; starting with something achievable lowers activation energy.
   **For overdue tasks, the size effect is also attenuated (90% reduction)**
   — a small task's quick-win bonus shouldn't let neutral overdue tasks
   outrank catastrophic ones just because they're smaller.

5. **Work state** — Stopped tasks (already have context loaded from a
   previous session) get a small nudge, reducing context-switching cost.

**Example scenario:**

| Task         | Impact      | Joy | Size | Deadline       |
| ------------ | ----------- | --- | ---- | -------------- |
| Boring bug A | Neutral (3) | 5   | S    | 2 days overdue |
| Big feature  | Sig. (2)    | 5   | L    | 15h away       |
| Fun fix      | Catas. (1)  | 8   | S    | 1 day away     |

A pure deadline sort puts "Boring bug A" first (most overdue). The blended
score promotes "Fun fix" — it's small, joyful, catastrophic-impact, and
approaching its deadline. That's the ideal ADHD task: important work with
low activation energy that your brain is excited to start.

**Configuring impact labels and multipliers:**

```bash
# View current impact configuration
gtr config promotion show

# Edit impact labels and multipliers (opens editor with JSON)
gtr config promotion set

# Reset all overrides to defaults
gtr config promotion reset
```

### 6. Delete a Task

```bash
gtr delete <task-id>
```

### 7. Search Tasks

```bash
gtr search "search query" --project my-project --limit 10
```

## Offline Mode

The CLI operates **offline-first** by default. All operations work locally
immediately, with automatic background synchronization to the server.

### How It Works

1. **Local Storage**: Tasks are stored as CRDT `.automerge` files in
   `~/.local/share/gtr/default/<project-id>/`
2. **SQLite Cache**: Fast queries using a local SQLite index at
   `~/.local/share/gtr/index.db`
3. **Automatic Sync**: Commands attempt to sync with the server (with timeout)
4. **Graceful Degradation**: If sync fails, operations complete locally and are
   queued for later sync

### Sync Status Indicators

After each command, you'll see one of:

- `✓ synced` - Successfully synced with server
- `⊙ queued for sync` - Saved locally, will sync when server is available
- `✗ sync failed` - Local operation succeeded, but sync failed (check
  connectivity)

### Disabling Sync

For fully offline operation, use the `--no-sync` flag:

```bash
# Create a task without attempting to sync
gtr create --project my-project "Offline task" --no-sync

# Update without sync
gtr update <task-id> --title "New title" --no-sync
```

### Manual Synchronization

```bash
# Sync all pending changes
gtr sync now

# Check sync status
gtr sync status
```

### Working Offline

The CLI fully supports offline work:

- **Create, update, delete** tasks while offline
- **Search and list** using local cache
- **CRDT-based conflict resolution** when syncing with server
- **Automatic merge** of concurrent edits from multiple devices

## Daily Feels

ADHD brains have variable capacity day-to-day. The feels system lets you
self-report **energy** (1-5) and **focus** (1-5) so `gtr next` adapts its
suggestions to what you can realistically tackle right now.

### Setting Feels

```bash
# Set energy=2, focus=4
gtr feels 2 4

# Show current day's state
gtr feels-show
```

### Scale

| Level | Energy                       | Focus                     |
| ----- | ---------------------------- | ------------------------- |
| 1     | Very low — need easy wins    | Scattered — small tasks   |
| 2     | Low — prefer enjoyable tasks | Limited — small/medium    |
| 3     | Moderate                     | Moderate                  |
| 4     | Good — can handle tedium     | Good — can tackle large   |
| 5     | High — bring on anything     | Deep — ready for anything |

### How It Affects Scoring

- **Low energy** → joyful tasks bubble up, boring tasks sink
- **Low focus** → small tasks bubble up, large tasks sink
- **High energy/focus** → these factors matter less, other signals dominate
- **Overdue tasks** → feels don't matter, impact/urgency dominates

### Prompt Behavior

- `gtr next` prompts for feels if not set today (set / defer 1h / skip)
- After 4 hours, re-prompts if values were previously set (keep / update / skip)
- No prompts on `list`, `show`, or `search`

## Markdown Rendering

Task descriptions are rendered with formatted markdown for better readability:

```bash
# View task with markdown formatting (default)
gtr show <task-id>

# Disable formatting for plain text
gtr show <task-id> --no-format
```

### Features

- **Bold** text highlighted in bright white
- _Italic_ text in cyan
- Headers in yellow
- `inline code` with dark background
- Code blocks with syntax-appropriate styling
- Bullet lists with green markers

### Environment Control

Markdown rendering respects terminal capabilities:

- **NO_COLOR** environment variable disables all formatting
- Automatic TTY detection (plain text when piped to other commands)
- `--no-format` flag for explicit plain text output

## Configuration

Config file location: `~/.config/gtr/config.toml`

```toml
server_url = "http://localhost:3000"
auth_token = "your-auth-token"
```

### Environment Variables

- `GTR_CONFIG`: Override config file path
- `GTR_SERVER_URL`: Override server URL
- `GTR_AUTH_TOKEN`: Override auth token

## Features

### Implemented ✓

- [x] **Offline-first operation** - All commands work locally with automatic
      sync
- [x] **CRDT synchronization** - Conflict-free sync using Automerge
- [x] **Local CRDT storage** - Tasks stored as `.automerge` files
- [x] **SQLite cache** - Fast local queries and full-text search
- [x] **HTTP client** - REST API integration with timeout handling
- [x] **Full CRUD commands** - create, show, update, delete, list, search
- [x] **State commands** - done, undone, restore
- [x] **Markdown rendering** - Formatted task descriptions with NO_COLOR support
- [x] **Log viewing** - View task change history
- [x] **Project management** - List and filter by project
- [x] **$EDITOR integration** - Edit task body in your preferred editor
- [x] **Pretty table output** - Color-coded task lists
- [x] **Sync commands** - Manual sync (`sync now`, `sync status`)
- [x] **Offline mode flag** - `--no-sync` for fully offline operation
- [x] **Impact levels** - Configurable urgency scaling for deadline promotion
- [x] **Joy scoring** - ADHD-friendly nudge toward enjoyable tasks

### Planned

- [ ] Interactive prompts for missing fields
- [ ] Shell completions (bash, zsh, fish)
- [ ] Config subcommands (view, edit, validate)
- [ ] Advanced filtering (by date range, custom fields)

## Development

```bash
# Build
cargo build

# Run
cargo run -- list --projects

# Test
cargo test

# Format
cargo fmt

# Lint
cargo clippy -- -D warnings
```

## License

This project is licensed under the GNU Affero General Public License v3.0 - see
the [LICENSE](LICENSE) file for details.
