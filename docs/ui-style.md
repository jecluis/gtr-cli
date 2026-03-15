# UI Style Conventions

## Hint/Placeholder Text

Never use `theme.muted` for hint or placeholder text. Use
`Style::default().fg(Color::Gray)` instead.

**Reason:** Muted text is too hard to read against most terminal
backgrounds.

**Applies to:** Any CLI display code rendering secondary/hint text —
labels, placeholders, disabled items, status indicators.
