# botcrit-ui

GitHub-style code review TUI for `botcrit`, built on `opentui_rust` (experimental fork of "opentui" under heavy development) with an Elm-style architecture (Model/Message/Update/View). Provides review lists, diff rendering (unified + side-by-side), thread anchoring, and themed UI.

## Multi-Agent Coordination

Use this template for decentralized mesh workflows across multiple agents.

## Mesh Protocol (Required)

Canonical spec: `https://raw.githubusercontent.com/bobisme/ai-docs/main/agents/mesh-protocol.md`.

Keep chat human-readable. Use labels for machine-readable events. Project is implied by channel.

Claims are mandatory for files, beads, and agent roles. Release claims when done.

## Rendering Tests (Botty)

Use Botty for rendering verification. The agent ID is always a **positional argument** (not a `--name` flag, except on `spawn`):

```bash
botty spawn --name <id> -- <command>   # spawn (--name sets the ID)
botty spawn --name <id> --no-resize -- <command>  # spawn, immune to view auto-resize
botty snapshot <id>                    # plain text snapshot (no ANSI)
botty snapshot --raw <id>              # snapshot with ANSI color codes
botty send <id> "<keys>"              # send printable keys
botty send-bytes <id> "<hex-bytes>"   # send raw bytes (e.g. "1b" for ESC)
botty kill <id>                        # kill a running agent
botty list                             # show all running agents
```

### Tips

- **Use absolute paths** when spawning — botty's cwd may differ from the project root:
  ```bash
  botty spawn --name crit-ui -- /home/bob/src/botcrit-ui/target/release/crit-ui
  ```
- **Use `--no-resize` for snapshot tests** — `botty view` auto-resizes terminals to match the viewer's dimensions. Use `--no-resize` on spawn to keep stable dimensions for programmatic snapshot comparisons.
- **Wait after spawn** before snapshot (`sleep 2-3`) to let the TUI render its first frame.
- **Wait after send** before snapshot (`sleep 0.3-0.5`) to let the app process input and redraw.
- **Prefer plain `botty snapshot <id>`** (without `--raw`) for analyzing layout and content. Use `--raw` only when verifying colors/styling.
- **`botty send-bytes <id> "1b"`** sends ESC — useful for Escape key presses in TUI apps.
- **Know the app's keybindings** before sending keys — `q` may quit the entire app, not navigate back. Check key mappings in source first.
- **After `botty kill`**, wait ~1s before respawning with the same ID.
- **Empty snapshots** can mean the app crashed on startup. Check `botty list` to verify it's still running.

If a real screenshot is needed, use Ghostty + `grim` to capture the window.

### CLI Deep-Link Flags

Use `--review`, `--file`, and `--thread` to skip the main menu and open directly to a specific location. Useful for spawning focused test instances:

```bash
# Open directly to a review
botty spawn --name crit-ui -- /home/bob/src/botcrit-ui/target/release/crit-ui --path /home/bob/src/botty/ws/default --review cr-qmr8

# Open to a specific file within a review
crit-ui --path /home/bob/src/botty/ws/default --review cr-qmr8 --file src/attach.rs

# Open with a specific thread expanded (also selects its file)
crit-ui --path /home/bob/src/botty/ws/default --review cr-qmr8 --thread th-lkxz
```

- `--file` and `--thread` require `--review`; without it they are silently ignored.
- If the review/file/thread ID doesn't exist, falls back gracefully (review list, or first file).
- `--thread` takes precedence over `--file` (it implies the file).

## Architecture Notes

### Module Structure

```
src/
├── input.rs          # Event → Message mapping (keyboard, mouse, resize)
├── layout.rs         # Named constants: THREAD_COL_WIDTH, SBS_LINE_NUM_WIDTH, etc.
├── stream.rs         # Diff stream layout computation
├── theme/mod.rs      # Theme + style token methods (style_muted(), style_line_number())
└── view/
    ├── components.rs # Shared: Rect, dim_rect, draw_help_bar, HotkeyHint
    └── diff/
        ├── mod.rs        # render_diff_stream, shared types (StreamCursor, DisplayItem)
        ├── analysis.rs   # map_threads_to_diff, diff_change_counts
        ├── unified.rs    # Unified diff line rendering
        ├── side_by_side.rs # SBS diff line rendering
        ├── comments.rs   # Comment block rendering
        ├── context.rs    # Orphaned context sections
        ├── helpers.rs    # Draw primitives (bars, base lines)
        └── text_util.rs  # Text wrapping, truncation, highlighting
```

