---
name: manage-repository-tasks
description: "Manage the current Git repository's TODO tasks: Create, inspect, prioritize, relate, update, complete, reopen. Use for requests about tasks & TODOs like fixing bugs and introducing features, backlog items, priorities, parent/sub-task dependencies, task statuses, or the tasks/ and tasks/done/ directories. Do not invoke for ordinary implementation work unless the user also asks to record or change task status."
---

# Manage Repository Tasks

Use the bundled `tat` CLI as the authoritative interface for repository task records. Run it from anywhere inside the target Git repository.
Do not operate on task files manually when `tat` supports the operation (e.g. construct IDs, filenames, or move tasks) except if the user explicitly asks for it (in which case run `tat check` immediately afterward).

The binary is fully self-documenting. Run `tat --help` for basic help, `tat help <command>` for command-specific help, `tat guide` for orientation and the recommended workflow, or `tat reference` for the complete reference manual.

Enable Tab completion for the current shell:

```powershell
tat completions powershell | Out-String | Invoke-Expression
```

```bash
source <(tat completions bash)
```

Add the appropriate line to your PowerShell profile or Bash startup file to enable it in future sessions. Completion suggests commands, aliases, options, fixed values, and IDs from the current repository. PowerShell shows each suggested ID's title, status, and priority; Bash inserts the ID. `done` suggests not-done IDs and `reopen` suggests done IDs. Task suggestions require a valid repository and are read-only. File arguments use the shell's path completion. Use `tat completions <shell> --static` to print Clap's static completion script without repository-aware suggestions.

Task types, list modes and their aliases, and `--color` values are case-insensitive. For example, `--type BUG`, `--type Bug`, and `--type bug` all select `BUG`; filenames and JSON retain canonical values.

## Use the task model

Each task is a Markdown file with this filename format:

```text
<task-id>[, parent-<parent-task-id>][, blocks-<blocked-task-id>-<blocked-task-id>...], p<priority>, <TYPE>, <description>.md
```

Examples:

```text
t@bcde, p5, TASK, Begin working on this repo.md
t@bcde, parent-t@abcd, blocks-t@a101-t@a102, p3.5, BUG, Fix Windows failure.md
```

- Store not-done tasks directly under `tasks/` and done tasks under `tasks/done/`.
- Every ID starts with `t@`. `tat new` tries 100 random lowercase-alphanumeric suffixes of length four before trying length five, then repeats that rule at increasing lengths.
- ID uniqueness is repository-local and includes both not-done and done tasks.
- Omit the complete `parent-...` field when there is no parent.
- Omit the complete `blocks-...` field when the task blocks nothing. Separate multiple IDs in this filename field with `-`.
- Use priorities from 0 through 9; lower values execute first and the default is 5. Use decimals only for useful fine-grained ordering.
- Use the types `BUG`, `FEATURE`, `TASK`, and `GATE`. A `GATE` represents a condition that must be satisfied before blocked work can proceed.
- A task named in a not-done task's `blocks` field is blocked. If a parent is blocked, all its not-done descendants are blocked. Done tasks retain and validate relationships but do not act as blockers.
- Parent and blocks relationships can be nested, but the not-done dependency graph must remain acyclic.

Repository commands locate the nearest ancestor containing a `.git` marker and validate filenames, IDs, references, duplicate IDs, priorities, and dependency cycles before returning task data or changing task status. `tat id` is repository-independent and validates only the supplied task basename.

## Create tasks

Create a task with a concise description:

```powershell
$path = tat new 'Begin working on this repo'
$taskId = tat id $path
```

Add optional metadata and relationships:

```powershell
tat new 'Fix Windows failure' --priority 3.5 --type bug `
    --parent 't@abcd' --blocks 't@a101,t@a102'
