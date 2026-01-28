# Multi-Agent Coordination Template

Use this template for decentralized mesh workflows across multiple agents.

## Mesh Protocol (Required)

Canonical spec: `https://raw.githubusercontent.com/bobisme/ai-docs/main/agents/mesh-protocol.md`.

Keep chat human-readable. Use labels for machine-readable events. Project is implied by channel.

Claims are mandatory for files, beads, and agent roles. Release claims when done.

## Setup (One-Time per Repo)

Use tool-provided AGENTS.md snippets when available:

- BotBus: `botbus agentsmd show` then `botbus agentsmd init`
- MAW: `maw agents init`
- Botcrit: `crit agents show` then `crit agents init`

Tools without injection should be added manually:

- Botty: add runtime/testing notes if needed
- Beads (br/bv): include the Beads Workflow block below

## Rendering Tests (Botty)

Use Botty for rendering verification with color escape codes:

```bash
botty spawn --name crit-ui -- <command>
botty snapshot --raw --name crit-ui
botty send --name crit-ui "<keys>"
botty send-bytes --name crit-ui "<hex-bytes>"
botty kill --name crit-ui
```

If a real screenshot is needed, use Ghostty + `grim` to capture the window.

## BotBus Coordination

```bash
# Identity (once per session)
export BOTBUS_AGENT=$(botbus generate-name)

# Project status
botbus status
botbus history
botbus agents

# Communicate (chatty + labels)
botbus send myproj "Working on bd-123" -L mesh -L task-claim

# File and bead claims (auto-announce in #general)
botbus claim "bead://myproj/bd-123" -m "bd-123"
botbus claim "src/path/**" -m "bd-123"
botbus release --all
```

Conventions:

- Channels: `#general`, `#project-name`, `#project-topic`.
- Names: lowercase alphanumeric with hyphens.
- Messages: short, actionable, include bead IDs; use labels for event types.

## Agent Lease + Spawn

```bash
# Check if role is online
botbus agents

# Claim agent lease
botbus claim "agent://reviewer-security" -m "bd-123"

# Spawn (example)
botty spawn --name reviewer-security -- claude -p

# Announce in project channel
botbus send myproj "Spawned reviewer-security" -L mesh -L spawn-ack
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

## Spawned Agent Template (Peer)

```text
You are a peer agent working on [TASK] (bead: [BEAD]).

Before starting:
1. export BOTBUS_AGENT=[AGENT_NAME]
2. botbus claim "bead://[PROJECT]/[BEAD]" -m "[BEAD]"
3. botbus claim "src/[path]/**" -m "[BEAD]"
4. maw ws create [WORKSPACE]
5. cd .workspaces/[WORKSPACE]
6. botbus send [PROJECT] "Working on [BEAD]" -L mesh -L task-claim

During work:
- Work only in your workspace
- Send task-update messages on progress or blockers

When done:
1. jj describe -m "[commit message]"
2. crit reviews create/request as needed
3. br update [BEAD] --status=closed
4. botbus send [PROJECT] "Done: [BEAD]" -L mesh -L task-update
5. botbus release --all
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

| Task | Command |
|------|---------|
| Create workspace | `maw ws create <name>` |
| Check status | `maw ws status` |
| Sync stale workspace | `maw ws sync` |
| Merge all work | `maw ws merge --all` |
| Destroy workspace | `maw ws destroy <name> --force` |

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

### Output Formats

- Default output is TOON (token-optimized, human-readable)
- Use `--json` flag for machine-parseable JSON output

### Key Concepts

- **Reviews** are anchored to jj Change IDs (survive rebases)
- **Threads** are anchored to specific commit hashes (snapshots)
- **Drift detection** maps comments to current line numbers automatically

<!-- end-crit-agent-instructions -->

---

<!-- botbus-agent-instructions-v1 -->

## BotBus Agent Coordination

This project uses [BotBus](https://github.com/anomalyco/botbus) for multi-agent coordination. Before starting work, check for other agents and active file claims.

### Quick Start

```bash
# Register yourself (once per session)
botbus register --name YourAgentName --description "Brief description"

# Check what's happening
botbus status              # Overview of project state
botbus history             # Recent messages
botbus agents              # Who's registered

# Communicate
botbus send general "Starting work on X"
botbus send general "Done with X, ready for review"
botbus send @OtherAgent "Question about Y"

# Coordinate file access
botbus claim "src/api/**" -m "Working on API routes"
botbus check-claim src/api/routes.rs   # Before editing
botbus release --all                    # When done
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