### Data Access

Data comes from the `crit` CLI via `CliClient` (`src/cli_client.rs`), which shells out to `crit --format json --path <repo>`. The `CritClient` trait in `src/db.rs` abstracts the backend. There is no direct SQLite access — rusqlite was removed.

### Thread Anchoring (view/diff.rs)

Threads anchor to diff hunks via **new-side line numbers only** (`map_threads_to_diff`). A thread whose `selection_start` doesn't appear on the new side of any hunk is "orphaned" and rendered in a separate context section.

Key invariant: **all line-number matching in diff rendering must use new-side only.** Old-side line numbers can collide with thread line numbers from different commits, causing false matches. This applies to:
- `map_threads_to_diff` (anchoring decision)
- `sbs_anchor_map` / `sbs_comment_map` (SBS display position)
- `hunk_exclusion_ranges` (orphaned context clipping) — must exclude **both** old and new side ranges since orphaned context shows raw file lines that could overlap with either side of the diff

### Debugging Rendering Issues

To investigate visual duplication or layout bugs:
1. Use `crit review <id> --format json` to check thread `selection_start`/`selection_end` values
2. Use `git diff <from> <to> -- <file> | grep '^@@'` to see hunk ranges (old_start,old_count → new_start,new_count)
3. Spawn with `botty spawn --name <id> -- crit-ui --path <repo> --review <id> --file <path>` and scroll through in both unified (`v` to toggle) and SBS modes
4. Wider terminals expose more SBS bugs — don't only test with `--no-resize`

## BotBus Coordination

```bash
# Identity (once per session)
export BOTBUS_AGENT=$(bus generate-name)

# Project status
bus status
bus history
bus agents

# Communicate (chatty + labels)
bus send myproj "Working on bd-123" -L mesh -L task-claim

# File and bead claims (auto-announce in #general)
bus claim "bead://myproj/bd-123" -m "bd-123"
bus claim "src/path/**" -m "bd-123"
bus release --all
```

Conventions:

- Channels: `#general`, `#project-name`, `#project-topic`.
- Names: lowercase alphanumeric with hyphens.
- Messages: short, actionable, include bead IDs; use labels for event types.

## Agent Lease + Spawn

```bash
# Check if role is online
bus agents

# Claim agent lease
bus claim "agent://reviewer-security" -m "bd-123"

# Spawn (example)
botty spawn --name reviewer-security -- claude -p

# Announce in project channel
bus send myproj "Spawned reviewer-security" -L mesh -L spawn-ack
```

## MAW Workspaces (jj)

```bash
maw ws list
maw ws create <assigned-name>
cd .workspaces/<assigned-name>
jj status
jj diff
jj describe -m "wip: working on X"
```

Stale workspace:

```bash
maw ws sync
```

## Reviews (Botcrit)

- Open a review and request reviewers with `crit reviews request`.
- If a reviewer is offline, claim `agent://reviewer-<name>` and spawn them.
- Reviewers loop on pending review requests, then send `review-done` messages (labels preferred).

## Beads Workflow (Required)

Create a bead before work. Do not edit `.beads/issues.jsonl` by hand.

```bash
br ready
br create --title="..." --description="..." --type=task --priority=2
br update <id> --status=in_progress
br close <id>
br sync --flush-only
```

Use `bv --robot-*` commands for dependency-aware planning.

Suggested BV loop:

```bash
bv --robot-triage
bv --robot-plan
bv --robot-priority
bv --robot-next
```

Compact triage for deciding what to work on (reduces ~7KB to ~2KB):

```bash
bv --robot-triage 2>/dev/null | jq '{
  top_picks: [.triage.quick_ref.top_picks[] | {id, title, unblocks}],
  quick_wins: [.triage.quick_wins[] | {id, title, reason}],
  blockers: [.triage.blockers_to_clear[] | {id, blocked_by, unblocks: .unblocks_ids}],
  ranked: [.triage.recommendations[] | {id, title, type, p: .priority, score: (.score * 100 | round), action, blocked_by}],
  health: {open: .triage.project_health.counts.open, blocked: .triage.project_health.counts.blocked, velocity_7d: .triage.project_health.velocity.closed_last_7_days}
}'
```