```

Supply the initial Markdown body directly, from a file, or through stdin:

```powershell
tat new 'Document task model' --body 'Acceptance criteria go here.'
tat new 'Document task model' --body-file '.\task-body.md'
$body | tat new 'Document task model'
```

`tat new` initializes `tasks/` and `tasks/done/` when necessary and prints the new file's absolute path. The `--blocks`, `--add-blocks`, and `--remove-blocks` options accept IDs separated by commas, hyphens, whitespace, or a mixture; filenames always store them in the canonical dashed form. Never retry by inventing an ID manually.

## List and inspect tasks

Get the ID of the first ready task, ordered by numeric priority and then ID:

```powershell
$taskId = tat next
tat view $taskId
```

Use `tat next` when selecting one task; do not parse the first row of `tat list`.
List tasks when you need to compare ready work or inspect multiple records:

```powershell
tat list
tat list ready
tat list unblocked
```

The default output is a Markdown table whose first two columns are `ID` and `Description`. `--markdown` and `--output-markdown` explicitly request that same format. Use `--filenames` or `--filenames-output` for raw filenames only. Use `--json` for the versioned machine-readable record and resolved relationship data:

```powershell
tat list ready --json
tat list all --json
```

JSON schema version 4 includes each selected task's ID, title, complete Markdown body in `body_markdown`, nullable `created_at`, derived `completed_at`, the complete `completions` history, numeric and display priority, type, status, filename, direct parent/children and blocks/blocked-by relationships, and current direct or inherited blocking causes. Timestamps are canonical UTC RFC 3339 strings with millisecond precision. Relationship references include ID, title, status, and display priority. Reject unsupported schema versions rather than guessing.

Other list modes are:

```powershell
tat list blocked
tat list not-done
tat list done
tat list all
```

`active` is a compatibility alias for `ready`, as is `unblocked`. `not-done` lists every task that is ready or blocked.

Before emitting any list output, `tat` detects parent, blocks, and mixed dependency cycles. If a cycle exists, it reports the cycle on stderr, exits unsuccessfully, and emits no task output.

Inspect one task's metadata and Markdown body, or request only its filename:

```powershell
tat view 't@bcde'
tat view 't@bcde' --filename
```

`show` is an alias for `view`.

Export a read-only, self-contained interactive HTML dashboard with the bundled native viewer:

```powershell
tatviewer
tatviewer --all
tatviewer --recent 72 --output-path '.\tasks.html'
tatviewer --recent 2026-08-01 --open
```

There are two report presets. `tatviewer` starts with READY, BLOCKED, and RECENTLY DONE selected; `tatviewer --all` additionally selects OLD DONE. The four status buttons remain independent toggles. BUG, FEATURE, TASK, and GATE are a second row of independent on/off buttons directly below them and initially all four types are selected. `--recent 48` defines recent work as the inclusive 48-hour window ending at the most recent current completion in the repository; any other positive integer selects that many hours. `--recent YYYY-MM-DD` instead uses midnight at the start of that UTC calendar day as the inclusive cutoff. The default is 48 hours. Tasks with no known current completion are OLD DONE.

With no output path, `tatviewer` creates a unique temporary HTML file and prints its absolute path. It refuses to replace an existing explicit output unless `--force` is supplied. `tatviewer` owns Mermaid generation: it reads the complete validated model from `tat list all --json`, then limits the diagram to READY, BLOCKED, and RECENTLY DONE tasks. OLD DONE tasks never appear in the diagram, including in a `--all` report, although `--all` still includes their cards. `tat.exe` has no `graph`/`deps` command.

Rebuild the bundled `tat.exe` and `tatviewer.exe` with `tests/tatviewer/Rebuild-Tatviewer.ps1`. The script compiles both release binaries, copies them into the skill directory, validates the complex task fixture, and regenerates `tests/tatviewer/tatviewer-output.html`. Use this script for bundled binary updates so the pair and fixture output stay in sync.

The sticky header uses two compact logical lines. The first contains the inverted tree button, `TAT DASHBOARD`, the repository name at the same size as issue-card titles, status filters, type filters, a search control that expands from its icon when focused, and a minimal monochrome reset icon. Status and type filters share the same rounded-rectangle styling rather than pill shapes. This toolbar and its filter groups wrap whenever needed to stay within the page width. The second logical line contains only the live result count.

The task-card list is an indented parent-child tree with `#545F6B` connector lines. Each vertical connector is a link to its parent card, with a hit area that covers the two-pixel line and four pixels on either side. Filtering selects matching tasks and then adds their complete ancestor chains so the hierarchy stays intact. Ancestors that do not match appear as dimmed, title-only context cards; their active status rails show `R` or `B`. Search matches only a task's own ID, title, details, type, status, and priority. Relationship text does not make another task match. The viewer preserves report input order and provides no sorting control. The result count separates direct matches from ancestors shown for context. The collapsible graph appears after the task tree and has no vertical height limit or nested vertical scrollbar; it grows downward for as long as the diagram requires. The graph button beside the repository heading expands the graph, scrolls to it, moves keyboard focus there, and updates the report address to its stable `#graph` fragment. A report can therefore be opened or shared directly at the graph as `file:///c:/path/to/report.html#graph`. The graph uses a left-to-right layout for tasks connected by BLOCKS relationships. Tasks without blocking links in the included set appear in a compact list below it. Labels retain their natural size; wide flows scroll horizontally instead of shrinking text.

