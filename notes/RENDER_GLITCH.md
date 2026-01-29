# Review List Render Glitch Notes

## Summary of the problem
The first row of the review list duplicates or leaves artifacts when scrolling. In screenshots, the top row repeats and the selection marker appears twice, even though the data source contains unique rows. The issue reproduces in a minimal example, which suggests the problem is not specific to botcrit-ui's layout logic.

Observed behavior:
- The first list item can appear twice in the first two rows.
- The selection marker (`▸` or `>`) appears on multiple rows after repeated scroll input.
- The glitch appears after scrolling and sometimes right at initial render depending on terminal state.

## How I'm testing

Primary tooling:
- **Botty** (as documented in `AGENTS.md`):
  - `botty spawn --name crit-ui -- <command>`
  - `botty snapshot --raw --name crit-ui`
  - `botty send` / `botty send-bytes` to drive input
  - `botty kill --name crit-ui`

Example sequence:
```
botty spawn --name crit-ui -- bash -lc "cd ~/src/botty && ~/src/botcrit-ui/target/release/crit-ui"
botty send-bytes crit-ui "6a6a6a"   # jjj
botty snapshot --raw --name crit-ui
botty kill --name crit-ui
```

I also inspect snapshots by stripping ANSI codes and counting selection markers (`▸`) to confirm duplication.

## Things attempted so far

Changes in botcrit-ui:
- **Removed OPEN/CLOSED section headers** and switched to a flat, scrollable list.
- **Row background fill** on each list row to overwrite stale content.
- **Invalidate on list scroll** so list renders on every scroll event.
- **Parse all input events** in the read buffer (not just the first) to prevent partial updates.
- **Moved/removed header rows** in the list view to test if row 0 was special.
- **Adjusted list height and offsets** to align list rows with the visible viewport.

Changes in rendering pipeline:
- **Explicit `renderer.clear()`** before draw.
- **Disable terminal autowrap** (DECSET 7 off) via `AutoWrapGuard` (did not resolve).
- **Force absolute cursor moves** for every cell (no change).
- **Clamp width** in the app by subtracting columns (did not resolve duplication).
- **Replace Unicode ellipsis with ASCII `...`** to avoid wide-character wrap issues.

Outcome summary:
- Parsing all input events helps reduce stale rows but does **not** fully eliminate duplication.
- Changing wrap behavior and width clamps did **not** solve it.
- The glitch **still reproduces in a minimal example**, pointing toward the rendering library or terminal output strategy.

## What might still be going on

Possible remaining culprits:
- **Render loop behavior in `opentui_rust`** (diff renderer, cursor movement strategy, or terminal output buffering).
- **Row rendering with full-width writes** (if the final column is written, terminal may auto-wrap or the renderer might assume a wrap state).
- **Terminal output buffering edge case** where a full-row write and cursor movement causes the next row to be mispositioned.
- **Botty/terminal interaction** (botty snapshots reveal the issue consistently; worth verifying in a real terminal).

Next steps to isolate:
- Add a **second minimal example** that draws only a single row that changes, to test if the duplication is tied to list row iteration or any full-row writes.
- Force a **full screen clear** (`ESC[2J` + `ESC[H`) each frame to see if this is an output diff bug.
- Instrument `opentui_rust` render path to log cursor positions and row width used when writing rows.
- Test in a real terminal (Ghostty + `grim` screenshot) to confirm if the issue is botty-specific.

## Example app details

Minimal repro app: `examples/list_repro.rs`

What it does:
- Renders a simple list with selection.
- Scrolls with `j/k`.
- Uses the same input parser loop as the main app (parses all events).
- No DB, no theme system, no sidebar/diff panes.

Run it:
```
cargo run --example list_repro
```

Botty test:
```
botty spawn --name list-repro -- cargo run --example list_repro
botty snapshot --raw --name list-repro
botty send --name list-repro "jjj"
botty snapshot --raw --name list-repro
botty kill --name list-repro
```