## Spawned Agent Template (Peer)

```text
You are a peer agent working on [TASK] (bead: [BEAD]).

Before starting:
1. export BOTBUS_AGENT=[AGENT_NAME]
2. bus claim "bead://[PROJECT]/[BEAD]" -m "[BEAD]"
3. bus claim "src/[path]/**" -m "[BEAD]"
4. maw ws create [WORKSPACE]
5. cd .workspaces/[WORKSPACE]
6. bus send [PROJECT] "Working on [BEAD]" -L mesh -L task-claim

During work:
- Work only in your workspace
- Send task-update messages on progress or blockers

When done:
1. jj describe -m "[commit message]"
2. crit reviews create/request as needed
3. br update [BEAD] --status=closed
4. bus send [PROJECT] "Done: [BEAD]" -L mesh -L task-update
5. bus release --all
```

## Reviewer Agent Template

```text
You are a reviewer agent for [PROJECT].

Loop:
- Watch for review-request messages addressed to you
- Use crit to review/comment/approve
- Send review-done when finished
- Sleep/backoff when no reviews are pending
```

<!-- maw-agent-instructions-v1 -->

## Multi-Agent Workflow with MAW

This project uses MAW for coordinating multiple agents via jj workspaces.
Each agent gets an isolated working copy and **their own commit** - you can edit files without blocking other agents.

### Quick Start

```bash
maw ws create <your-name>      # Creates workspace + your own commit
cd .workspaces/<your-name>
# ... edit files ...
jj describe -m "feat: what you did"
maw ws status                  # See all agent work
```

### Quick Reference

| Task                 | Command                         |
| -------------------- | ------------------------------- |
| Create workspace     | `maw ws create <name>`          |
| Check status         | `maw ws status`                 |
| Sync stale workspace | `maw ws sync`                   |
| Merge all work       | `maw ws merge --all`            |
| Destroy workspace    | `maw ws destroy <name> --force` |

**Note:** Your workspace starts with an empty commit. This is intentional - it gives you ownership immediately, preventing conflicts when multiple agents work concurrently.

### During Work

```bash
jj diff                        # See changes
jj describe -m "feat: ..."     # Save work to your commit
jj commit -m "feat: ..."       # Commit and start fresh
```

### Stale Workspace

If you see "working copy is stale":

```bash
maw ws sync
```

### Conflicts

jj records conflicts in commits (non-blocking). If you see conflicts:

```bash
jj status                      # Shows conflicted files
# Edit files to resolve
jj describe -m "resolve: ..."
```

<!-- end-maw-agent-instructions -->

<!-- crit-agent-instructions -->

## Crit: Agent-Centric Code Review