Graph references link to task cards, including cards outside the active dashboard filters, and IDs are bold. Flow nodes use GATE diamonds, BUG hexagons, and FEATURE/TASK rounded rectangles. Ready, blocked, and recently done tasks use green, red, and gray styling respectively. Red and green status text uses a 30%-darker variant of the corresponding structural color. Arrows represent only explicit BLOCKS relationships, including retained links on recently done tasks; they do not imply that completed work still blocks execution. Hovering a flow node shows its direct parent. A newly focused card highlights through a `#FFFF50` fill that fades over 2.88 seconds before returning to its normal status styling. The Mermaid renderer is bundled so the report stays self-contained and offline.

The dashboard uses the native variable UI fonts available on current Windows releases, with Aptos and Segoe UI fallbacks, and Cascadia for task IDs and code. It distinguishes display text, prose, IDs, labels, and pills through weight, spacing, and line height rather than making every element equally bold. Each task headline places a small space between the title and its adjoining pills. Issue titles use font weight 600 and the neutral dark-gray text color, while pill text uses weight 700. Ready and blocked cards show their status vertically inside a wider `#42965E` green or `#9E453F` red left rail and keep only type and priority pills beside the title; done cards retain their status pill. BUG pills use warm orange, FEATURE pills use violet, and TASK pills use blue, with separate accessible colors for light and dark themes. Parentage and children are communicated by the tree rather than separate PARENT or CHILDREN rows. BLOCKED BY: appears between the headline and task details. Its references omit status pills and instead use semibold dark-green or dark-red titles to show ready or blocked status; priority pills remain visible. BLOCKS: remains below the details. Both blocking labels use the red blocked pill style, and a blocked task referenced after BLOCKS: omits its redundant BLOCKED status pill while retaining its priority. Collapsed task details use the same Markdown styling as expanded details, replace every source line break with `↵ `, and clamp the result to three visual lines; click a truncated preview to expand the complete Markdown-rendered details with original blank lines. A delayed tooltip is available only while that preview is actually truncated, never after expansion. Card titles do not open detail tooltips. Instead, hover over an ID or title in a relationship row for the referenced task's title and detail preview. BLOCKED BY: omits done tasks and disappears when no current blocker remains. Use the viewer for visual review only; continue to make task changes through `tat`.

Extract an ID from a canonical filename or full path without parsing it manually:

```powershell
$taskId = tat id '.\tasks\t@bcde, p5, TASK, Begin working on this repo.md'
```

`tat id` parses the basename, does not require the file to exist, and can run outside a Git repository.

## Change tasks

Use `tat set` to preserve a task's ID, storage location, and body while changing selected filename metadata:

```powershell
tat set 't@bcde' --description 'Clarify the failure' --priority 2 --type bug
tat set 't@bcde' --parent 't@abcd'
tat set 't@bcde' --clear-parent
tat set 't@bcde' --blocks 't@a101,t@a102'
tat set 't@bcde' --add-blocks 't@a103' --remove-blocks 't@a101'
tat set 't@bcde' --clear-blocks
tat set 't@bcde' --body-file '.\replacement-body.md'
```

