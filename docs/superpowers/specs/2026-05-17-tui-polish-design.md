# TUI Polish — Scrollable Detail, Markdown Rendering, Simpler Keys

Date: 2026-05-17
Status: design

## Problem

The TUI shipped in the 0.0.4 release works but feels rough:

1. **Detail pane is not scrollable.** Long body/acceptance blocks get clipped. The footer advertises `PgUp/PgDn scroll` but no code handles it.
2. **Body renders as literal markdown source.** Section headers print as `## body`, `## acceptance`. Bullets and bold/italics show as `**foo**` / `- bar` rather than as styled text.
3. **Important shortcuts are awkward.**
   - Start is on capital `S` (Shift+S). The Ctrl+S binding exists but is hidden from the footer and forces a tmux delivery path. Users want one obvious key.
   - "Show ready tasks" is a two-key chord `f r`. So is "clear filters" (`f c`).
   - Backlog tasks are reachable only by typing `f s` and then `backlog`; there's no quick way to glance at the backlog.

## Goals

- Detail pane scrolls and matches what the footer claims.
- Body/acceptance render with at least: styled section headings, bullet lists, bold, italic, and inline code — no raw `#` / `*` artifacts.
- One key (`Ctrl+S`) starts a task. Visible in the footer.
- One key (`Tab`) cycles list view: `active` → `done` → `backlog` → `active`.
- Default ("clean") view is `active` (status in `open` / `in_progress`), and the current view is shown as a chip in the top bar so it's obvious which slice you're looking at.

## Non-goals

- Full CommonMark support. Tables, nested blockquotes, links-with-titles, reference-style links, HTML — all out of scope.
- New persistent settings. The view cycle is in-memory only, resets on launch.
- Touching the CLI, db schema, or `start`/`open session` flows.

## Design

### Scrollable detail (`task_detail.rs`)

- `App` gains `detail_scroll: u16`. Reset to 0 whenever the selected task changes (cursor moves or reload).
- When `focus == Detail`, key handling:
  - `j` / `Down` → `detail_scroll += 1`
  - `k` / `Up`   → `detail_scroll = saturating_sub(1)`
  - `PgDn`      → `+= page` (page = max(1, viewport_height - 2))
  - `PgUp`      → `-= page` (saturating)
  - `g`          → 0
  - `G`          → end (clamped during render)
- `task_detail::render` passes `(detail_scroll, 0)` to `Paragraph::scroll`. Clamps so we don't scroll past `content_lines.saturating_sub(viewport)`.
- Viewport height is `area.height.saturating_sub(2)` (block borders).

Clamping happens in the widget at render time and writes back the clamped value via a small helper (`App::clamp_detail_scroll(max)`), called once per frame.

### Markdown rendering (`task_detail.rs` + new `markdown.rs`)

A small parser, not a dependency. Lives in `src/tui/widgets/markdown.rs`. Exposes:

```rust
pub fn render_block(input: &str) -> Vec<Line<'static>>;
```

Supported syntax (line-oriented, single pass):

| Source | Rendered |
|---|---|
| `# heading` | Bold, color Cyan |
| `## heading` | Bold, color Yellow |
| `### heading` | Bold |
| `- item` or `* item` | `  • item` (DIM bullet, normal text) |
| `  - nested` | `    ◦ nested` (one level of nesting) |
| ` ``` ` fenced code block | Each line raw, DIM gray bg-less |
| `` `inline` `` | Span with `Color::LightMagenta` |
| `**bold**` | Bold span |
| `*italic*` | Italic span (Modifier::ITALIC) |
| blank line | empty `Line` (kept, for spacing) |
| anything else | plain line, wrapped |

Parsing strategy: split into lines; detect block kind per line (heading / fence-open / fence-close / list / blank / text); within a text/list line, run an inline pass that walks the string accumulating spans, recognizing the three inline forms above. Unmatched `**` / `*` / `` ` `` are emitted as raw text rather than swallowing the trailing characters.

The detail pane stops emitting literal `## body` / `## acceptance`. Instead it renders body and acceptance as two markdown blocks separated by a styled divider:

