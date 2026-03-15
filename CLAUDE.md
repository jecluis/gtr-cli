# GTR CLI — Command-Line Client

Command-line client for Getting Things Rusty. Offline-first
architecture with local CRDT storage, SQLite cache, and background
sync to the server.

## Essential Commands

```bash
cargo build
cargo test
cargo clippy -- -D warnings
cargo fmt -- --check
npx prettier --write '**/*.md'       # Format markdown (80-col wrap)
```

## Architecture

### Offline-First Design

Every command operates locally first, then attempts sync:

1. Write/read local CRDT storage (always succeeds)
2. Attempt sync with timeout (3s writes, 2s reads)
3. Display result with status: `✓` synced, `⊙` queued, `✗` failed
4. `--no-sync` flag skips sync attempt

### Storage Layers

```
~/.cache/gtr/          (or configured cache_dir)
  tasks/
    <uuid>.automerge   # Local CRDT (flat layout)
  documents/
    <uuid>.automerge   # Local CRDT
  index.db             # SQLite cache (fast queries, sync tracking)
```

- **CRDT Storage** (`storage/`) — File I/O for `.automerge` files.
  Create, load, update, merge operations. Atomic writes.
- **SQLite Cache** (`cache.rs`) — Denormalized task/project/document
  metadata for fast queries. Tracks `needs_push` flag for modified
  entities and `last_synced` timestamps.
- **Sync Manager** (`sync.rs`) — Push/pull with server. Sends raw
  CRDT bytes, receives merged result. Timeout-aware, non-blocking.
- **LocalContext** (`local.rs`) — Coordinator wrapping storage + cache
  + sync. Entry point for all local-first operations.

### Module Structure

- `main.rs` — Clap command parser, global CLI structure
- `commands/` — All CLI commands (~36 commands across task, project,
  document, namespace, sync, config groups)
- `client.rs` — HTTP client (reqwest). Percent-encodes URLs. All
  server API calls.
- `config.rs` — TOML config (`~/.config/gtr/config.toml`). Server
  URL, auth token, cache dir, client ID.
- `models.rs` — Domain types matching server API (Task, Project,
  Document, Namespace)
- `crdt/` — Automerge 0.7.3 wrappers (TaskDocument, PkmsDocument)
- `storage/` — File I/O for `.automerge` files, storage config,
  migration
- `cache.rs` — SQLite cache (tasks, projects, documents, namespaces,
  feels, references)
- `local.rs` — LocalContext coordinator (storage + cache + sync)
- `sync.rs` — SyncManager (push/pull, timeout handling)
- `mutations.rs` — State machine helpers for local mutations
  (mark_done, update fields, etc.)
- `display.rs` — Pure data rendering (urgency, deadlines, progress
  bars, label colors). No terminal dependencies.
- `output/` — Pretty printing (DetailView, tables, label rendering,
  ID formatting)
- `tui/` — Optional ratatui-based TUI (behind `tui` feature flag)
- `icons.rs` — Glyph rendering for unicode/nerd font themes
- `labels.rs` — Label utilities and filtering
- `hierarchy.rs` — Parent-child relationships, cycle detection, depth
  tracking
- `urgency.rs` — Urgency scoring (deadline, impact, joy, energy,
  focus)
- `promotion.rs` — Threshold-based priority promotion
- `resolve.rs` — Name/path resolution (project names → UUIDs)
- `utils.rs` — Task/project ID resolution, picker dialogs, date
  validation
- `editor.rs` — External editor integration (vim, nano, etc.)
- `markdown.rs` — Markdown rendering (termimad-based)
- `slug.rs` — Slug generation for documents
- `references/` — Reference types (task ↔ document linking)
- `url_fetch/` — HTML scraping for `--from URL` / `--bookmark URL`
- `logging.rs` — Tracing initialization
- `error.rs` — Custom error types (UserFacing, Config, Database, etc.)
- `threshold_cache.rs` — In-memory cache of promotion thresholds

### Command Modification Pattern

All mutation commands follow this structure:

```rust
// 1. Resolve IDs (server/cache lookup)
let full_id = utils::resolve_task_id(&client, task_id).await?;

// 2. Create LocalContext (sync enabled unless --no-sync)
let ctx = LocalContext::new(config, !no_sync)?;

// 3. Ensure entity is locally available (fetches from server if needed)
ctx.load_task(&client, &full_id).await?;

// 4. Mutate local storage + cache
mutations::mark_done(&ctx.storage, &ctx.cache, &full_id)?;

// 5. Attempt sync (best-effort, timeout-aware)
let synced = ctx.try_sync().await;
// Display: ✓ synced / ⊙ queued / ✗ failed
```

## Important References

- **[UI Style Conventions](docs/ui-style.md)** — Display style rules
  (e.g., never use `theme.muted` for hint text)
- **[Automerge API](../get-things-rusty/docs/automerge-api.md)** —
  Real Automerge 0.7.3 API (design doc pseudocode is inaccurate)
- **[CRDT Testing](../get-things-rusty/docs/crdt-testing.md)** — Merge
  test setup conventions (fork from shared bytes, not independent
  creation)

## Code Standards

### License Header

All `.rs` files must include:

```rust
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
```

Note: The subtitle line differs from the server (`CLI client for
Getting Things Rusty` vs `ADHD-friendly task tracker...`).
