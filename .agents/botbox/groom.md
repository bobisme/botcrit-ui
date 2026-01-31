# Groom

Groom a set of ready beads to improve backlog quality. Use this when you need to clean up beads without necessarily working on them.

## Arguments

- `$AGENT` = agent identity (optional)

## Steps

1. Check ready beads: `br ready`
2. For each bead from `br ready`, run `br show <bead-id>` and fix anything missing:
   - **Title**: Should be clear and actionable (imperative form, e.g., "Add /health endpoint"). If vague, update: `br update <bead-id> --title="..."`
   - **Description**: Should explain what and why. If missing or vague, add context: `br update <bead-id> --description="..."`
   - **Priority**: Should reflect relative importance. Adjust if wrong: `br update <bead-id> --priority=<1-4>`
   - **Labels**: Add labels if the bead fits a category (see label conventions). Create labels with `br label create <name>`, apply with `br label add <bead-id> <label>`.
   - **Acceptance criteria**: Description should include what "done" looks like. If missing, append criteria to the description.
   - **Testing strategy**: Description should mention how to verify the work (e.g., "run tests", "manual check", "curl endpoint"). If missing, append a brief testing note.
   - Add a comment noting what you groomed: `br comments add <bead-id> "Groomed by $AGENT: <what changed>"`
3. Check bead size: if a bead is large (epic, or description suggests multiple distinct changes), break it down:
   - Create smaller child beads with `br create` and `br dep add <child> <parent>`.
   - Add a comment to the parent: `br comments add <parent-id> "Broken down into smaller tasks: <child-id>, ..."`
4. Announce if you groomed multiple beads: `botbus send --agent $AGENT $BOTBOX_PROJECT "Groomed N beads: <summary>" -L mesh -L grooming`

## Acceptance Criteria

- All ready beads have clear, actionable titles
- Descriptions include acceptance criteria and testing strategy
- Priority levels make sense relative to each other
- Large beads are broken into smaller, atomic tasks
- Beads with the same owner/context are labeled consistently

## When to Use

- Before a dev agent starts a work cycle (ensures picking work is fast)
- After filing a batch of new beads (get them ready for triage)
- When you notice vague or overlapping beads (preventive cleanup)
- As a standalone task when other work is blocked
