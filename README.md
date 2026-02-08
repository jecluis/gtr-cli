# Getting Things Rusty CLI

Command-line client for Getting Things Rusty - an ADHD-friendly task tracker
with offline-first CRDT synchronization.

## Status

**In Development** - Basic skeleton implemented, commands to be completed in
Phase 1.

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
authentication token.

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
  --size M
```

### 4. Show a Task

```bash
gtr show <task-id>
```

Shows task with pretty markdown rendering.

### 5. Update a Task

```bash
gtr update <task-id> --title "New title" --priority for-now
```

### 6. Delete a Task

```bash
gtr delete <task-id>
```

### 7. Search Tasks

```bash
gtr search "search query" --project my-project --limit 10
```

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

### Phase 1 (In Progress)

- [x] Project skeleton
- [ ] HTTP client
- [ ] Basic CRUD commands (list, show, create, update, delete)
- [ ] Pretty table output
- [ ] Markdown rendering with termimad

### Phase 1.5 (Planned)

- [ ] Local cache directory
- [ ] Offline read capability
- [ ] `gtr sync` command (pull-only)

### Phase 3 (Planned)

- [ ] Full offline CRDT sync
- [ ] Local .automerge storage
- [ ] Bi-directional sync
- [ ] Conflict resolution

### Phase 4 (Planned)

- [ ] `$EDITOR` integration
- [ ] Interactive prompts
- [ ] Shell completions

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