```
T-12  fix scrollable detail               <- existing header (no change)
[open]  [feature]  p2
group: tui
deps: T-9 (done)

▸ body
<markdown-rendered body>

▸ acceptance
<markdown-rendered acceptance>
```

The `▸ body` / `▸ acceptance` lines use Bold + Cyan and a small triangle so they read as section markers without being literal `#`.

### Keybinding cleanup

`Filters` gets a new field:

```rust
#[derive(Default, Clone, PartialEq, Eq)]
pub enum ViewMode {
    #[default] Active,   // status in {open, in_progress}
    Done,                // status == "done"
    Backlog,             // status == "backlog"
}
```

stored as `filters.view: ViewMode`. `filtered_indices` consults `view` before the existing per-field filters. The `status` text filter (`f s`) layers on top of the view (so e.g. you can narrow `Active` to `in_progress`).

`is_default()` now also requires `view == Active`. `clear()` resets view to `Active`.

Key changes in `app.rs` / `keybindings.rs`:

| Old | New |
|---|---|
| `S` → start (TmuxDelivery::Off), footer-visible | removed |
| `Ctrl+S` → start (TmuxDelivery::ForceSpawn), hidden from footer | `Ctrl+S` → start (TmuxDelivery::ForceSpawn), **shown in footer** |
| `f r` → toggle ready_only | removed (use `Tab` for views; ready is still reachable via `f r` chord? **no** — removed) |
| `f c` → clear filters | removed |
| (none) | `Tab` → cycle `view` Active → Done → Backlog → Active, footer-visible |
| `f s` / `f g` / `f p` / `f b` | unchanged |

`ready_only` and `blocked_only` fields stay on `Filters` (CLI/internal callers may still set them); they're just no longer reachable from the TUI by chord. Or — simpler — we remove them entirely from `Filters` since `filtered_indices` will no longer be exercised that way from the TUI, but the field stays so the filter chip in the top bar still works if set programmatically. **Decision:** keep the fields, drop only the chord bindings, to minimize blast radius. Tests covering the chords are updated to cover `Tab` instead.

Footer (List scope) becomes:

```
? help · q quit · / filter · j/k nav · l detail · n new · e edit · s status · d done · x del · Ctrl+S start · Tab view
```

Footer (Detail scope) becomes:

```
? help · q quit · h list · e edit · j/k PgUp/PgDn scroll · g/G top/bottom
```

### Top bar

Always show the current view as the first chip after the title — `active`, `done`, or `backlog`. Styled the same as the existing filter chips so the user always knows what slice they're looking at.

When `view == Active` the chip is rendered in a slightly dimmer style so the "default" doesn't shout, but it's still visible.

## Testing

Per `templates/tdd-pursuit.md` — one behavior at a time, red then green.

### Scrollable detail
- `pgdn_in_detail_increments_scroll`
- `pgup_in_detail_decrements_scroll_saturating`
- `g_in_detail_resets_scroll_to_zero`
- `cursor_change_resets_detail_scroll`
- detail-pane render test isn't easy without a fake backend; covered indirectly by scroll-state assertions.

### Markdown
Pure-function unit tests in `widgets/markdown.rs`:
- `heading_one_renders_bold_cyan`
- `heading_two_renders_bold_yellow`
- `bullet_renders_with_bullet_glyph`
- `bold_inline_emits_bold_span`
- `italic_inline_emits_italic_span`
- `inline_code_emits_styled_span`
- `unmatched_marker_falls_back_to_raw_text`
- `fenced_code_block_preserves_lines`
- `blank_line_is_preserved`

### Keybindings & view cycle
- `tab_cycles_view_active_done_backlog_active`
- `ctrl_s_starts_task_with_default_delivery`
- `capital_s_no_longer_starts_task` (no `pending_start` set)
- `default_view_excludes_backlog`
- `default_view_includes_in_progress`
- `done_view_shows_only_done`
- `backlog_view_shows_only_backlog`
- existing `f_then_r_*` and `f_then_c_*` tests are removed (the chords are gone)
- footer-content tests updated to include `Tab` / `Ctrl+S` and exclude `S`

### Top bar
- `top_bar_shows_active_chip_by_default` — handled via a snapshot of the chip text list

## Open questions

None I need answered up-front. Going to proceed and revise if something looks wrong in practice.