`edit`, `update`, and `change` are aliases for `set`. The command validates the prospective dependency graph before renaming or rewriting the task.

When a task's scope changes because of new evidence, update its description or body. Do not silently implement unrelated work documented in another task.

## Complete and reopen tasks

After confirming the work and its required verification are finished, complete it with either form:

```powershell
tat done 't@bcde'
tat complete 't@bcde'
```

Completion fails if the task is blocked or has any not-done descendant. A successful completion moves exactly that record to `tasks/done/`, preserves its filename and existing body, appends a formal completion timestamp, mirrors that instant in LastWriteTime, and prints the destination path. The directory is authoritative current state and the inline lifecycle metadata is authoritative history; LastWriteTime is only a sorting convenience because Git and file copies do not preserve it reliably.

Append completion notes whenever there is useful closure context:

```powershell
tat done t@bcde --completion-notes 'Implemented the retry cap; verified unit and timeout tests.'
tat done t@bcde --done-notes-file .\completion.md
```

tat appends each note after the existing task text under a timestamped heading such as `## Completion notes — 2026-08-30T09:12:04.552Z`. This pairs every note with its completion when a task is reopened and completed more than once. It is a good practice because it keeps original intent beside actual outcome, captures decisions and deviations, records verification evidence, and gives later maintainers or agents enough context to review, reopen, debug regressions, or plan follow-up work without reconstructing the ending from Git history or memory. Do not replace the original body with a completion summary.

tat records lifecycle timestamps in a hidden first-line JSON comment:

```markdown
<!-- tat-metadata: {"version":2,"created_at":"2026-08-30T08:41:35.123Z","completions":[]} -->
```

`created_at` is stable and may be null only when migrating a legacy file whose true creation time is unknown. `completions` appends one canonical UTC RFC 3339 timestamp for every completion and survives reopen/complete cycles. Current `completed_at` is derived in JSON only: for a file under `tasks/done/`, it is the last completion if known; otherwise it is null. Reopening does not mutate history. Legacy files without metadata remain readable and are promoted with unknown history rather than unreliable filesystem estimates when tat next changes them. Version 1 metadata is accepted and upgraded on change.

Reopen a done task with:

```powershell
tat reopen 't@bcde'
```

`open` is an alias for `reopen`. Reopening is rejected if it would create a not-done dependency cycle.

## Check, visualize, and remove

Check the repository explicitly or open/export its interactive dependency diagram:

```powershell
tat check
tatviewer
tatviewer --all
```

`validate` aliases `check`. Dependency visualization belongs to `tatviewer`, not `tat`.

Only remove a record when the user explicitly requests permanent deletion and the exact task is clear:

```powershell
tat remove 't@bcde' --force
```

Removal requires `--force` and is rejected while another task references the target. `delete` and `rm` are aliases for `remove`.

Run `tat --help` for basic help, `tat guide` for the recommended workflow, `tat reference` for the reference manual, or `tat <command> --help` for complete command syntax. Human-facing output uses semantic ANSI colors when stdout or stderr is a terminal; use global `--color always` to force color or `--color never` to suppress it. Auto mode keeps redirected and piped output plain.

## Write useful task bodies

For a bug or business-logic problem, capture current behavior, the concrete failure mode, intended behavior, likely implementation shape, required regression tests, and useful negative controls.

For other work, include enough context to act without rediscovery: the goal, relevant constraints, acceptance criteria, likely approach, and verification. Keep facts distinct from guesses and ask the policy owner when business intent is uncertain.

## Verify changes to this skill

After changing `tat` or this skill:

```powershell
cargo fmt --all -- --check
cargo test
node --test tests/graph-ui.cjs
.\tests\tatviewer\Rebuild-Tatviewer.ps1
tat --version
tatviewer --version
```

Also validate the skill directory with the skill-creator validator. Keep task records tracked in their owning Git repository and review their Git diff before handoff.
