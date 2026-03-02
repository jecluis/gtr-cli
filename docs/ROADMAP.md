# GTR CLI — Roadmap

Future enhancements and deferred work items.

## TUI

### Search

- **Full-text body search**: Currently search matches title only
  (from SQLite cache). Add body content search by loading tasks and
  documents from CRDT storage, matching what the CLI `search` command
  already does. Consider indexing body text in the cache for faster
  interactive filtering.

- **Server-side search**: Leverage the server's Tantivy full-text
  index via `GET /api/search` when online, falling back to local
  search when offline.

### Icons

- **Structured icon system**: The current `Glyphs` struct is a flat
  collection of `&'static str` fields with no per-glyph width
  metadata. Alignment padding (e.g. `impact_pad`, `work_pad`) is
  tracked as separate ad-hoc fields that must be kept in sync
  manually. Refactor into a `struct Icon { glyph: &str, width: usize }`
  so each glyph carries its own cell width, enabling a single
  `glyph_width()` accessor instead of proliferating pad fields.
  This also enforces correctness — today we rely on the implicit
  (undocumented) convention that row-prefix glyphs are 2-cell emoji
  in Unicode mode and 1-cell in Nerd mode, which will break the
  moment a glyph of a different width is used in a prefix position.