This project uses [crit](https://github.com/anomalyco/botcrit) for distributed code reviews optimized for AI agents.

### Quick Start

```bash
# Initialize crit in the repository (once)
crit init

# Create a review for current change
crit reviews create --title "Add feature X"

# List open reviews
crit reviews list

# Show review details
crit reviews show <review_id>
```

### Reviewing Code

```bash
# Create a comment thread on specific lines
crit threads create <review_id> --file src/main.rs --lines 42-50

# Add a comment to a thread
crit comments add <thread_id> "This buffer should be cleared after use"

# List threads on a review
crit threads list <review_id>

# Resolve a thread
crit threads resolve <thread_id>
```

### Agent Best Practices

1. **Use optimistic locking** to avoid stale comments:

   ```bash
   crit comments add <thread_id> "message" --expected-hash <hash>
   ```

2. **Use request IDs** for idempotent retries:

   ```bash
   crit comments add <thread_id> "message" --request-id <uuid>
   ```

3. **Check status before acting**:

   ```bash
   crit status <review_id> --unresolved-only
   ```

4. **Run doctor** to verify setup:

   ```bash
   crit doctor
   ```

5. **Prefer stable output + paths** in automation:

   ```bash
   crit reviews list --format json --path /path/to/repo
   ```

### Output Formats

- Default output is TOON (token-optimized, human-readable)
- Use `--format json` for machine-parseable output (preferred for scripts)
- Use `--format text` for log-friendly output; `--format toon` for compact display
- JSON output includes file context for threads (v0.10.0+)

### Key Concepts

- **Reviews** are anchored to jj Change IDs (survive rebases)
- **Threads** are anchored to specific commit hashes (snapshots)
- **Drift detection** maps comments to current line numbers automatically

<!-- end-crit-agent-instructions -->

---

<!-- botbus-agent-instructions-v1 -->

## BotBus Agent Coordination

This project uses [BotBus](https://github.com/anomalyco/botbus) for multi-agent coordination. The CLI binary is `bus`. Before starting work, check for other agents and active file claims.

### Quick Start

```bash
# Register yourself (once per session)
bus register --name YourAgentName --description "Brief description"

# Check what's happening
bus status              # Overview of project state
bus history             # Recent messages
bus agents              # Who's registered

# Communicate
bus send general "Starting work on X"
bus send general "Done with X, ready for review"
bus send @OtherAgent "Question about Y"

# Coordinate file access
bus claim "src/api/**" -m "Working on API routes"
bus check-claim src/api/routes.rs   # Before editing
bus release --all                    # When done
```

### Best Practices

1. **Announce your intent** before starting significant work
2. **Claim files** you plan to edit to avoid conflicts
3. **Check claims** before editing files outside your claimed area
4. **Send updates** on blockers, questions, or completed work
5. **Release claims** when done - don't hoard files

### Channel Conventions

- `#general` - Default channel for project-wide updates
- `#backend`, `#frontend`, etc. - Create topic channels as needed
- `@AgentName` - Direct messages for specific coordination

### Message Conventions

Keep messages concise and actionable:

- "Starting work on issue #123: Add foo feature"
- "Blocked: need database credentials to proceed"
- "Question: should auth middleware go in src/api or src/auth?"
- "Done: implemented bar, tests passing"

<!-- end-botbus-agent-instructions -->

<!-- botbox:managed-start -->
## Botbox Workflow

**New here?** Read [worker-loop.md](.agents/botbox/worker-loop.md) first — it covers the complete triage → start → work → finish cycle.

**All tools have `--help`** with usage examples. When unsure, run `<tool> --help` or `<tool> <command> --help`.

### Directory Structure (maw v2)

This project uses a **bare repo** layout. Source files live in workspaces under `ws/`, not at the project root.

```
project-root/          ← bare repo (no source files here)
├── ws/
│   ├── default/       ← main working copy (AGENTS.md, .beads/, src/, etc.)
│   ├── frost-castle/  ← agent workspace (isolated jj commit)
│   └── amber-reef/    ← another agent workspace
├── .jj/               ← jj repo data
├── .git/              ← git data (core.bare=true)
├── AGENTS.md          ← stub redirecting to ws/default/AGENTS.md
└── CLAUDE.md          ← symlink → AGENTS.md
```

**Key rules:**
- `ws/default/` is the main workspace — beads, config, and project files live here
- Agent workspaces (`ws/<name>/`) are isolated jj commits for concurrent work
- Use `maw exec <ws> -- <command>` to run commands in a workspace context
- Use `maw exec default -- br|bv ...` for beads commands (always in default workspace)
- Use `maw exec <ws> -- crit ...` for review commands (always in the review's workspace)
- Never run `br`, `bv`, `crit`, or `jj` directly — always go through `maw exec`

### Beads Quick Reference

| Operation | Command |
|-----------|---------|
| View ready work | `maw exec default -- br ready` |
| Show bead | `maw exec default -- br show <id>` |
| Create | `maw exec default -- br create --actor $AGENT --owner $AGENT --title="..." --type=task --priority=2` |
| Start work | `maw exec default -- br update --actor $AGENT <id> --status=in_progress --owner=$AGENT` |
| Add comment | `maw exec default -- br comments add --actor $AGENT --author $AGENT <id> "message"` |
| Close | `maw exec default -- br close --actor $AGENT <id>` |
| Add dependency | `maw exec default -- br dep add --actor $AGENT <blocked> <blocker>` |
| Sync | `maw exec default -- br sync --flush-only` |
| Triage (scores) | `maw exec default -- bv --robot-triage` |
| Next bead | `maw exec default -- bv --robot-next` |

**Required flags**: `--actor $AGENT` on mutations, `--author $AGENT` on comments.

### Workspace Quick Reference

| Operation | Command |
|-----------|---------|
| Create workspace | `maw ws create <name>` |
| List workspaces | `maw ws list` |
| Merge to main | `maw ws merge <name> --destroy` |
| Destroy (no merge) | `maw ws destroy <name>` |
| Run jj in workspace | `maw exec <name> -- jj <jj-args...>` |

**Avoiding divergent commits**: Each workspace owns ONE commit. Only modify your own.

| Safe | Dangerous |
|------|-----------|
| `jj describe` (your working copy) | `jj describe main -m "..."` |
| `maw exec <your-ws> -- jj describe -m "..."` | `jj describe <other-change-id>` |

If you see `(divergent)` in `jj log`:
```bash
jj abandon <change-id>/0   # keep one, abandon the divergent copy
```

### Beads Conventions

- Create a bead before starting work. Update status: `open` → `in_progress` → `closed`.
- Post progress comments during work for crash recovery.
- **Push to main** after completing beads (see [finish.md](.agents/botbox/finish.md)).

### Identity

Your agent name is set by the hook or script that launched you. Use `$AGENT` in commands.
For manual sessions, use `<project>-dev` (e.g., `myapp-dev`).

### Claims

When working on a bead, stake claims to prevent conflicts:

```bash
bus claims stake --agent $AGENT "bead://<project>/<id>" -m "<id>"
bus claims stake --agent $AGENT "workspace://<project>/<ws>" -m "<id>"
bus claims release --agent $AGENT --all  # when done
```

### Reviews

Use `@<project>-<role>` mentions to request reviews:

```bash
maw exec $WS -- crit reviews request <review-id> --reviewers $PROJECT-security --agent $AGENT
bus send --agent $AGENT $PROJECT "Review requested: <review-id> @$PROJECT-security" -L review-request
```

The @mention triggers the auto-spawn hook for the reviewer.

### Cross-Project Communication

**Don't suffer in silence.** If a tool confuses you or behaves unexpectedly, post to its project channel.

1. Find the project: `bus history projects -n 50` (the #projects channel has project registry entries)
2. Post question or feedback: `bus send --agent $AGENT <project> "..." -L feedback`
3. For bugs, create beads in their repo first
4. **Always create a local tracking bead** so you check back later:
   ```bash
   maw exec default -- br create --actor $AGENT --owner $AGENT --title="[tracking] <summary>" --labels tracking --type=task --priority=3
   ```

See [cross-channel.md](.agents/botbox/cross-channel.md) for the full workflow.

### Session Search (optional)

Use `cass search "error or problem"` to find how similar issues were solved in past sessions.

### Workflow Docs

- [Ask questions, report bugs, and track responses across projects](.agents/botbox/cross-channel.md)
- [Close bead, merge workspace, release claims, sync](.agents/botbox/finish.md)
- [groom](.agents/botbox/groom.md)
- [Verify approval before merge](.agents/botbox/merge-check.md)
- [Turn specs/PRDs into actionable beads](.agents/botbox/planning.md)
- [Validate toolchain health](.agents/botbox/preflight.md)
- [Create and validate proposals before implementation](.agents/botbox/proposal.md)
- [Report bugs/features to other projects](.agents/botbox/report-issue.md)
- [Reviewer agent loop](.agents/botbox/review-loop.md)
- [Request a review](.agents/botbox/review-request.md)
- [Handle reviewer feedback (fix/address/defer)](.agents/botbox/review-response.md)
- [Explore unfamiliar code before planning](.agents/botbox/scout.md)
- [Claim bead, create workspace, announce](.agents/botbox/start.md)
- [Find work from inbox and beads](.agents/botbox/triage.md)
- [Change bead status (open/in_progress/blocked/done)](.agents/botbox/update.md)
- [Full triage-work-finish lifecycle](.agents/botbox/worker-loop.md)
<!-- botbox:managed-end -->
