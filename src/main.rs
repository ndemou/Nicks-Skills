use clap::{ArgAction, Args, CommandFactory, FromArgMatches, Parser, Subcommand, ValueEnum};
use rand::Rng;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::{SystemTime, UNIX_EPOCH};
use tat::list_json::{
    BlockingCause, TASK_LIST_SCHEMA_VERSION, TaskListDocument, TaskListItem, TaskReference,
    TaskRelationships,
};
use unicode_width::UnicodeWidthStr;

type AppResult<T> = Result<T, String>;

const TASK_METADATA_VERSION: u32 = 2;
const TASK_METADATA_PREFIX: &str = "<!-- tat-metadata: ";
const TASK_METADATA_SUFFIX: &str = " -->";

struct RuntimeDiagnostic {
    summary: String,
    explanation: String,
    details: Option<String>,
    action: String,
}

static COLOR_STDOUT: AtomicBool = AtomicBool::new(false);
static COLOR_STDERR: AtomicBool = AtomicBool::new(false);

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_HEADING: &str = "\x1b[1;36m";
const ANSI_COMMAND: &str = "\x1b[1;32m";
const ANSI_ID: &str = "\x1b[1;36m";
const ANSI_PARENT: &str = "\x1b[34m";
const ANSI_BLOCKS: &str = "\x1b[31m";
const ANSI_PRIORITY: &str = "\x1b[33m";
const ANSI_KIND: &str = "\x1b[35m";
const ANSI_READY: &str = "\x1b[1;32m";
const ANSI_BLOCKED: &str = "\x1b[1;31m";
const ANSI_DONE: &str = "\x1b[2;32m";
const ANSI_WARNING: &str = "\x1b[1;33m";
const ANSI_ERROR: &str = "\x1b[1;31m";
const ANSI_DIM: &str = "\x1b[2m";
const ANSI_DESCRIPTION: &str = "\x1b[37m";

const TOP_LEVEL_ABOUT: &str = "tat manages repository-local work as reviewable Markdown files. It is designed\nfor humans and agents that want a small, Git-friendly backlog without a service\nor database.";

const TOP_LEVEL_HELP: &str = r#"QUICK START
  tat new "Implement retry handling" --priority 3 --type feature
  tat next                         Print the ID of the first ready task
  tat view t@abcd                  Inspect one task and its Markdown body
  tat set t@abcd --priority 2      Change selected metadata
  tat done t@abcd                  Move finished work to tasks/done/

FIND THE RIGHT COMMAND
  Create             new
  Find work          next, list [ready|blocked|not-done|done|all]
  Inspect            id, view, check
  Change             set, reopen
  Finish             done
  Permanently delete remove --force

LEARN AND TROUBLESHOOT
  tat help <COMMAND>    More detailed help for a particular command
  tat guide             Orientation and recommended workflow (How To)
  tat reference         The reference manual"#;

const NEW_HELP: &str = r#"BEHAVIOR
  Creates tasks/ and tasks/done/ when needed, validates existing records and
  requested relationships, generates a repository-unique ID, writes the body,
  and prints the new task's absolute path to stdout.

  IDs start with t@ and normally have four random lowercase-alphanumeric
  characters after that prefix. The description becomes part of the filename,
  so quote it and avoid Windows-invalid filename characters.

BODY INPUT
  Use exactly one of --body, --body-file, or piped stdin. With no body source,
  an interactive invocation creates an empty Markdown body.

EXAMPLES
  tat new "Investigate slow startup"
  tat new "Fix Windows crash" -p 2.5 -t bug --body "Repro and acceptance criteria"
  tat new "Implement parser" --parent t@abcd
  tat new "Release gate" --type gate --blocks t@a101,t@a102
  Get-Content .\\task-body.md -Raw | tat new "Document task model"

See `tat reference` before creating complex
relationships. The returned path is suitable for scripts and command capture;
pass it to `tat id` when only the generated ID is needed."#;

const LIST_HELP: &str = r#"MODES
  ready       Tasks that can start now (default; aliases: unblocked, active)
  blocked     Not-done tasks prevented from starting
  not-done    Every task not yet done, whether ready or blocked
  done        Tasks from tasks/done/
  all         Every not-done and done task

OUTPUT
  Markdown is the default. Its first columns are ID and Description, followed
  by Prio, Type, Status, Parent, and Blocks. --filenames emits only raw
  basenames, one per line. --json emits the versioned machine-readable task
  records, complete Markdown bodies, and resolved relationships. Results sort
  by numeric priority, then ID.

SAFETY
  The full repository is validated before stdout is written. Dependency cycles
  and malformed or missing references produce an error on stderr, a nonzero
  exit status, and no partial task output.

EXAMPLES
  tat list
  tat next
  tat list unblocked
  tat list blocked
  tat list all --markdown
  tat list ready --filenames
  tat list all --json

For scripting details, see `tat reference`."#;

const NEXT_HELP: &str = r#"OUTPUT
  Prints only the ID of the first ready task, using the same numeric-priority and
  ID tie-break ordering as `tat list ready`. The complete task store is validated
  first. If no ready task exists, tat explains whether to inspect blocked or
  not-done work and exits unsuccessfully without stdout.

EXAMPLES
  tat next
  $taskId = tat next
  if ($LASTEXITCODE -eq 0) { tat view $taskId }

Use `tat list ready` when you want to compare every ready task and its metadata."#;

const ID_HELP: &str = r#"OUTPUT
  Extracts and prints the task ID from a canonical task filename or from the last
  component of a full path. No Git repository is required, and the file need not
  exist. The complete basename is validated before its ID is returned.

EXAMPLES
  tat id 't@bcde, p5, TASK, Begin working on this repo.md'
  tat id 'C:\repo\tasks\t@bcde, p5, TASK, Begin working on this repo.md'
  $path = tat new 'Investigate startup latency'
  $taskId = tat id $path

Quote literal filenames and paths because task descriptions normally contain spaces."#;

const DONE_HELP: &str = r#"BEHAVIOR
  Moves exactly one not-done task from tasks/ to tasks/done/ and prints the
  destination's absolute path. The ID, filename metadata, and existing Markdown
  body are preserved. If --completion-notes or --completion-notes-file is supplied,
  tat appends a timestamped "Completion notes" section after that existing body.
  The completion timestamp is appended to the task's hidden lifecycle history.
  The destination file's LastWriteTime mirrors that time for convenient sorting,
  but the inline metadata remains authoritative. Done tasks no longer block other tasks.

  Complete a task only after its work and required verification are finished.
  Completion is refused when the task is blocked, already done, or has any
  not-done descendant. Finish descendants first and satisfy not-done blockers before
  retrying.

COMPLETION NOTES
  Completion notes are strongly recommended. Record what actually changed, important
  decisions or deviations from the original plan, and the verification performed.
  Appending instead of replacing the original task text preserves intent beside
  outcome, so future maintainers and agents do not need to reconstruct closure from
  commits, terminal history, or memory.

EXAMPLES
  tat done t@abcd
  tat done t@abcd --completion-notes "Implemented retries; verified timeout tests"
  tat done t@abcd --done-notes-file .\completion.md
  tat complete t@abcd
  tat list blocked                 Diagnose why work cannot complete
  tat view t@abcd                  Review scope and body before completion

Use `tat reopen t@abcd` to restore a done task."#;

const SET_HELP: &str = r#"BEHAVIOR
  Changes only the requested fields while preserving the task ID and storage
  location. The Markdown body is preserved unless --body or --body-file is supplied.
  The prospective graph is validated before any rename or rewrite occurs.

RELATIONSHIP OPTIONS
  --parent ID / --clear-parent      Replace or remove the parent
  --blocks IDS / --clear-blocks     Replace or remove the complete blocks list
  --add-blocks IDS                  Add IDs not already present
  --remove-blocks IDS               Remove selected IDs

  Separate IDS with commas, hyphens, whitespace, or any mixture of them.
  --blocks and --clear-blocks cannot be combined with incremental block edits.
  At least one effective change is required.

EXAMPLES
  tat set t@abcd --description "Clarify retry behavior" -p 2 -t bug
  tat set t@abcd --parent t@root
  tat set t@abcd --clear-parent
  tat set t@abcd --blocks t@a101,t@a102
  tat set t@abcd --add-blocks t@a103 --remove-blocks t@a101
  tat set t@abcd --clear-blocks
  tat set t@abcd --body-file .\\revised-task.md

Aliases: edit, update, change. See `tat reference` for edge direction
and cycle rules."#;

const REOPEN_HELP: &str = r#"BEHAVIOR
  Moves one done task from tasks/done/ back to tasks/ and prints the not-done
  path. Its ID, metadata, and body are preserved. Reopening a not-done task fails.

  Because done relationships rejoin the not-done graph, reopening is rejected if
  it would introduce a not-done dependency cycle. A reopened blocker can also make
  its targets and their descendants blocked again; check `tat list blocked`.

EXAMPLES
  tat reopen t@abcd
  tat open t@abcd
  tat list done                    Find done task IDs
  tat view t@abcd                  Inspect before reopening"#;

const VIEW_HELP: &str = r#"OUTPUT
  Prints a Markdown heading, metadata table, filename, and the task's Markdown
  body. It finds tasks in both tasks/ and tasks/done/. --filename changes
  stdout to the raw basename only, which is useful for scripts.

EXAMPLES
  tat view t@abcd
  tat show t@abcd
  tat view t@abcd --filename

Use `tat list all` when the ID is unknown."#;

const REMOVE_HELP: &str = r#"WARNING
  Permanently deletes one task record. This is different from `tat done`, which
  retains history under tasks/done/. Removal always requires --force and is
  refused while any task names the target as a parent or in
  its blocks field. The target may still have outgoing blocks edges, so deleting
  it can immediately make other tasks ready. Inspect the graph before and the ready
  list after deletion.

  Use remove only when permanent deletion was explicitly requested and the exact
  target has been confirmed. Prefer done when repository history remains useful.

EXAMPLES
  tat view t@abcd                  Confirm the exact target first
  tat remove t@abcd --force
  tat rm t@abcd --force

If removal reports references, inspect them with `tat view <ID>` and explicitly
change those relationships with `tat set` before retrying."#;

const CHECK_HELP: &str = r#"CHECKS
  - task filename grammar, ID prefix and length, priority, and task type
  - duplicate IDs across not-done and done records
  - parent and blocks references to missing tasks
  - parent-only, blocks-only, and mixed cycles among not-done tasks

OUTPUT
  Success prints not-done and done counts to stdout and exits 0. Failure prints
  the diagnostic to stderr, exits nonzero, and does not modify task records.

EXAMPLES
  tat check
  tat validate                      Alias for check

Run this after resolving merge conflicts or manually recovering task files."#;

const GUIDE_HELP: &str = r#"HOW TO
  Read this guide when adopting tat or when you want the recommended workflow.
  Use `tat reference` for the complete task model, automation contracts, aliases,
  color behavior, and recovery procedures.

The guide does not require a repository and is safe to capture or search."#;

const REFERENCE_HELP: &str = r#"REFERENCE
  The complete task model, dependency rules, command aliases, output contracts,
  color behavior, and recovery procedures. Use `tat guide` for orientation and
  the recommended workflow.

The reference does not require a repository and is safe to capture or search."#;

const GUIDE_OVERVIEW: &str = r#"TAT GUIDE

PURPOSE
  tat manages repository-local work as reviewable Markdown files. It is designed
  for humans and agents that want a small, Git-friendly backlog without a service
  or database.

Task IDs are generated by `tat new`; do not invent or rename them manually.

WHEN TO USE TAT
  Use tat when asked to create, inspect, prioritize, relate, update, complete,
  reopen, or remove repository tasks. Ordinary implementation work does not by
  itself authorize changing task records; mutate the backlog only when the user
  also asks to record work or change task status.

AGENT OPERATING RULES
  - Treat tat as the authoritative interface. Do not construct IDs, rename task
    files, edit filename metadata, or move records manually when tat supports the
    operation.
  - If the user explicitly requires a manual task-file edit, preserve the task
    body and valid ID, make the smallest requested change, and immediately run
    `tat check`. For an already-invalid store, follow `tat reference` recovery.
  - Read `tat view <ID>` before acting. Keep facts distinct from guesses and ask
    the policy owner when intended behavior cannot be inferred safely.
  - When new evidence changes existing scope, update the task with `tat set`. Do
    not silently absorb unrelated work into the current task; report it, and use
    `tat new` only when the user wants it tracked.
  - Complete a task only after its work and required verification are finished.
  - Permanently remove a task only when deletion was explicitly requested and the
    exact target is clear. Prefer `tat done` when history should remain.
  - Keep task records in version control and review their repository diff before
    committing or handing off changes.

REPOSITORY LAYOUT
  <repository>/tasks/*.md         Not-done tasks (ready or blocked)
  <repository>/tasks/done/*.md    Done tasks

  Run tat from the repository root or any descendant. tat walks upward to the
  nearest .git file or directory. `tat new` initializes the task directories;
  other commands never create a repository for you.

RECOMMENDED WORKFLOW
  1. `tat check`                  Confirm the task store is coherent.
  2. `$taskId = tat next`         Select the lowest-priority-number ready task.
  3. `tat view $taskId`           Read its metadata, context, and acceptance criteria.
  4. Do and verify the work.
  5. `tat done $taskId --completion-notes "<outcome and verification>"`
                                  Record completion and unblock dependents.
  6. Repeat `tat next`.

  Record newly discovered work with `tat new`; revise existing scope with
  `tat set`; use `tat reopen` when done work becomes not-done again. Prefer
  completion over permanent removal so repository history remains useful.

  Append completion notes whenever useful. They preserve what was actually done,
  why the implementation differed from the original scope, and how it was verified.
  Keeping intent and outcome together makes review, reopening, handoff, regression
  diagnosis, and future planning substantially faster.

WRITE ACTIONABLE TASKS
  For bugs and business-logic problems, record current behavior, the concrete
  failure, intended behavior, likely implementation shape, required regression
  tests, and useful negative controls.

  For other work, record the goal, relevant constraints, acceptance criteria,
  likely approach, and verification. Mark uncertainty honestly: keep facts
  distinct from guesses and ask the policy owner when business intent is unclear.

SAFETY GUARANTEES
  - IDs are generated and checked across not-done and done tasks.
  - Reads and mutations validate the complete task store first.
  - Relationship changes are rejected if they would create a not-done cycle.
  - Completion is rejected for blocked tasks and parents with not-done descendants.
  - Permanent removal requires --force and an unreferenced target.

  Fail-closed validation can require careful manual filename recovery when the
  existing store is already invalid. Use `tat reference` first.

NEXT
  `tat reference`             Read the complete reference manual.
  `tat help <COMMAND>`        Get detailed command syntax and examples."#;

const REFERENCE_INTRO: &str = r#"TAT REFERENCE MANUAL

SCOPE
  This is the complete reference for tat's task model, dependency semantics,
  automation contracts, aliases, color behavior, and recovery procedures.
  Read `tat guide` first for orientation and the recommended workflow.

COMMAND ALIASES
  ls=list; complete/close=done; edit/update/change=set; open=reopen;
  show=view; delete/rm=remove; validate=check.
  list unblocked and list active alias list ready. --filenames-output aliases
  --filenames; --output-markdown aliases --markdown.

COLOR
  --color auto      Color only terminal output (default; honors NO_COLOR,
                    CLICOLOR=0, and TERM=dumb)
  --color always    Force ANSI colors, including redirected output
  --color never     Never emit ANSI colors"#;

const GUIDE_MODEL: &str = r#"TASK MODEL

FILENAME GRAMMAR
  <id>[, parent-<PARENT>][, blocks-<ID>-<ID>...], p<PRIORITY>, <TYPE>, <DESCRIPTION>.md

  t@bcde, p5, TASK, Begin working on this repo.md
  t@bcde, parent-t@abcd, blocks-t@a101-t@a102, p3.5, BUG, Fix Windows failure.md

  The parent and blocks fields are optional as whole fields. Empty placeholders
  are invalid. The relationship label is separated from its first ID by `-`, and
  multiple IDs in a blocks field are also separated by `-`. tat writes parent
  before blocks and preserves the visible Markdown body unless a body replacement or
  appended completion note is requested. tat adds or upgrades its hidden metadata
  comment whenever a task is created or changed.

  Command input is more flexible than stored filenames: --blocks, --add-blocks,
  and --remove-blocks accept IDs separated by commas, hyphens, whitespace, or a
  mixture. tat deduplicates and sorts them before writing the canonical dashed form.

IDS
  - Every ID begins with t@ and uses lowercase ASCII letters and digits.
  - New IDs normally contain four random characters after t@: t@abcd.
  - tat makes 100 attempts at that length, then increases the suffix by one.
  - Uniqueness is local to the repository and covers tasks/ plus tasks/done/.
  - IDs are stable for the task's lifetime. Generate them only with `tat new`.
  - Use `tat id <PATH_OR_FILENAME>` to extract an ID from a canonical basename;
    do not split filenames manually.

STATUS AND STORAGE
  Every task exposes exactly one status: ready, blocked, or done. Ready and blocked
  records are files directly under tasks/; done records are under tasks/done/.
  `tat done` and `tat reopen` move the file without inventing a new ID. Completion
  notes, when supplied, are appended after the existing body rather than replacing it.
  The directory is the single authoritative source of current status; status is not
  duplicated in lifecycle metadata.

LIFECYCLE METADATA
  tat stores machine-readable lifecycle metadata in a hidden HTML comment on the
  first line of every task it creates or updates:

    <!-- tat-metadata: {"version":2,"created_at":"2026-08-30T08:41:35.123Z","completions":[]} -->

  Timestamps use canonical UTC RFC 3339 with millisecond precision. `created_at` is
  stable for the task's lifetime. `completions` appends one timestamp on every
  successful completion and survives reopen/complete cycles. Current completion is
  derived only when the file is under tasks/done/: it is the last history entry, if
  known. Reopening therefore does not erase or rewrite lifecycle history.

  LastWriteTime is set to the same instant during completion, but Git and copies do
  not preserve it reliably and tat never treats it as authoritative lifecycle data.
  Legacy files without metadata remain readable. When tat next changes one, it records
  `created_at: null` rather than inventing history from filesystem timestamps. Version
  1 metadata is read and upgraded on the next change. Do not edit the comment manually.

PRIORITY
  Priorities range from 0 through 9. Lower numbers come first; the default is 5.
  Decimals such as 2.5 are allowed. Listings break equal-priority ties by ID.

TYPE
  BUG       Incorrect existing behavior
  FEATURE   New user-visible or system capability
  TASK      General implementation, investigation, or maintenance work
  GATE      A condition that blocks work until satisfied, such as a date or approval

DESCRIPTION AND BODY
  The short description is filename metadata. It must be nonempty, cannot end in
  a period or space, and cannot contain Windows-invalid filename characters
  (< > : " / \\ | ? * or control characters). Put detailed context, constraints,
  acceptance criteria, and verification in the Markdown body.

  For bugs or business-logic problems, include current behavior, the concrete
  failure, intended behavior, likely implementation shape, required regression
  tests, and useful negative controls. For other work, include the goal,
  constraints, acceptance criteria, likely approach, and verification. Keep facts
  distinct from guesses and ask the policy owner when business intent is unclear.

VALIDATION
  Do not hand-rename valid task records. Use `tat set` so the resulting filename,
  references, and dependency graph are checked before mutation. The sole exception
  is deliberate recovery of a store that is already invalid and therefore rejected
  by every task command; follow `tat reference` and run `tat check`
  after the smallest necessary manual repair."#;

const GUIDE_DEPENDENCIES: &str = r#"DEPENDENCIES AND READINESS

RELATIONSHIPS
  parent-<P>       Declares P as the task's structural parent.
  blocks-<A>-<B>   Declares that this task prevents A and B from starting.

  `tat new "Gate" --blocks t@work` creates the directed edge Gate -> Work.
  `tat new "Child" --parent t@parent` creates the hierarchy Parent -> Child.
  A relationship is explicit: similarity or shared subject matter creates no edge.

READY VS BLOCKED
  A not-done task is directly blocked when another not-done task names its ID in blocks.
  Blocking also propagates down the parent hierarchy: if a parent is blocked, all
  not-done descendants are blocked. A not-done child is not blocked merely because
  its parent is not done.

  Done tasks retain relationship metadata for history and reference validation,
  but they do not block not-done work. Completing a blocker can therefore
  make its targets and their descendants ready immediately.

  Reopening reverses that effect: if the done task has outgoing blocks edges,
  making it not-done again can re-block its targets and all their descendants. Check
  `tat list blocked` after reopening relationship-heavy work.

CYCLES
  Not-done parent and blocks edges form one directed graph. Parent-only,
  blocks-only, and mixed cycles are invalid. Every command validates the graph;
  `new`, `set`, and `reopen` validate the prospective graph before changing files.

  If `A blocks B` and A is made a child of B, the edges A -> B and B -> A form a
  mixed cycle and the edit is rejected. Inspect the reported cycle path and, when
  the current store is valid, use `tatviewer` to review the existing relationships;
  remove or reverse the edge that misstates the intended order.

COMPLETION ORDER
  A blocked task cannot be completed. A task with not-done descendants also cannot
  be completed, so finish the deepest children first. The structural parent can
  be completed after all descendants are done and its blockers are satisfied.

EDITING RELATIONSHIPS
  tat set <ID> --parent <P>              Replace parent
  tat set <ID> --clear-parent            Remove parent
  tat set <ID> --blocks <A,B>            Replace blocks list
  tat set <ID> --add-blocks <C-D>        Add targets
  tat set <ID> --remove-blocks "<A> <B>" Remove targets
  tat set <ID> --clear-blocks            Remove all targets

Block-list options accept commas, hyphens, whitespace, or mixed separators.

Use `tat list ready`, `tat list blocked`, and `tatviewer` to verify the result."#;

const GUIDE_AUTOMATION: &str = r#"AUTOMATION CONTRACT

PROCESS CONTRACT
  Exit 0    Command, --help, or --version succeeded.
  Exit 1    Repository/task validation or requested operation failed.
  Exit 2    Command-line syntax was invalid or a required argument was absent.

  Operation diagnostics use stderr. Successful data uses stdout. Clap syntax
  errors include usage. Runtime errors follow this human-readable structure:

    error: <terse expert summary>

    <plain-language explanation of what happened and why tat stopped>

    details:
    <exact task IDs, paths, cycles, references, or operating-system error>

    try: <specific recovery action>

  The details section is omitted when there is nothing useful to add. Runtime
  failures preserve their technical cause, point to a concrete next step, and
  never write partial success data to stdout. Text output is UTF-8.

COLOR CONTRACT
  --color auto styles terminal output and keeps redirected/piped output plain.
  --color always forces ANSI escape sequences; --color never suppresses them.
  Auto mode also disables color when NO_COLOR is set, CLICOLOR=0, or TERM=dumb.
  JSON output is always ANSI-free. Scripts should use auto/never unless ANSI
  styling is explicitly desired for another output format.

SUCCESSFUL STDOUT
  new                 Absolute path of the created task, one line
  list                Markdown table, filenames, or JSON document
  next                ID of the first ready task, one line
  id                  ID extracted from a task filename/path, one line
  done / reopen / set Absolute resulting path, one line
  view                Markdown document, or basename with --filename
  remove              `removed <path>`
  check               `OK: <N> not-done, <N> done task(s)`
  guide / reference   Documentation (styled on terminals, plain when redirected)

FAILURE AND MUTATION ORDER
  `list` validates before writing stdout, so a validation failure emits no partial
  table or JSON document. Mutation commands validate before their intended
  rename/move and reject existing destinations. Success output is written only
  after the requested filesystem operation completes. No crash-consistency or
  power-loss transaction guarantee is promised; keep task records in version control.
  Always check the exit status before consuming stdout.

PARSEABLE CHOICES
  Use `tat next` when only the first ready ID is needed. Use `tat id <PATH>` to
  convert a path returned by `tat new` into an ID without parsing punctuation.
  Prefer `tat list <MODE> --json` for structured metadata and relationships.
  Prefer `--filenames` when exact filenames alone are required. Use
  `tat view <ID> --filename` for one exact basename.

  JSON is a single UTF-8 document with `schema_version: 4`, the canonical list
  mode, repository name, and selected tasks. Each task includes ID, title,
  Markdown body, nullable `created_at` and derived `completed_at` UTC timestamps,
  the complete `completions` history,
  numeric and display priority, type, status, filename, direct
  parent/children and blocks/blocked_by relationships, and current direct or
  inherited `blocking_causes`. Relationship references include ID, title, status,
  and display priority.
  `blocked_by` is the direct inverse of stored blocks edges, including done
  records; `blocking_causes` separately explains which not-done edges currently
  make a task blocked. Consumers must reject unsupported schema versions.

  Filenames contain spaces and punctuation. Read output by line; do not split a
  filename on spaces. Task IDs are the text before the first comma in a basename.

  Markdown table Status values are ready, blocked, or done. Table cells escape `|`
  as `\\|`, double literal backslashes, and render line breaks as `<br>`. Treat
  Markdown tables as human/agent-readable output rather than a machine schema.

STDIN
  `tat new` reads stdin to EOF when stdin is redirected and neither --body nor
  --body-file is supplied. In automation, pass one of those options explicitly;
  use `--body ""` when an intentionally empty body is required. This avoids waiting
  on an inherited pipe that has not been closed.

POWERSHELL EXAMPLES
  $path = tat new 'Automatable task'
  if ($LASTEXITCODE -ne 0) { throw 'task creation failed' }
  $createdId = tat id $path

  $nextId = tat next
  if ($LASTEXITCODE -eq 0) { tat view $nextId }

  tat check 1>validation.txt 2>validation-error.txt
  if ($LASTEXITCODE -ne 0) { Get-Content validation-error.txt }

DISCOVERY
  tat uses the nearest ancestor containing a .git marker and never contacts a
  server. Run repository commands from the intended worktree. `tat id` is the
  exception: it parses a supplied basename and does not discover a repository.
  Outputs and generated IDs are repository-local."#;

const GUIDE_RECIPES: &str = r#"COMMON RECIPES

START A SIMPLE BACKLOG INSIDE AN EXISTING GIT REPOSITORY
  # tat does not create the Git repository. The first new task initializes
  # tasks/ and tasks/done/ beneath the nearest .git marker.
  tat new "Investigate startup latency" --type task --priority 3
  tat new "Fix login crash" --type bug --priority 1
  tat list

CREATE A PARENT WITH CHILDREN
  $parentPath = tat new 'Ship search feature' --type feature
  $parentId = tat id $parentPath
  tat new 'Implement search index' --parent $parentId
  tat new 'Add search UI' --parent $parentId

CREATE A GATE THAT PREVENTS WORK
  $workPath = tat new 'Perform the security-sensitive work'
  $workId = tat id $workPath
  $gatePath = tat new 'Wait for security approval' --type gate --blocks $workId
  $gateId = tat id $gatePath
  tat list blocked
  # After approval and verification:
  tat done $gateId
  tat list ready

TAKE THE NEXT READY TASK
  tat check
  $taskId = tat next              Lowest numeric priority, then ID
  tat view $taskId

REVISE SCOPE WITHOUT LOSING IDENTITY
  tat set t@abcd --description 'Handle transient and timeout errors' -p 2
  tat set t@abcd --body-file .\\updated-criteria.md
  tat view t@abcd

FINISH A TASK TREE
  tat list not-done
  tat done t@c001 --completion-notes 'Implemented the leaf; focused tests pass.'
  tat done t@p001 --completion-notes 'Integrated children; full feature test passes.'
  tat done t@r001 --completion-notes 'Released and verified the complete task tree.'

REOPEN DONE WORK
  tat list done
  tat view t@abcd
  tat reopen t@abcd

EXPORT DATA
  tat list all --markdown > tasks.md
  tat list all --json > tasks.json
  tat list ready --filenames > ready-task-files.txt
  tatviewer --all --output-path .\task-dashboard.html

DELETE ONLY WHEN HISTORY IS UNWANTED
  tat list blocked                 Record readiness before deletion
  tatviewer --all                  Check incoming and outgoing relationships
  tat view t@abcd
  tat remove t@abcd --force
  tat list ready                   Confirm the intended readiness change

CHECK AFTER A CLEAN MERGE
  tat check
  tat list all
  tatviewer --all
  If validation fails, stop normal task operations and follow the ordered recovery
  procedure in `tat reference`."#;

const GUIDE_TROUBLESHOOTING: &str = r#"TROUBLESHOOTING

"the current directory is not inside a Git repository"
  Run from the intended repository or a descendant. The repository root must have
  a .git directory or .git worktree file.

RECOVER AN ALREADY-INVALID STORE
  Normal task commands intentionally fail closed when any existing record is
  malformed, duplicated, missing a reference, or cyclic. In that state, `view`,
  `set`, `new`, `list`, and `tatviewer` cannot perform the repair. Use this exception
  to the no-manual-renames rule:

  1. Preserve recovery options: commit/stash current work or copy tasks/ somewhere
     safe. Do not delete the only copy of a task body.
  2. Run `tat check` and record the complete diagnostic.
  3. Inspect only Markdown files directly in tasks/ and tasks/done/. Compare their
     basenames with this reference's TASK MODEL section; relationships live in
     filenames, bodies do not.
  4. Make the smallest manual rename, restoration, or temporary move needed to
     correct the reported condition. Preserve valid IDs and body contents.
  5. Run `tat check` immediately. Repeat one diagnostic at a time.
  6. As soon as validation succeeds, return to `tat set`/`new` for further edits
     and review the repository diff before committing.

"task filename does not follow the required format"
  A Markdown file directly in tasks/ or tasks/done/ was manually named or damaged.
  Compare it with this reference's TASK MODEL section. Recover the intended ID and
  metadata, ensure
  optional parent/blocks fields are omitted rather than empty, make one careful
  manual filename repair, then run `tat check`. Do not alter the body.

"duplicate task ID"
  Two not-done/done files share an ID, often after a merge or manual copy.
  Decide whether they represent one task. If they do, preserve/merge the intended
  body and keep one canonical record. If they are separate tasks, temporarily move
  one conflicting file outside tasks/ and tasks/done/, run `tat check`, then
  recreate that work with `tat new --body-file <moved-file>` so tat generates its
  new ID. Never
  invent a replacement ID or discard the recovery copy before verification.

"references missing parent" / "references missing blocked task"
  The referenced record is absent from both tasks/ and tasks/done/. Restore it from
  version-control history when possible. If the relationship itself is wrong and
  restoration is impossible, manually rename only the referrer's filename to remove
  or correct that one parent/blocks ID, then run `tat check`. Once valid, use `tat set`.

"dependency cycle(s) detected"
  Follow the printed ID path and inspect those filenames; `tatviewer` also fails
  closed until the cycle is gone. Parent and blocks edges participate in the same
  graph. Manually remove or correct one demonstrably wrong relationship in one
  filename, run `tat check`, then use `tatviewer` to confirm the repaired graph.

"is blocked and cannot be completed"
  Run `tat list blocked` and `tatviewer`. Complete or change every not-done blocker;
  if the blocked task's parent is blocked, resolve the ancestor's blockers.

"has not-done descendants"
  Complete the deepest not-done children first. `tat list not-done` and `tatviewer`
  show the remaining hierarchy.

"already done" / "already not-done"
  Use `tat reopen` only for done tasks and `tat done` only for not-done tasks. Confirm
  status with `tat view <ID>` or `tat list all`.

"still referenced and cannot be removed"
  The diagnostic lists referring IDs. Prefer keeping history. If deletion is truly
  intended, inspect each referrer and explicitly clear/change its relationships.
  Also inspect the target's outgoing blocks edges: those do not prevent removal,
  but deleting the blocker can immediately make its targets and descendants ready.

"no changes were requested"
  `tat set` received no effective update. Supply a changed metadata/body option;
  use `tat view` if the intent was inspection.

"destination already exists"
  A conflicting file already occupies the intended not-done/done path. Do not
  overwrite it. Inspect both records and resolve the collision deliberately.

DESCRIPTION OR PRIORITY REJECTED
  Priorities must be finite numbers from 0 through 9. Descriptions must be nonempty,
  cannot end with a period/space, and cannot contain < > : " / \\ | ? *.

MORE HELP
  tat help <COMMAND>       Exact syntax, behavior, conflicts, and examples
  tat guide                Orientation and recommended workflow (How To)
  tat reference            The reference manual"#;

#[derive(Parser, Debug)]
#[command(
    name = "tat",
    version,
    about = TOP_LEVEL_ABOUT,
    long_about = TOP_LEVEL_ABOUT,
    after_help = TOP_LEVEL_HELP,
    disable_help_flag = true,
    arg_required_else_help = true,
    subcommand_required = true,
    propagate_version = true,
    term_width = 0
)]
struct Cli {
    /// Control ANSI colors:
    #[arg(long, global = true, value_enum, default_value = "auto")]
    color: ColorWhen,
    /// Print help
    #[arg(short = 'h', long = "help", global = true, action = ArgAction::Help)]
    help: Option<bool>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a new task.
    #[command(after_help = NEW_HELP)]
    New(NewArgs),
    /// List tasks, defaulting to tasks that are ready to execute.
    #[command(visible_alias = "ls", after_help = LIST_HELP)]
    List(ListArgs),
    /// Print the ID of the first ready task.
    #[command(after_help = NEXT_HELP)]
    Next,
    /// Extract a task ID from a task filename or path.
    #[command(after_help = ID_HELP)]
    Id(TaskFileArgs),
    /// Complete a not-done task.
    #[command(visible_aliases = ["complete", "close"], after_help = DONE_HELP)]
    Done(DoneArgs),
    /// Change a task while preserving its ID and storage location.
    #[command(
        visible_aliases = ["edit", "update", "change"],
        after_help = SET_HELP
    )]
    Set(SetArgs),
    /// Reopen a done task.
    #[command(visible_alias = "open", after_help = REOPEN_HELP)]
    Reopen(IdArgs),
    /// View one task.
    #[command(visible_alias = "show", after_help = VIEW_HELP)]
    View(ViewArgs),
    /// Permanently remove an unreferenced task.
    #[command(
        visible_aliases = ["delete", "rm"],
        after_help = REMOVE_HELP
    )]
    Remove(RemoveArgs),
    /// Validate filenames, references, IDs, priorities, and dependency cycles.
    #[command(visible_alias = "validate", after_help = CHECK_HELP)]
    Check,
    /// Orientation and recommended workflow (How To).
    #[command(after_help = GUIDE_HELP)]
    Guide,
    /// The reference manual.
    #[command(after_help = REFERENCE_HELP)]
    Reference,
}

#[derive(Args, Debug)]
struct NewArgs {
    /// Short task description. Multiple unquoted words are joined with spaces.
    #[arg(required = true, num_args = 1..)]
    description: Vec<String>,
    /// Priority from 0 through 9; decimals are allowed. Lower numbers run first.
    #[arg(short = 'p', long, default_value = "5")]
    priority: String,
    /// Task type.
    #[arg(short = 't', long = "type", value_enum, default_value = "task")]
    kind: KindArg,
    /// Existing parent task ID.
    #[arg(long)]
    parent: Option<String>,
    /// Existing task IDs blocked by this task; separate with commas, hyphens, or spaces.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    blocks: Vec<String>,
    /// Markdown body text. If omitted, piped stdin is used when available.
    #[arg(long, conflicts_with = "body_file")]
    body: Option<String>,
    /// Read the Markdown body from a file.
    #[arg(long, value_name = "PATH", conflicts_with = "body")]
    body_file: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ListMode {
    #[value(aliases = ["unblocked", "active"])]
    Ready,
    Blocked,
    NotDone,
    Done,
    All,
}

impl ListMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Blocked => "blocked",
            Self::NotDone => "not-done",
            Self::Done => "done",
            Self::All => "all",
        }
    }
}

#[derive(Args, Debug)]
struct ListArgs {
    /// Which tasks to list. "unblocked" and "active" alias "ready".
    #[arg(
        value_enum,
        default_value = "ready",
        hide_possible_values = true,
        long_help = "Which tasks to list: ready (aliases: unblocked, active), blocked, not-done, done, or all.\nSee MODES below for exact semantics."
    )]
    mode: ListMode,
    /// Emit raw filenames instead of a Markdown table.
    #[arg(long, visible_alias = "filenames-output", conflicts_with = "json")]
    filenames: bool,
    /// Explicitly request Markdown output (the default).
    #[arg(
        long,
        alias = "Markdown",
        visible_alias = "output-markdown",
        conflicts_with_all = ["filenames", "json"]
    )]
    markdown: bool,
    /// Emit a versioned JSON document with resolved relationships.
    #[arg(long, conflicts_with_all = ["filenames", "markdown"])]
    json: bool,
}

#[derive(Args, Debug)]
struct IdArgs {
    /// Task ID.
    task_id: String,
}

#[derive(Args, Debug)]
struct DoneArgs {
    /// Task ID.
    task_id: String,
    /// Append Markdown completion notes before marking the task done.
    #[arg(
        long,
        visible_aliases = ["done-notes", "notes"],
        conflicts_with = "completion_notes_file"
    )]
    completion_notes: Option<String>,
    /// Append Markdown completion notes read from a file.
    #[arg(
        long,
        value_name = "PATH",
        visible_aliases = ["done-notes-file", "notes-file"],
        conflicts_with = "completion_notes"
    )]
    completion_notes_file: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct TaskFileArgs {
    /// Canonical task filename or path whose basename is a task filename.
    #[arg(value_name = "PATH_OR_FILENAME")]
    path: PathBuf,
}

#[derive(Args, Debug)]
struct SetArgs {
    /// Task ID.
    task_id: String,
    /// Replace the description.
    #[arg(long)]
    description: Option<String>,
    /// Replace the priority (0 through 9; decimals are allowed).
    #[arg(short = 'p', long)]
    priority: Option<String>,
    /// Replace the task type.
    #[arg(short = 't', long = "type", value_enum)]
    kind: Option<KindArg>,
    /// Replace the parent task ID.
    #[arg(long, conflicts_with = "clear_parent")]
    parent: Option<String>,
    /// Remove the parent relationship.
    #[arg(long)]
    clear_parent: bool,
    /// Replace all blocked-task IDs; separate with commas, hyphens, or spaces.
    #[arg(
        long,
        value_delimiter = ',',
        num_args = 1..,
        conflicts_with_all = ["clear_blocks", "add_blocks", "remove_blocks"]
    )]
    blocks: Option<Vec<String>>,
    /// Remove every blocked-task relationship.
    #[arg(long, conflicts_with_all = ["blocks", "add_blocks", "remove_blocks"])]
    clear_blocks: bool,
    /// Add blocked-task IDs; separate with commas, hyphens, or spaces.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    add_blocks: Vec<String>,
    /// Remove blocked-task IDs; separate with commas, hyphens, or spaces.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    remove_blocks: Vec<String>,
    /// Replace the Markdown body text.
    #[arg(long, conflicts_with = "body_file")]
    body: Option<String>,
    /// Replace the Markdown body from a file.
    #[arg(long, value_name = "PATH", conflicts_with = "body")]
    body_file: Option<PathBuf>,
}

#[derive(Args, Debug)]
struct ViewArgs {
    /// Task ID.
    task_id: String,
    /// Emit only the raw filename.
    #[arg(long)]
    filename: bool,
}

#[derive(Args, Debug)]
struct RemoveArgs {
    /// Task ID.
    task_id: String,
    /// Confirm permanent deletion.
    #[arg(long)]
    force: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ColorWhen {
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum KindArg {
    Bug,
    Feature,
    Task,
    Gate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskKind {
    Bug,
    Feature,
    Task,
    Gate,
}

impl TaskKind {
    fn parse(value: &str) -> AppResult<Self> {
        match value.to_ascii_uppercase().as_str() {
            "BUG" => Ok(Self::Bug),
            "FEATURE" => Ok(Self::Feature),
            "TASK" => Ok(Self::Task),
            "GATE" => Ok(Self::Gate),
            _ => Err(format!("unsupported task type '{value}'")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Bug => "BUG",
            Self::Feature => "FEATURE",
            Self::Task => "TASK",
            Self::Gate => "GATE",
        }
    }
}

impl From<KindArg> for TaskKind {
    fn from(value: KindArg) -> Self {
        match value {
            KindArg::Bug => Self::Bug,
            KindArg::Feature => Self::Feature,
            KindArg::Task => Self::Task,
            KindArg::Gate => Self::Gate,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskState {
    NotDone,
    Done,
}

#[derive(Clone, Debug)]
struct Task {
    id: String,
    parent: Option<String>,
    blocks: Vec<String>,
    priority: f64,
    priority_text: String,
    kind: TaskKind,
    description: String,
    body_markdown: String,
    metadata: Option<TaskMetadata>,
    created_at: Option<String>,
    completions: Vec<String>,
    path: PathBuf,
    state: TaskState,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct TaskMetadata {
    version: u32,
    created_at: Option<String>,
    #[serde(default)]
    completions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct TaskMetadataV1 {
    version: u32,
    created_at_unix_ms: u64,
    completed_at_unix_ms: Option<u64>,
    #[serde(default)]
    completion_history_unix_ms: Vec<u64>,
}

#[derive(Clone, Debug)]
struct Repository {
    root: PathBuf,
    tasks_dir: PathBuf,
    done_dir: PathBuf,
    tasks: Vec<Task>,
}

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().collect::<Vec<_>>();
    if arguments.len() == 2 && (arguments[1] == "help" || arguments[1] == "--help") {
        arguments[1] = OsString::from("-h");
    }
    let requested_color = requested_color(&arguments);
    let matches = match Cli::command().try_get_matches_from(arguments) {
        Ok(matches) => matches,
        Err(error) => {
            let use_stderr = error.use_stderr();
            let color = color_enabled(
                requested_color,
                if use_stderr {
                    io::stderr().is_terminal()
                } else {
                    io::stdout().is_terminal()
                },
            );
            let rendered = colorize_document(&error.to_string(), color);
            if use_stderr {
                eprint!("{rendered}");
            } else {
                print!("{rendered}");
            }
            return ExitCode::from(error.exit_code() as u8);
        }
    };
    let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    configure_color(cli.color);
    let command = command_name(&cli.command);
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            print_runtime_diagnostic(command, &error);
            ExitCode::FAILURE
        }
    }
}

fn command_name(command: &Commands) -> &'static str {
    match command {
        Commands::New(_) => "new",
        Commands::List(_) => "list",
        Commands::Next => "next",
        Commands::Id(_) => "id",
        Commands::Done(_) => "done",
        Commands::Set(_) => "set",
        Commands::Reopen(_) => "reopen",
        Commands::View(_) => "view",
        Commands::Remove(_) => "remove",
        Commands::Check => "check",
        Commands::Guide => "guide",
        Commands::Reference => "reference",
    }
}

fn run(cli: Cli) -> AppResult<()> {
    match cli.command {
        Commands::New(args) => command_new(args),
        Commands::List(args) => command_list(args),
        Commands::Next => command_next(),
        Commands::Id(args) => command_id(args),
        Commands::Done(args) => command_done(args),
        Commands::Set(args) => command_set(args),
        Commands::Reopen(args) => command_reopen(args),
        Commands::View(args) => command_view(args),
        Commands::Remove(args) => command_remove(args),
        Commands::Check => command_check(),
        Commands::Guide => command_guide(),
        Commands::Reference => command_reference(),
    }
}

fn print_runtime_diagnostic(command: &str, raw: &str) {
    let diagnostic = runtime_diagnostic(command, raw);
    eprintln!(
        "{}: {}",
        paint_stderr("error", ANSI_ERROR),
        diagnostic.summary
    );
    eprintln!();
    eprintln!("{}", diagnostic.explanation);
    if let Some(details) = diagnostic.details {
        eprintln!();
        eprintln!("{}", paint_stderr("details:", ANSI_DIM));
        eprintln!("{details}");
    }
    eprintln!();
    eprintln!(
        "{}: {}",
        paint_stderr("try", ANSI_WARNING),
        diagnostic.action
    );
}

fn runtime_diagnostic(command: &str, raw: &str) -> RuntimeDiagnostic {
    let detail = || Some(raw.to_owned());
    if let Some(cycles_and_repairs) =
        raw.strip_prefix("requested change would create a dependency cycle:\n")
    {
        let (cycles_and_repairs, retry) = cycles_and_repairs
            .split_once("\nretry command:\n")
            .map_or((cycles_and_repairs, None), |(body, retry)| {
                (body, Some(retry))
            });
        let (cycles, repairs) = cycles_and_repairs
            .split_once("\nrepair commands:\n")
            .unwrap_or((cycles_and_repairs, ""));
        let mut action = if repairs.is_empty() {
            "Run 'tatviewer' to inspect the current relationships and remove one existing parent/blocks edge with 'tat set'. Then rerun the same command from shell history; tat does not repeat body text in diagnostics.".to_owned()
        } else {
            format!(
                "Run 'tatviewer' to inspect the current relationships. Choose one existing cycle edge below and run its exact command(s):\n{}",
                indent_repair_lines(repairs)
            )
        };
        if let Some(retry) = retry {
            action.push_str(&format!("\n  After repairing the graph, run '{retry}'."));
        } else if !repairs.is_empty() {
            action.push_str("\n  After repairing the graph, rerun the same command from shell history; tat does not repeat body text in diagnostics.");
        }
        return RuntimeDiagnostic {
            summary: "The requested change creates a dependency cycle.".to_owned(),
            explanation: "tat rejected the change because it would leave not-done tasks waiting on one another. A cycle has no valid starting task, so tat cannot determine a safe execution order. No task files were changed.".to_owned(),
            details: Some(format!("Cycle:\n{}", indent_cycle_lines(cycles))),
            action,
        };
    }
    if let Some(cycles) = raw.strip_prefix("dependency cycle(s) detected among not-done tasks:\n") {
        return RuntimeDiagnostic {
            summary: "The task graph contains a dependency cycle.".to_owned(),
            explanation: "At least one not-done task ultimately depends on itself. There is no safe first task in that loop, so tat stops before reading task data or making changes.".to_owned(),
            details: Some(format!("Cycle:\n{}", indent_cycle_lines(cycles))),
            action: "Follow the invalid-store recovery steps in 'tat reference', repair one incorrect parent/blocks edge, and run 'tat check' again.".to_owned(),
        };
    }
    if raw == "the current directory is not inside a Git repository" {
        return RuntimeDiagnostic {
            summary: "No Git repository was found.".to_owned(),
            explanation: "tat searches the current directory and then walks upward for a .git directory or worktree file. It found neither, so it does not know where this repository's task store belongs.".to_owned(),
            details: detail(),
            action: "Change to the intended Git repository (or one of its subdirectories) and run the command again.".to_owned(),
        };
    }
    if raw.starts_with("the tasks path is a file:")
        || raw.starts_with("the done-tasks path is a file:")
    {
        return RuntimeDiagnostic {
            summary: "The task-store layout is invalid.".to_owned(),
            explanation: "tat needs tasks/ and tasks/done/ to be directories, but one of those paths is occupied by a regular file. It will not replace or overwrite that file automatically.".to_owned(),
            details: detail(),
            action: "Inspect the reported path, move or rename the conflicting file deliberately, and run 'tat check' again.".to_owned(),
        };
    }
    if raw.starts_with("duplicate task ID '") {
        return RuntimeDiagnostic {
            summary: "The task store contains a duplicate ID.".to_owned(),
            explanation: "Two Markdown records claim the same task identity. tat cannot safely know which record is authoritative, so every task operation stops until the conflict is resolved.".to_owned(),
            details: detail(),
            action: "Preserve both files, follow duplicate-ID recovery in 'tat reference', then run 'tat check'.".to_owned(),
        };
    }
    if raw.contains("references missing parent") || raw.contains("references missing blocked task")
    {
        return RuntimeDiagnostic {
            summary: "The task graph contains a broken reference.".to_owned(),
            explanation: "A task names a parent or blocked task that is absent from both tasks/ and tasks/done/. tat stops because readiness and dependency results would otherwise be misleading.".to_owned(),
            details: detail(),
            action: "Restore the missing task or follow broken-reference recovery in 'tat reference', then run 'tat check'.".to_owned(),
        };
    }
    if raw.starts_with("task filename does not follow the required format:")
        || raw.starts_with("task filename is not valid UTF-8:")
    {
        return RuntimeDiagnostic {
            summary: "A task filename is invalid.".to_owned(),
            explanation: "tat stores task identity and metadata in Markdown filenames. This file cannot be parsed reliably, so tat stops instead of guessing what the record means.".to_owned(),
            details: detail(),
            action: "Compare the filename with the TASK MODEL in 'tat reference', make the smallest safe recovery edit, and run 'tat check'.".to_owned(),
        };
    }
    if raw == "no ready tasks are available" {
        return RuntimeDiagnostic {
            summary: "No ready task is available.".to_owned(),
            explanation: "The repository has no not-done task that can start now. Either all remaining work is blocked, or every task is already done.".to_owned(),
            details: detail(),
            action: "Run 'tat list blocked' to inspect waiting work and 'tat list not-done' to see whether any unfinished tasks remain.".to_owned(),
        };
    }
    if raw.starts_with("task '") && raw.ends_with(" does not exist") {
        let id = first_quoted_value(raw).unwrap_or("requested task");
        return RuntimeDiagnostic {
            summary: format!("Task {id} does not exist."),
            explanation: "tat searched both not-done and done task records but found no matching ID. The ID may be mistyped, or the task may have been removed.".to_owned(),
            details: detail(),
            action: format!("Run 'tat list all' and 'git log --all --name-status -- tasks/ tasks/done/'. If neither output contains {id}, stop and verify the ID with the user. If Git history contains it, show that evidence and ask before restoring anything; never invent a replacement ID."),
        };
    }
    if raw.starts_with("task '") && raw.ends_with(" is already done") {
        let id = first_quoted_value(raw).unwrap_or("requested task");
        return RuntimeDiagnostic {
            summary: format!("Task {id} is already done."),
            explanation: "The task record is already stored under tasks/done/, so completing it again would not change anything.".to_owned(),
            details: detail(),
            action: format!("Run 'tat view {id}' to inspect it, or 'tat reopen {id}' if the work must become not-done again."),
        };
    }
    if raw.starts_with("task '") && raw.ends_with(" is already not-done") {
        let id = first_quoted_value(raw).unwrap_or("requested task");
        return RuntimeDiagnostic {
            summary: format!("Task {id} is already not-done."),
            explanation: "The task is already in tasks/ with status ready or blocked, so reopening it would not change its storage or status.".to_owned(),
            details: detail(),
            action: format!("Run 'tat view {id}' to inspect its current status, then choose the operation you actually need."),
        };
    }
    if raw.starts_with("task '") && raw.contains(" is blocked and cannot be completed") {
        let id = first_quoted_value(raw).unwrap_or("requested task");
        return RuntimeDiagnostic {
            summary: format!("Task {id} is blocked."),
            explanation: "One or more not-done tasks still prevent this task (or one of its parents) from proceeding. Completing it now would record work as finished before its prerequisites are satisfied.".to_owned(),
            details: detail(),
            action: blocked_task_recovery(raw, id),
        };
    }
    if raw.starts_with("task '") && raw.contains(" has not-done descendants") {
        let id = first_quoted_value(raw).unwrap_or("requested task");
        return RuntimeDiagnostic {
            summary: format!("Task {id} still has unfinished descendants."),
            explanation: "A parent cannot be completed while any child below it remains not-done. Finishing the parent first would leave active work under a completed container.".to_owned(),
            details: detail(),
            action: "Complete the listed descendants from the deepest level upward, then retry the parent.".to_owned(),
        };
    }
    if raw == "no changes were requested" {
        return RuntimeDiagnostic {
            summary: "No task changes were requested.".to_owned(),
            explanation: "The set command did not receive an option that would alter metadata, relationships, or the Markdown body, so tat left the task untouched.".to_owned(),
            details: detail(),
            action: "Run 'tat help set', supply the intended change option, and retry; use 'tat view <ID>' if you only meant to inspect the task.".to_owned(),
        };
    }
    if raw.starts_with("remove of task '")
        && raw.ends_with(" is permanent; pass --force to confirm")
    {
        let id = first_quoted_value(raw).unwrap_or("<ID>");
        return RuntimeDiagnostic {
            summary: "Permanent deletion was not confirmed.".to_owned(),
            explanation: "remove destroys the task record instead of preserving it under tasks/done/. tat requires an explicit confirmation so an accidental command cannot erase task history.".to_owned(),
            details: detail(),
            action: format!("Run 'tat view {id}' to confirm the exact task. Use 'tat done {id}' to preserve history, or run 'tat remove {id} --force' only when permanent deletion is intentional."),
        };
    }
    if raw.starts_with("task '") && raw.contains(" is still referenced and cannot be removed") {
        let id = first_quoted_value(raw).unwrap_or("requested task");
        return RuntimeDiagnostic {
            summary: format!("Task {id} is still referenced."),
            explanation: "Other task records still name this task as a parent or dependency. Deleting it would leave broken relationships, so tat refused the removal.".to_owned(),
            details: detail(),
            action: referenced_task_recovery(raw, id),
        };
    }
    if raw.starts_with('\'') && raw.contains(" is not a valid task ID") {
        return RuntimeDiagnostic {
            summary: "The task ID is invalid.".to_owned(),
            explanation: "tat accepts IDs that begin with t@ followed by at least four lowercase letters or digits. The supplied value does not match that shape.".to_owned(),
            details: detail(),
            action: "Copy an ID from 'tat list all' instead of inventing or retyping one, then retry.".to_owned(),
        };
    }
    if raw == "task ID list must contain at least one ID" {
        return RuntimeDiagnostic {
            summary: "No task IDs were provided.".to_owned(),
            explanation: "This relationship option requires at least one existing task ID, but the supplied list contained only separators or whitespace.".to_owned(),
            details: detail(),
            action: "Copy one or more IDs from 'tat list all', or use the matching --clear option when the intent is to remove the relationship.".to_owned(),
        };
    }
    if raw.starts_with("description ") {
        return RuntimeDiagnostic {
            summary: "The task description is invalid.".to_owned(),
            explanation: "Descriptions become part of Windows-compatible filenames. tat rejected this value because it would be empty, end unsafely, or contain a character the filesystem cannot store reliably.".to_owned(),
            details: detail(),
            action: "Use a short nonempty description without a trailing period/space or any of < > : \" / \\ | ? *, then retry.".to_owned(),
        };
    }
    if raw.starts_with("priority ") {
        return RuntimeDiagnostic {
            summary: "The task priority is invalid.".to_owned(),
            explanation: "Priorities must be finite numbers from 0 through 9; lower numbers are listed first, and decimals such as 2.5 are allowed.".to_owned(),
            details: detail(),
            action: "Choose a numeric priority between 0 and 9 and retry.".to_owned(),
        };
    }
    if raw.starts_with("parent '") || raw.starts_with("blocked task '") {
        return RuntimeDiagnostic {
            summary: "A referenced task does not exist.".to_owned(),
            explanation: "The requested relationship points to an ID that is absent from both not-done and done records. tat will not create a dangling parent or dependency edge.".to_owned(),
            details: detail(),
            action: "Run 'tat list all', copy the intended existing ID, and retry the relationship change.".to_owned(),
        };
    }
    if raw.starts_with("destination already exists:") {
        return RuntimeDiagnostic {
            summary: "The destination task file already exists.".to_owned(),
            explanation: "tat will not overwrite an existing record while moving or renaming a task because doing so could destroy another task's body or metadata.".to_owned(),
            details: detail(),
            action: "Inspect both files, resolve the collision deliberately, run 'tat check', and retry.".to_owned(),
        };
    }
    if raw.starts_with("cannot ") || raw.starts_with("path has no UTF-8 filename:") {
        return RuntimeDiagnostic {
            summary: "A filesystem operation failed.".to_owned(),
            explanation: format!("tat could not finish the '{command}' command because Windows rejected or could not complete a required file operation. The operating-system detail below identifies the exact path and cause."),
            details: detail(),
            action: "Check the path, permissions, free space, and file locks; then run 'tat check' before retrying the command.".to_owned(),
        };
    }

    RuntimeDiagnostic {
        summary: "The command could not be completed.".to_owned(),
        explanation: format!(
            "tat stopped while running '{command}'. The technical detail below is preserved exactly; no success output was produced."
        ),
        details: detail(),
        action: format!(
            "Run 'tat help {command}' for command-specific guidance or 'tat reference' for recovery steps, then retry."
        ),
    }
}

fn indent_cycle_lines(cycles: &str) -> String {
    cycles
        .lines()
        .map(|line| format!("  {}", line.trim_start_matches([' ', '-'])))
        .collect::<Vec<_>>()
        .join("\n")
}

fn indent_repair_lines(repairs: &str) -> String {
    repairs
        .lines()
        .map(|line| format!("  {}", line.trim_start_matches([' ', '-'])))
        .collect::<Vec<_>>()
        .join("\n")
}

fn first_quoted_value(value: &str) -> Option<&str> {
    value.split('\'').nth(1)
}

fn blocked_task_recovery(raw: &str, target: &str) -> String {
    let blocker_and_edge_target = raw.lines().skip(1).find_map(|line| {
        let mut parts = line.split('\'');
        parts.next()?;
        let blocker = parts.next()?;
        parts.next()?;
        let edge_target = parts.next()?;
        Some((blocker, edge_target))
    });
    if let Some((blocker, edge_target)) = blocker_and_edge_target {
        return format!(
            "Run 'tat view {blocker}' and 'tatviewer'. If the prerequisite is finished, run 'tat done {blocker}' and then 'tat done {target}'. If the blocks edge is wrong, remove it with 'tat set {blocker} --remove-blocks {edge_target}'."
        );
    }
    format!(
        "Run 'tat list blocked' and 'tatviewer' to identify the prerequisite, complete or correct it, then retry 'tat done {target}'."
    )
}

fn referenced_task_recovery(raw: &str, target: &str) -> String {
    let first_reference = raw
        .lines()
        .skip(1)
        .map(|line| line.trim_start_matches([' ', '-']))
        .find(|line| !line.is_empty());
    if let Some(reference) = first_reference {
        let referrer = reference.split_whitespace().next().unwrap_or("<REFERRER>");
        if reference.contains("blocks field") {
            return format!(
                "Run 'tat view {referrer}', then remove that edge with 'tat set {referrer} --remove-blocks {target}'. Repair every listed referrer, run 'tat check', and run 'tat remove {target} --force' only if deletion is still intended."
            );
        }
        if reference.contains("as parent") {
            return format!(
                "Run 'tat view {referrer}', then remove that edge with 'tat set {referrer} --clear-parent'. Repair every listed referrer, run 'tat check', and run 'tat remove {target} --force' only if deletion is still intended."
            );
        }
    }
    format!(
        "Inspect every listed referrer, repair its relationship deliberately with 'tat set', run 'tat check', and run 'tat remove {target} --force' only if deletion is still intended."
    )
}

fn validate_prospective(
    original: &Repository,
    prospective: &Repository,
    retry_command: Option<&str>,
) -> AppResult<()> {
    prospective.validate().map_err(|error| {
        error
            .strip_prefix("dependency cycle(s) detected among not-done tasks:\n")
            .map_or(error.clone(), |cycles| {
                let repairs = cycle_repair_commands(original, cycles);
                let mut message = if repairs.is_empty() {
                    format!("requested change would create a dependency cycle:\n{cycles}")
                } else {
                    format!(
                        "requested change would create a dependency cycle:\n{cycles}\nrepair commands:\n{repairs}"
                    )
                };
                if let Some(retry_command) = retry_command {
                    message.push_str(&format!("\nretry command:\n{retry_command}"));
                }
                message
            })
    })
}

fn set_retry_command(original: &Task, updated: &Task) -> Option<String> {
    let mut options = Vec::new();
    if original.description != updated.description {
        options.push(format!(
            "--description {}",
            powershell_quote(&updated.description)
        ));
    }
    if original.priority_text != updated.priority_text {
        options.push(format!("--priority {}", updated.priority_text));
    }
    if original.kind != updated.kind {
        options.push(format!(
            "--type {}",
            updated.kind.as_str().to_ascii_lowercase()
        ));
    }
    if original.parent != updated.parent {
        if let Some(parent) = &updated.parent {
            options.push(format!("--parent {parent}"));
        } else {
            options.push("--clear-parent".to_owned());
        }
    }
    if original.blocks != updated.blocks {
        if updated.blocks.is_empty() {
            options.push("--clear-blocks".to_owned());
        } else {
            options.push(format!("--blocks {}", updated.blocks.join("-")));
        }
    }
    (!options.is_empty()).then(|| format!("tat set {} {}", updated.id, options.join(" ")))
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn cycle_repair_commands(repository: &Repository, cycles: &str) -> String {
    let mut repairs = BTreeSet::new();
    for cycle in cycles.lines() {
        let nodes = cycle
            .trim_start_matches([' ', '-'])
            .split(" -> ")
            .collect::<Vec<_>>();
        for edge in nodes.windows(2) {
            let from = edge[0];
            let to = edge[1];
            let mut commands = Vec::new();
            if repository
                .by_id(from)
                .is_some_and(|task| task.blocks.iter().any(|blocked| blocked == to))
            {
                commands.push(format!("tat set {from} --remove-blocks {to}"));
            }
            if repository
                .by_id(to)
                .is_some_and(|task| task.parent.as_deref() == Some(from))
            {
                commands.push(format!("tat set {to} --clear-parent"));
            }
            if !commands.is_empty() {
                let instruction = if commands.len() == 1 {
                    format!("For edge {from} -> {to}, run '{}'", commands[0])
                } else {
                    format!(
                        "For edge {from} -> {to}, run both '{}' and '{}'",
                        commands[0], commands[1]
                    )
                };
                repairs.insert(format!(" - {instruction}"));
            }
        }
    }
    repairs.into_iter().collect::<Vec<_>>().join("\n")
}

impl Repository {
    fn discover(create_task_dirs: bool) -> AppResult<Self> {
        let current = std::env::current_dir()
            .map_err(|error| format!("cannot read the current directory: {error}"))?;
        let root = current
            .ancestors()
            .find(|candidate| candidate.join(".git").exists())
            .map(Path::to_path_buf)
            .ok_or_else(|| "the current directory is not inside a Git repository".to_owned())?;
        let tasks_dir = root.join("tasks");
        let done_dir = tasks_dir.join("done");
        if tasks_dir.is_file() {
            return Err(format!("the tasks path is a file: {}", tasks_dir.display()));
        }
        if done_dir.is_file() {
            return Err(format!(
                "the done-tasks path is a file: {}",
                done_dir.display()
            ));
        }
        if create_task_dirs {
            fs::create_dir_all(&done_dir)
                .map_err(|error| format!("cannot initialize task directories: {error}"))?;
        }

        let mut repository = Self {
            root,
            tasks_dir,
            done_dir,
            tasks: Vec::new(),
        };
        repository.load()?;
        Ok(repository)
    }

    fn load(&mut self) -> AppResult<()> {
        self.tasks.clear();
        if self.tasks_dir.is_dir() {
            self.tasks
                .extend(read_task_directory(&self.tasks_dir, TaskState::NotDone)?);
        }
        if self.done_dir.is_dir() {
            self.tasks
                .extend(read_task_directory(&self.done_dir, TaskState::Done)?);
        }
        Ok(())
    }

    fn validate(&self) -> AppResult<()> {
        let mut by_id: HashMap<&str, &Task> = HashMap::new();
        for task in &self.tasks {
            validate_task_metadata(task)?;
            if by_id.insert(&task.id, task).is_some() {
                return Err(format!(
                    "duplicate task ID '{}' appears in multiple files",
                    task.id
                ));
            }
        }
        for task in &self.tasks {
            if let Some(parent) = &task.parent
                && !by_id.contains_key(parent.as_str())
            {
                return Err(format!(
                    "task '{}' references missing parent '{parent}'",
                    task.id
                ));
            }
            for blocked in &task.blocks {
                if !by_id.contains_key(blocked.as_str()) {
                    return Err(format!(
                        "task '{}' references missing blocked task '{blocked}'",
                        task.id
                    ));
                }
            }
        }

        let cycles = self.dependency_cycles();
        if !cycles.is_empty() {
            let lines = cycles
                .iter()
                .map(|cycle| format!(" - {cycle}"))
                .collect::<Vec<_>>()
                .join("\n");
            return Err(format!(
                "dependency cycle(s) detected among not-done tasks:\n{lines}"
            ));
        }
        Ok(())
    }

    fn by_id(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    fn blocked_ids(&self) -> HashSet<String> {
        let state_by_id: HashMap<&str, TaskState> = self
            .tasks
            .iter()
            .map(|task| (task.id.as_str(), task.state))
            .collect();
        let mut blocked = HashSet::new();
        for task in self
            .tasks
            .iter()
            .filter(|task| task.state == TaskState::NotDone)
        {
            for target in &task.blocks {
                if state_by_id.get(target.as_str()) == Some(&TaskState::NotDone) {
                    blocked.insert(target.clone());
                }
            }
        }
        loop {
            let before = blocked.len();
            for task in self
                .tasks
                .iter()
                .filter(|task| task.state == TaskState::NotDone)
            {
                if task
                    .parent
                    .as_ref()
                    .is_some_and(|parent| blocked.contains(parent))
                {
                    blocked.insert(task.id.clone());
                }
            }
            if blocked.len() == before {
                break;
            }
        }
        blocked
    }

    fn blocking_reasons(&self, id: &str) -> Vec<String> {
        let mut reasons = Vec::new();
        let mut current = Some(id);
        while let Some(target) = current {
            for blocker in self.tasks.iter().filter(|task| {
                task.state == TaskState::NotDone
                    && task.blocks.iter().any(|blocked| blocked == target)
            }) {
                if target == id {
                    reasons.push(format!("task '{}' directly blocks '{id}'", blocker.id));
                } else {
                    reasons.push(format!(
                        "task '{}' blocks ancestor '{target}', and that block propagates to '{id}'",
                        blocker.id
                    ));
                }
            }
            current = self.by_id(target).and_then(|task| {
                task.parent.as_deref().filter(|parent| {
                    self.by_id(parent)
                        .is_some_and(|parent_task| parent_task.state == TaskState::NotDone)
                })
            });
        }
        reasons.sort();
        reasons.dedup();
        reasons
    }

    fn dependency_edges(&self) -> HashMap<String, BTreeSet<String>> {
        let not_done: HashSet<&str> = self
            .tasks
            .iter()
            .filter(|task| task.state == TaskState::NotDone)
            .map(|task| task.id.as_str())
            .collect();
        let mut edges: HashMap<String, BTreeSet<String>> = not_done
            .iter()
            .map(|id| ((*id).to_owned(), BTreeSet::new()))
            .collect();
        for task in self
            .tasks
            .iter()
            .filter(|task| task.state == TaskState::NotDone)
        {
            for target in &task.blocks {
                if not_done.contains(target.as_str()) {
                    edges
                        .entry(task.id.clone())
                        .or_default()
                        .insert(target.clone());
                }
            }
            if let Some(parent) = &task.parent
                && not_done.contains(parent.as_str())
            {
                edges
                    .entry(parent.clone())
                    .or_default()
                    .insert(task.id.clone());
            }
        }
        edges
    }

    fn dependency_cycles(&self) -> BTreeSet<String> {
        let edges = self.dependency_edges();
        let mut states: HashMap<String, u8> = HashMap::new();
        let mut stack = Vec::new();
        let mut cycles = BTreeSet::new();
        let mut nodes = edges.keys().cloned().collect::<Vec<_>>();
        nodes.sort();
        for node in nodes {
            if !states.contains_key(&node) {
                visit_dependency_node(&node, &edges, &mut states, &mut stack, &mut cycles);
            }
        }
        cycles
    }

    fn not_done_descendants(&self, root_id: &str) -> BTreeSet<String> {
        let mut ancestors = HashSet::from([root_id.to_owned()]);
        let mut descendants = BTreeSet::new();
        loop {
            let before = descendants.len();
            for task in self
                .tasks
                .iter()
                .filter(|task| task.state == TaskState::NotDone)
            {
                if task
                    .parent
                    .as_ref()
                    .is_some_and(|parent| ancestors.contains(parent))
                    && !ancestors.contains(&task.id)
                {
                    descendants.insert(task.id.clone());
                    ancestors.insert(task.id.clone());
                }
            }
            if descendants.len() == before {
                break;
            }
        }
        descendants
    }

    fn references_to(&self, id: &str) -> Vec<String> {
        let mut references = Vec::new();
        for task in &self.tasks {
            if task.id == id {
                continue;
            }
            if task.parent.as_deref() == Some(id) {
                references.push(format!("{} uses it as parent", task.id));
            }
            if task.blocks.iter().any(|blocked| blocked == id) {
                references.push(format!("{} names it in its blocks field", task.id));
            }
        }
        references.sort();
        references
    }
}

fn task_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"^(?P<id>t@[a-z0-9]{4,}), (?:parent-(?P<parent>t@[a-z0-9]{4,}), )?(?:blocks-(?P<blocks>t@[a-z0-9]{4,}(?:-t@[a-z0-9]{4,})*), )?p(?P<priority>[0-9](?:\.[0-9]+)?), (?P<kind>BUG|FEATURE|TASK|GATE), (?P<description>.+)\.md$",
        )
        .expect("task filename regex is valid")
    })
}

fn id_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^t@[a-z0-9]{4,}$").expect("task ID regex is valid"))
}

fn read_task_directory(directory: &Path, state: TaskState) -> AppResult<Vec<Task>> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot read '{}': {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot enumerate '{}': {error}", directory.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut tasks = Vec::new();
    for entry in entries {
        let path = entry.path();
        if !path.is_file() || path.extension() != Some(OsStr::new("md")) {
            continue;
        }
        tasks.push(parse_task_file(&path, state)?);
    }
    Ok(tasks)
}

fn parse_task_file(path: &Path, state: TaskState) -> AppResult<Task> {
    let filename = path
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("task filename is not valid UTF-8: {}", path.display()))?;
    let captures = task_regex().captures(filename).ok_or_else(|| {
        format!(
            "task filename does not follow the required format: {}",
            path.display()
        )
    })?;
    let priority_text = captures["priority"].to_owned();
    let priority = parse_priority(&priority_text)?.0;
    let parent = captures
        .name("parent")
        .map(|value| value.as_str().to_owned());
    let blocks = captures
        .name("blocks")
        .map(|value| value.as_str().split('-').map(str::to_owned).collect())
        .unwrap_or_default();
    let stored_text = fs::read_to_string(path)
        .map_err(|error| format!("cannot read task text '{}': {error}", path.display()))?;
    let (metadata, body_markdown) = parse_task_text(&stored_text)
        .map_err(|error| format!("invalid task metadata in '{}': {error}", path.display()))?;
    let created_at = metadata.as_ref().and_then(|value| value.created_at.clone());
    let completions = metadata
        .as_ref()
        .map_or_else(Vec::new, |value| value.completions.clone());
    Ok(Task {
        id: captures["id"].to_owned(),
        parent,
        blocks,
        priority,
        priority_text,
        kind: TaskKind::parse(&captures["kind"])?,
        description: captures["description"].to_owned(),
        body_markdown,
        metadata,
        created_at,
        completions,
        path: path.to_path_buf(),
        state,
    })
}

fn parse_task_text(stored_text: &str) -> AppResult<(Option<TaskMetadata>, String)> {
    let Some(after_prefix) = stored_text.strip_prefix(TASK_METADATA_PREFIX) else {
        return Ok((None, stored_text.to_owned()));
    };
    let (metadata_line, remainder) = after_prefix
        .split_once('\n')
        .ok_or_else(|| "metadata comment must end on its first line".to_owned())?;
    let metadata_json = metadata_line
        .trim_end_matches('\r')
        .strip_suffix(TASK_METADATA_SUFFIX)
        .ok_or_else(|| format!("metadata comment must end with '{TASK_METADATA_SUFFIX}'"))?;
    let value: serde_json::Value = serde_json::from_str(metadata_json)
        .map_err(|error| format!("metadata JSON is invalid: {error}"))?;
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| "metadata JSON requires an integer version".to_owned())?;
    let metadata = match version {
        1 => upgrade_v1_metadata(
            serde_json::from_value(value)
                .map_err(|error| format!("version 1 metadata JSON is invalid: {error}"))?,
        )?,
        version if version == u64::from(TASK_METADATA_VERSION) => serde_json::from_value(value)
            .map_err(|error| format!("version 2 metadata JSON is invalid: {error}"))?,
        _ => {
            return Err(format!(
                "unsupported metadata version {version}; expected 1 or {TASK_METADATA_VERSION}"
            ));
        }
    };
    let body = remainder
        .strip_prefix("\r\n")
        .or_else(|| remainder.strip_prefix('\n'))
        .unwrap_or(remainder)
        .to_owned();
    Ok((Some(metadata), body))
}

fn render_task_text(metadata: &TaskMetadata, body_markdown: &str) -> AppResult<String> {
    let json = serde_json::to_string(metadata)
        .map_err(|error| format!("cannot serialize task metadata: {error}"))?;
    if body_markdown.is_empty() {
        Ok(format!(
            "{TASK_METADATA_PREFIX}{json}{TASK_METADATA_SUFFIX}\n"
        ))
    } else {
        Ok(format!(
            "{TASK_METADATA_PREFIX}{json}{TASK_METADATA_SUFFIX}\n\n{body_markdown}"
        ))
    }
}

fn validate_task_metadata(task: &Task) -> AppResult<()> {
    let Some(metadata) = &task.metadata else {
        return Ok(());
    };
    if metadata.version != TASK_METADATA_VERSION {
        return Err(format!(
            "task '{}' uses unsupported metadata version {}; expected {}",
            task.id, metadata.version, TASK_METADATA_VERSION
        ));
    }
    let created_ms = metadata
        .created_at
        .as_deref()
        .map(parse_rfc3339_utc_millis)
        .transpose()
        .map_err(|error| format!("task '{}' has invalid creation time: {error}", task.id))?;
    let mut previous = None;
    for completion in &metadata.completions {
        let completion_ms = parse_rfc3339_utc_millis(completion)
            .map_err(|error| format!("task '{}' has invalid completion time: {error}", task.id))?;
        if previous.is_some_and(|value| completion_ms < value) {
            return Err(format!(
                "task '{}' has completion history that is not chronological",
                task.id
            ));
        }
        if created_ms.is_some_and(|value| completion_ms < value) {
            return Err(format!(
                "task '{}' has a completion timestamp before its creation timestamp",
                task.id
            ));
        }
        previous = Some(completion_ms);
    }
    Ok(())
}

fn task_metadata(task: &Task) -> TaskMetadata {
    task.metadata.clone().unwrap_or_else(|| TaskMetadata {
        version: TASK_METADATA_VERSION,
        created_at: task.created_at.clone(),
        completions: task.completions.clone(),
    })
}

fn upgrade_v1_metadata(metadata: TaskMetadataV1) -> AppResult<TaskMetadata> {
    if metadata.version != 1 {
        return Err(format!(
            "version 1 metadata declares version {}",
            metadata.version
        ));
    }
    if metadata
        .completion_history_unix_ms
        .windows(2)
        .any(|pair| pair[0] > pair[1])
    {
        return Err("version 1 completion history is not chronological".to_owned());
    }
    if metadata.completed_at_unix_ms.is_some()
        && metadata.completed_at_unix_ms != metadata.completion_history_unix_ms.last().copied()
    {
        return Err(
            "version 1 current completion does not match its completion history".to_owned(),
        );
    }
    Ok(TaskMetadata {
        version: TASK_METADATA_VERSION,
        created_at: Some(unix_ms_to_rfc3339(metadata.created_at_unix_ms)?),
        completions: metadata
            .completion_history_unix_ms
            .into_iter()
            .map(unix_ms_to_rfc3339)
            .collect::<AppResult<Vec<_>>>()?,
    })
}

fn system_time_to_rfc3339(time: SystemTime) -> AppResult<String> {
    let milliseconds = time
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock is before the Unix epoch: {error}"))?
        .as_millis();
    let milliseconds = u64::try_from(milliseconds)
        .map_err(|_| "system clock timestamp is too large".to_owned())?;
    unix_ms_to_rfc3339(milliseconds)
}

fn unix_ms_to_rfc3339(milliseconds: u64) -> AppResult<String> {
    let total_seconds = milliseconds / 1_000;
    let millisecond = milliseconds % 1_000;
    let days = i64::try_from(total_seconds / 86_400)
        .map_err(|_| "timestamp is outside the supported RFC 3339 range".to_owned())?;
    let seconds_of_day = total_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    if !(1970..=9999).contains(&year) {
        return Err("timestamp year must be between 1970 and 9999".to_owned());
    }
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    Ok(format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millisecond:03}Z"
    ))
}

fn parse_rfc3339_utc_millis(value: &str) -> AppResult<u64> {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    let regex = REGEX.get_or_init(|| {
        Regex::new(r"^(?P<year>[0-9]{4})-(?P<month>[0-9]{2})-(?P<day>[0-9]{2})T(?P<hour>[0-9]{2}):(?P<minute>[0-9]{2}):(?P<second>[0-9]{2})\.(?P<millisecond>[0-9]{3})Z$")
            .expect("canonical UTC timestamp regex is valid")
    });
    let captures = regex.captures(value).ok_or_else(|| {
        format!("'{value}' is not canonical UTC RFC 3339 with millisecond precision")
    })?;
    let parse = |name: &str| -> AppResult<u32> {
        captures[name]
            .parse::<u32>()
            .map_err(|error| format!("invalid {name}: {error}"))
    };
    let year = parse("year")?;
    let month = parse("month")?;
    let day = parse("day")?;
    let hour = parse("hour")?;
    let minute = parse("minute")?;
    let second = parse("second")?;
    let millisecond = parse("millisecond")?;
    if !(1970..=9999).contains(&year)
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(format!("'{value}' contains an out-of-range date or time"));
    }
    let days = days_from_civil(i64::from(year), i64::from(month), i64::from(day));
    let total = u64::try_from(days)
        .map_err(|_| "timestamp is before the Unix epoch".to_owned())?
        .checked_mul(86_400_000)
        .and_then(|value| value.checked_add(u64::from(hour) * 3_600_000))
        .and_then(|value| value.checked_add(u64::from(minute) * 60_000))
        .and_then(|value| value.checked_add(u64::from(second) * 1_000))
        .and_then(|value| value.checked_add(u64::from(millisecond)))
        .ok_or_else(|| "timestamp is too large".to_owned())?;
    Ok(total)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        _ => 31,
    }
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let shifted = days_since_epoch + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_piece = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_piece + 2) / 5 + 1;
    let month = month_piece + if month_piece < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let month_piece = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_piece + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn visit_dependency_node(
    node: &str,
    edges: &HashMap<String, BTreeSet<String>>,
    states: &mut HashMap<String, u8>,
    stack: &mut Vec<String>,
    cycles: &mut BTreeSet<String>,
) {
    states.insert(node.to_owned(), 1);
    stack.push(node.to_owned());
    if let Some(neighbors) = edges.get(node) {
        for neighbor in neighbors {
            match states.get(neighbor).copied() {
                None => visit_dependency_node(neighbor, edges, states, stack, cycles),
                Some(1) => {
                    if let Some(start) = stack.iter().position(|item| item == neighbor) {
                        let nodes = stack[start..].to_vec();
                        let canonical = canonical_cycle(&nodes);
                        let first = canonical.split(" -> ").next().unwrap_or_default();
                        cycles.insert(format!("{canonical} -> {first}"));
                    }
                }
                _ => {}
            }
        }
    }
    stack.pop();
    states.insert(node.to_owned(), 2);
}

fn canonical_cycle(nodes: &[String]) -> String {
    (0..nodes.len())
        .map(|start| {
            (0..nodes.len())
                .map(|offset| nodes[(start + offset) % nodes.len()].as_str())
                .collect::<Vec<_>>()
                .join(" -> ")
        })
        .min()
        .unwrap_or_default()
}

fn command_new(args: NewArgs) -> AppResult<()> {
    let repository = Repository::discover(true)?;
    repository.validate()?;
    let description = args.description.join(" ").trim().to_owned();
    validate_description(&description)?;
    let (priority, priority_text) = parse_priority(&args.priority)?;
    let parent = args.parent.as_deref().map(normalize_id).transpose()?;
    let blocks = normalize_id_list(args.blocks)?;
    validate_reference(&repository, parent.as_deref(), "parent")?;
    for blocked in &blocks {
        validate_reference(&repository, Some(blocked), "blocked task")?;
    }
    let body = read_new_body(args.body, args.body_file)?;
    let created_at = system_time_to_rfc3339(SystemTime::now())?;
    let metadata = TaskMetadata {
        version: TASK_METADATA_VERSION,
        created_at: Some(created_at.clone()),
        completions: Vec::new(),
    };
    let stored_text = render_task_text(&metadata, &body)?;

    let existing: HashSet<String> = repository
        .tasks
        .iter()
        .map(|task| task.id.clone())
        .collect();
    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    for suffix_length in 4usize.. {
        for _ in 0..100 {
            let suffix = (0..suffix_length)
                .map(|_| alphabet[rng.random_range(0..alphabet.len())] as char)
                .collect::<String>();
            let id = format!("t@{suffix}");
            if existing.contains(&id) {
                continue;
            }
            let task = Task {
                id,
                parent: parent.clone(),
                blocks: blocks.clone(),
                priority,
                priority_text: priority_text.clone(),
                kind: args.kind.into(),
                description: description.clone(),
                body_markdown: body.clone(),
                metadata: Some(metadata.clone()),
                created_at: Some(created_at.clone()),
                completions: Vec::new(),
                path: PathBuf::new(),
                state: TaskState::NotDone,
            };
            let filename = task_filename(&task);
            let path = repository.tasks_dir.join(filename);
            let mut prospective = repository.clone();
            let mut prospective_task = task.clone();
            prospective_task.path = path.clone();
            prospective.tasks.push(prospective_task);
            validate_prospective(&repository, &prospective, None)?;
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    if let Err(error) = file.write_all(stored_text.as_bytes()) {
                        drop(file);
                        let _ = fs::remove_file(&path);
                        return Err(format!("cannot write '{}': {error}", path.display()));
                    }
                    let absolute = absolute_path(&path)?;
                    println!("{}", rendered_task_path(&absolute, &task)?);
                    return Ok(());
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("cannot create '{}': {error}", path.display())),
            }
        }
    }
    unreachable!("unbounded ID suffix growth must eventually find a candidate")
}

fn command_list(args: ListArgs) -> AppResult<()> {
    let repository = Repository::discover(false)?;
    repository.validate()?;
    let blocked = repository.blocked_ids();
    let mut selected = repository
        .tasks
        .iter()
        .filter(|task| match args.mode {
            ListMode::Ready => task.state == TaskState::NotDone && !blocked.contains(&task.id),
            ListMode::Blocked => task.state == TaskState::NotDone && blocked.contains(&task.id),
            ListMode::NotDone => task.state == TaskState::NotDone,
            ListMode::Done => task.state == TaskState::Done,
            ListMode::All => true,
        })
        .collect::<Vec<_>>();
    sort_tasks(&mut selected);
    if args.json {
        let document = json_list_document(&repository, args.mode, &selected, &blocked)?;
        let json = serde_json::to_string_pretty(&document)
            .map_err(|error| format!("cannot serialize task list as JSON: {error}"))?;
        println!("{json}");
    } else if args.filenames {
        for task in selected {
            println!("{}", rendered_task_filename(task));
        }
    } else {
        print_markdown_table(&selected, &blocked);
    }
    Ok(())
}

fn json_list_document(
    repository: &Repository,
    mode: ListMode,
    selected: &[&Task],
    blocked: &HashSet<String>,
) -> AppResult<TaskListDocument> {
    let repository_name = repository
        .root
        .file_name()
        .and_then(OsStr::to_str)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| repository.root.display().to_string());
    Ok(TaskListDocument {
        schema_version: TASK_LIST_SCHEMA_VERSION,
        repository: repository_name,
        mode: mode.as_str().to_owned(),
        tasks: selected
            .iter()
            .map(|task| json_task(repository, task, blocked))
            .collect::<AppResult<Vec<_>>>()?,
    })
}

fn json_task(
    repository: &Repository,
    task: &Task,
    blocked: &HashSet<String>,
) -> AppResult<TaskListItem> {
    let parent = task
        .parent
        .as_deref()
        .and_then(|id| repository.by_id(id))
        .map(|related| json_task_reference(related, blocked));
    let children = json_task_references(
        repository
            .tasks
            .iter()
            .filter(|candidate| candidate.parent.as_deref() == Some(task.id.as_str()))
            .collect(),
        blocked,
    );
    let blocks = json_task_references(
        task.blocks
            .iter()
            .filter_map(|id| repository.by_id(id))
            .collect(),
        blocked,
    );
    let blocked_by = json_task_references(
        repository
            .tasks
            .iter()
            .filter(|candidate| candidate.blocks.iter().any(|id| id == &task.id))
            .collect(),
        blocked,
    );
    Ok(TaskListItem {
        id: task.id.clone(),
        title: task.description.clone(),
        body_markdown: task.body_markdown.clone(),
        created_at: task.created_at.clone(),
        completed_at: (task.state == TaskState::Done)
            .then(|| task.completions.last().cloned())
            .flatten(),
        completions: task.completions.clone(),
        priority: task.priority,
        priority_text: task.priority_text.clone(),
        task_type: task.kind.as_str().to_owned(),
        status: task_status(task, blocked).to_owned(),
        filename: rendered_task_filename(task),
        relationships: TaskRelationships {
            parent,
            children,
            blocks,
            blocked_by,
        },
        blocking_causes: json_blocking_causes(repository, task, blocked),
    })
}

fn json_task_references(mut tasks: Vec<&Task>, blocked: &HashSet<String>) -> Vec<TaskReference> {
    tasks.sort_by(|left, right| left.id.cmp(&right.id));
    tasks
        .into_iter()
        .map(|task| json_task_reference(task, blocked))
        .collect()
}

fn json_task_reference(task: &Task, blocked: &HashSet<String>) -> TaskReference {
    TaskReference {
        id: task.id.clone(),
        title: task.description.clone(),
        status: task_status(task, blocked).to_owned(),
        priority_text: task.priority_text.clone(),
    }
}

fn json_blocking_causes(
    repository: &Repository,
    task: &Task,
    blocked: &HashSet<String>,
) -> Vec<BlockingCause> {
    if task.state == TaskState::Done || !blocked.contains(&task.id) {
        return Vec::new();
    }
    let mut causes = Vec::new();
    let mut seen = BTreeSet::new();
    let mut current = Some(task);
    while let Some(target) = current {
        for blocker in repository.tasks.iter().filter(|candidate| {
            candidate.state == TaskState::NotDone
                && candidate.blocks.iter().any(|id| id == &target.id)
        }) {
            if seen.insert((blocker.id.clone(), target.id.clone())) {
                causes.push(BlockingCause {
                    blocker: json_task_reference(blocker, blocked),
                    blocked_target: json_task_reference(target, blocked),
                    kind: if target.id == task.id {
                        "direct".to_owned()
                    } else {
                        "inherited".to_owned()
                    },
                });
            }
        }
        current = target.parent.as_deref().and_then(|parent| {
            repository
                .by_id(parent)
                .filter(|parent_task| parent_task.state == TaskState::NotDone)
        });
    }
    causes.sort_by(|left, right| {
        left.blocker
            .id
            .cmp(&right.blocker.id)
            .then_with(|| left.blocked_target.id.cmp(&right.blocked_target.id))
    });
    causes
}

fn task_status<'a>(task: &Task, blocked: &'a HashSet<String>) -> &'static str {
    if task.state == TaskState::Done {
        "done"
    } else if blocked.contains(&task.id) {
        "blocked"
    } else {
        "ready"
    }
}

fn command_next() -> AppResult<()> {
    let repository = Repository::discover(false)?;
    repository.validate()?;
    let blocked = repository.blocked_ids();
    let mut ready = repository
        .tasks
        .iter()
        .filter(|task| task.state == TaskState::NotDone && !blocked.contains(&task.id))
        .collect::<Vec<_>>();
    sort_tasks(&mut ready);
    let task = ready
        .first()
        .ok_or_else(|| "no ready tasks are available".to_owned())?;
    println!("{}", paint(&task.id, ANSI_ID, stdout_color()));
    Ok(())
}

fn command_id(args: TaskFileArgs) -> AppResult<()> {
    let filename = file_name(&args.path)?;
    let captures = task_regex().captures(filename).ok_or_else(|| {
        format!(
            "task filename does not follow the required format: {}",
            args.path.display()
        )
    })?;
    println!("{}", paint(&captures["id"], ANSI_ID, stdout_color()));
    Ok(())
}

fn command_done(args: DoneArgs) -> AppResult<()> {
    let repository = Repository::discover(false)?;
    repository.validate()?;
    let completion_notes = read_optional_body(args.completion_notes, args.completion_notes_file)?;
    if completion_notes
        .as_ref()
        .is_some_and(|notes| notes.trim().is_empty())
    {
        return Err("completion notes cannot be empty when supplied".to_owned());
    }
    let id = normalize_id(&args.task_id)?;
    let task = repository
        .by_id(&id)
        .ok_or_else(|| format!("task '{id}' does not exist"))?;
    if task.state == TaskState::Done {
        return Err(format!("task '{id}' is already done"));
    }
    if repository.blocked_ids().contains(&id) {
        let reasons = repository.blocking_reasons(&id);
        let details = reasons
            .iter()
            .map(|reason| format!(" - {reason}"))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "task '{id}' is blocked and cannot be completed{}",
            if details.is_empty() {
                String::new()
            } else {
                format!(":\n{details}")
            }
        ));
    }
    let descendants = repository.not_done_descendants(&id);
    if !descendants.is_empty() {
        return Err(format!(
            "task '{id}' has not-done descendants and cannot be completed: {}",
            descendants.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    fs::create_dir_all(&repository.done_dir)
        .map_err(|error| format!("cannot create '{}': {error}", repository.done_dir.display()))?;
    let completion_time = SystemTime::now();
    let completed_at = system_time_to_rfc3339(completion_time)?;
    let mut metadata = task_metadata(task);
    metadata.completions.push(completed_at.clone());
    let body = append_completion_notes(
        &task.body_markdown,
        completion_notes.as_deref(),
        &completed_at,
    );
    let stored_text = render_task_text(&metadata, &body)?;
    let destination = repository.done_dir.join(file_name(&task.path)?);
    persist_task_change(
        &task.path,
        &destination,
        Some(&stored_text),
        Some(completion_time),
    )?;
    let absolute = absolute_path(&destination)?;
    println!("{}", rendered_task_path(&absolute, task)?);
    Ok(())
}

fn command_reopen(args: IdArgs) -> AppResult<()> {
    let repository = Repository::discover(false)?;
    repository.validate()?;
    let id = normalize_id(&args.task_id)?;
    let index = repository
        .tasks
        .iter()
        .position(|task| task.id == id)
        .ok_or_else(|| format!("task '{id}' does not exist"))?;
    if repository.tasks[index].state == TaskState::NotDone {
        return Err(format!("task '{id}' is already not-done"));
    }
    let mut prospective = repository.clone();
    prospective.tasks[index].state = TaskState::NotDone;
    let metadata = task_metadata(&repository.tasks[index]);
    prospective.tasks[index].metadata = Some(metadata.clone());
    let retry_command = format!("tat reopen {id}");
    validate_prospective(&repository, &prospective, Some(&retry_command))?;
    let source = &repository.tasks[index].path;
    let destination = repository.tasks_dir.join(file_name(source)?);
    let stored_text = render_task_text(&metadata, &repository.tasks[index].body_markdown)?;
    persist_task_change(source, &destination, Some(&stored_text), None)?;
    let absolute = absolute_path(&destination)?;
    println!(
        "{}",
        rendered_task_path(&absolute, &repository.tasks[index])?
    );
    Ok(())
}

fn command_set(args: SetArgs) -> AppResult<()> {
    let repository = Repository::discover(false)?;
    repository.validate()?;
    let id = normalize_id(&args.task_id)?;
    let index = repository
        .tasks
        .iter()
        .position(|task| task.id == id)
        .ok_or_else(|| format!("task '{id}' does not exist"))?;
    let mut updated = repository.tasks[index].clone();
    let mut changed = false;

    if let Some(description) = args.description {
        let normalized = description.trim().to_owned();
        validate_description(&normalized)?;
        updated.description = normalized;
        changed = true;
    }
    if let Some(priority) = args.priority {
        let (number, text) = parse_priority(&priority)?;
        updated.priority = number;
        updated.priority_text = text;
        changed = true;
    }
    if let Some(kind) = args.kind {
        updated.kind = kind.into();
        changed = true;
    }
    if let Some(parent) = args.parent {
        updated.parent = Some(normalize_id(&parent)?);
        changed = true;
    } else if args.clear_parent {
        updated.parent = None;
        changed = true;
    }
    if let Some(blocks) = args.blocks {
        updated.blocks = normalize_id_list(blocks)?;
        changed = true;
    } else if args.clear_blocks {
        updated.blocks.clear();
        changed = true;
    } else {
        for blocked in normalize_id_list(args.add_blocks)? {
            if !updated.blocks.contains(&blocked) {
                updated.blocks.push(blocked);
                changed = true;
            }
        }
        let remove = normalize_id_list(args.remove_blocks)?;
        if !remove.is_empty() {
            let before = updated.blocks.len();
            updated.blocks.retain(|blocked| !remove.contains(blocked));
            changed |= updated.blocks.len() != before;
        }
    }
    updated.blocks.sort();
    updated.blocks.dedup();

    let replacement_body = read_optional_body(args.body, args.body_file)?;
    changed |= replacement_body.is_some();
    if !changed {
        return Err("no changes were requested".to_owned());
    }

    let destination_directory = match updated.state {
        TaskState::NotDone => &repository.tasks_dir,
        TaskState::Done => &repository.done_dir,
    };
    let retry_command = replacement_body
        .is_none()
        .then(|| set_retry_command(&repository.tasks[index], &updated))
        .flatten();
    let destination = destination_directory.join(task_filename(&updated));
    updated.path = destination.clone();
    let body = replacement_body
        .as_ref()
        .unwrap_or(&updated.body_markdown)
        .clone();
    let metadata = task_metadata(&updated);
    updated.body_markdown = body.clone();
    updated.metadata = Some(metadata.clone());
    let replacement_text = render_task_text(&metadata, &body)?;
    let mut prospective = repository.clone();
    prospective.tasks[index] = updated;
    validate_prospective(&repository, &prospective, retry_command.as_deref())?;
    persist_task_change(
        &repository.tasks[index].path,
        &destination,
        Some(&replacement_text),
        None,
    )?;
    let absolute = absolute_path(&destination)?;
    println!(
        "{}",
        rendered_task_path(&absolute, &prospective.tasks[index])?
    );
    Ok(())
}

fn command_view(args: ViewArgs) -> AppResult<()> {
    let repository = Repository::discover(false)?;
    repository.validate()?;
    let id = normalize_id(&args.task_id)?;
    let task = repository
        .by_id(&id)
        .ok_or_else(|| format!("task '{id}' does not exist"))?;
    if args.filename {
        println!("{}", rendered_task_filename(task));
        return Ok(());
    }
    let blocked = repository.blocked_ids().contains(&id);
    let status = if task.state == TaskState::Done {
        "done"
    } else if blocked {
        "blocked"
    } else {
        "ready"
    };
    println!(
        "{}",
        paint(
            &format!("# {} — {}", task.id, markdown_cell(&task.description)),
            ANSI_HEADING,
            stdout_color()
        )
    );
    println!();
    println!("| Field | Value |");
    println!("|---|---|");
    println!(
        "| Status | {} |",
        paint(
            status,
            match status {
                "blocked" => ANSI_BLOCKED,
                "done" => ANSI_DONE,
                _ => ANSI_READY,
            },
            stdout_color()
        )
    );
    println!(
        "| Priority | {} |",
        paint(&task.priority_text, ANSI_PRIORITY, stdout_color())
    );
    println!(
        "| Type | {} |",
        paint(task.kind.as_str(), ANSI_KIND, stdout_color())
    );
    println!(
        "| Created (UTC) | {} |",
        task.created_at.as_deref().unwrap_or("")
    );
    println!(
        "| Completed (UTC) | {} |",
        (task.state == TaskState::Done)
            .then(|| task.completions.last().map(String::as_str))
            .flatten()
            .unwrap_or("")
    );
    println!(
        "| Parent | {} |",
        paint(
            task.parent.as_deref().unwrap_or(""),
            ANSI_PARENT,
            stdout_color()
        )
    );
    println!(
        "| Blocks | {} |",
        paint(&task.blocks.join(", "), ANSI_BLOCKS, stdout_color())
    );
    println!("| Filename | {} |", rendered_task_filename(task));
    let body = &task.body_markdown;
    if !body.is_empty() {
        println!();
        println!("{}", paint("## Body", ANSI_HEADING, stdout_color()));
        println!();
        print!("{body}");
        if !body.ends_with('\n') {
            println!();
        }
    }
    Ok(())
}

fn command_remove(args: RemoveArgs) -> AppResult<()> {
    if !args.force {
        return Err(format!(
            "remove of task '{}' is permanent; pass --force to confirm",
            args.task_id
        ));
    }
    let repository = Repository::discover(false)?;
    repository.validate()?;
    let id = normalize_id(&args.task_id)?;
    let task = repository
        .by_id(&id)
        .ok_or_else(|| format!("task '{id}' does not exist"))?;
    let references = repository.references_to(&id);
    if !references.is_empty() {
        return Err(format!(
            "task '{id}' is still referenced and cannot be removed:\n - {}",
            references.join("\n - ")
        ));
    }
    fs::remove_file(&task.path)
        .map_err(|error| format!("cannot remove '{}': {error}", task.path.display()))?;
    println!(
        "{} {}",
        paint("removed", ANSI_WARNING, stdout_color()),
        rendered_task_path(&task.path, task)?
    );
    Ok(())
}

fn command_check() -> AppResult<()> {
    let repository = Repository::discover(false)?;
    repository.validate()?;
    let not_done = repository
        .tasks
        .iter()
        .filter(|task| task.state == TaskState::NotDone)
        .count();
    let done = repository.tasks.len() - not_done;
    println!(
        "{}: {} not-done, {} done task(s)",
        paint("OK", ANSI_READY, stdout_color()),
        paint(&not_done.to_string(), ANSI_ID, stdout_color()),
        paint(&done.to_string(), ANSI_DONE, stdout_color())
    );
    Ok(())
}

fn command_guide() -> AppResult<()> {
    let document = [GUIDE_OVERVIEW, GUIDE_RECIPES].join(&format!("\n\n{}\n\n", "=".repeat(78)));
    println!("{}", colorize_document(&document, stdout_color()));
    Ok(())
}

fn command_reference() -> AppResult<()> {
    let document = [
        REFERENCE_INTRO,
        GUIDE_MODEL,
        GUIDE_DEPENDENCIES,
        GUIDE_AUTOMATION,
        GUIDE_TROUBLESHOOTING,
    ]
    .join(&format!("\n\n{}\n\n", "=".repeat(78)));
    println!("{}", colorize_document(&document, stdout_color()));
    Ok(())
}

fn requested_color(arguments: &[OsString]) -> ColorWhen {
    for (index, argument) in arguments.iter().enumerate() {
        let value = argument.to_string_lossy();
        if let Some(choice) = value.strip_prefix("--color=") {
            return parse_color_choice(choice).unwrap_or(ColorWhen::Auto);
        }
        if value == "--color"
            && let Some(choice) = arguments.get(index + 1)
        {
            return parse_color_choice(&choice.to_string_lossy()).unwrap_or(ColorWhen::Auto);
        }
    }
    ColorWhen::Auto
}

fn parse_color_choice(value: &str) -> Option<ColorWhen> {
    match value.to_ascii_lowercase().as_str() {
        "auto" => Some(ColorWhen::Auto),
        "always" => Some(ColorWhen::Always),
        "never" => Some(ColorWhen::Never),
        _ => None,
    }
}

fn configure_color(choice: ColorWhen) {
    COLOR_STDOUT.store(
        color_enabled(choice, io::stdout().is_terminal()),
        AtomicOrdering::Relaxed,
    );
    COLOR_STDERR.store(
        color_enabled(choice, io::stderr().is_terminal()),
        AtomicOrdering::Relaxed,
    );
}

fn color_enabled(choice: ColorWhen, is_terminal: bool) -> bool {
    match choice {
        ColorWhen::Always => true,
        ColorWhen::Never => false,
        ColorWhen::Auto => {
            is_terminal
                && std::env::var_os("NO_COLOR").is_none()
                && std::env::var("TERM").map_or(true, |value| value != "dumb")
                && std::env::var("CLICOLOR").map_or(true, |value| value != "0")
        }
    }
}

fn stdout_color() -> bool {
    COLOR_STDOUT.load(AtomicOrdering::Relaxed)
}

fn paint(value: &str, ansi: &str, color: bool) -> String {
    if !color || value.is_empty() {
        value.to_owned()
    } else {
        format!("{ansi}{value}{ANSI_RESET}")
    }
}

fn paint_stderr(value: &str, ansi: &str) -> String {
    paint(value, ansi, COLOR_STDERR.load(AtomicOrdering::Relaxed))
}

fn colorize_document(document: &str, color: bool) -> String {
    if !color {
        return document.to_owned();
    }
    let mut output = String::with_capacity(document.len() + 128);
    for segment in document.split_inclusive('\n') {
        let (line, newline) = segment
            .strip_suffix('\n')
            .map_or((segment, ""), |line| (line, "\n"));
        let trimmed = line.trim();
        let rendered = if trimmed.is_empty() {
            line.to_owned()
        } else if trimmed.starts_with("error:") {
            paint(line, ANSI_ERROR, true)
        } else if trimmed == "WARNING" || trimmed.starts_with("WARNING ") {
            paint(line, ANSI_WARNING, true)
        } else if is_document_heading(trimmed) {
            paint(line, ANSI_HEADING, true)
        } else if is_help_command_entry(trimmed) {
            colorize_help_command_entry(line)
        } else if is_help_option_entry(trimmed) {
            paint(line, ANSI_COMMAND, true)
        } else if is_document_example(trimmed) {
            paint(line, ANSI_COMMAND, true)
        } else {
            colorize_inline_code(line)
        };
        output.push_str(&rendered);
        output.push_str(newline);
    }
    output
}

fn is_document_heading(value: &str) -> bool {
    value.starts_with("Usage:")
        || matches!(value, "Commands:" | "Arguments:" | "Options:")
        || (value.chars().any(char::is_alphabetic)
            && !value.chars().any(char::is_lowercase)
            && value.len() <= 80)
}

fn is_help_command_entry(value: &str) -> bool {
    let Some(command) = value.split_whitespace().next() else {
        return false;
    };
    matches!(
        command,
        "new"
            | "list"
            | "next"
            | "id"
            | "done"
            | "set"
            | "reopen"
            | "view"
            | "remove"
            | "check"
            | "validate"
            | "guide"
            | "reference"
            | "help"
    )
}

fn colorize_help_command_entry(line: &str) -> String {
    let start = line
        .find(|character: char| !character.is_whitespace())
        .unwrap_or(0);
    let end = line[start..]
        .find(char::is_whitespace)
        .map_or(line.len(), |offset| start + offset);
    format!(
        "{}{}{}",
        &line[..start],
        paint(&line[start..end], ANSI_COMMAND, true),
        &line[end..]
    )
}

fn is_help_option_entry(value: &str) -> bool {
    let mut fields = value.split_whitespace();
    let Some(first) = fields.next() else {
        return false;
    };
    if !first.starts_with('-') {
        return false;
    }
    if first.ends_with(',') {
        return fields.next().is_some_and(|field| field.starts_with("--"));
    }
    fields
        .next()
        .is_none_or(|field| field.starts_with('<') || field.starts_with('['))
}

fn is_document_example(value: &str) -> bool {
    let tat_command = value
        .strip_prefix("tat ")
        .and_then(|remainder| remainder.split_whitespace().next())
        .is_some_and(|command| {
            matches!(
                command,
                "new"
                    | "list"
                    | "ls"
                    | "next"
                    | "id"
                    | "done"
                    | "complete"
                    | "close"
                    | "set"
                    | "edit"
                    | "update"
                    | "change"
                    | "reopen"
                    | "open"
                    | "view"
                    | "show"
                    | "remove"
                    | "delete"
                    | "rm"
                    | "validate"
                    | "check"
                    | "guide"
                    | "help"
            )
        });
    tat_command
        || value.starts_with("$parent")
        || value.starts_with("$work")
        || value.starts_with("$gate")
        || value.starts_with("$path")
        || value.starts_with("$files")
        || value.starts_with("Get-Content ")
        || value.starts_with("if (")
        || value.starts_with("# ")
        || value.starts_with("t@")
}

fn colorize_inline_code(line: &str) -> String {
    let mut output = String::with_capacity(line.len() + 16);
    for (index, part) in line.split('`').enumerate() {
        if index > 0 {
            output.push_str(&paint("`", ANSI_DIM, true));
        }
        if index % 2 == 1 {
            output.push_str(&paint(part, ANSI_PRIORITY, true));
        } else {
            output.push_str(part);
        }
    }
    output
}

fn normalize_id(value: &str) -> AppResult<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if !id_regex().is_match(&normalized) {
        return Err(format!(
            "'{value}' is not a valid task ID; IDs must start with t@"
        ));
    }
    Ok(normalized)
}

fn normalize_id_list(values: Vec<String>) -> AppResult<Vec<String>> {
    if values.is_empty() {
        return Ok(Vec::new());
    }
    let mut normalized = Vec::new();
    for value in values {
        for item in value
            .split(|character: char| {
                character == ',' || character == '-' || character.is_whitespace()
            })
            .filter(|item| !item.is_empty())
        {
            let id = normalize_id(item)?;
            if !normalized.contains(&id) {
                normalized.push(id);
            }
        }
    }
    if normalized.is_empty() {
        return Err("task ID list must contain at least one ID".to_owned());
    }
    normalized.sort();
    Ok(normalized)
}

fn validate_reference(repository: &Repository, value: Option<&str>, label: &str) -> AppResult<()> {
    if let Some(id) = value
        && repository.by_id(id).is_none()
    {
        return Err(format!("{label} '{id}' does not identify an existing task"));
    }
    Ok(())
}

fn validate_description(description: &str) -> AppResult<()> {
    if description.trim().is_empty() {
        return Err("description must contain a non-whitespace character".to_owned());
    }
    if description.ends_with('.') || description.ends_with(' ') {
        return Err("description must not end with a period or space".to_owned());
    }
    if description.chars().any(|character| {
        matches!(
            character,
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'
        ) || character.is_control()
    }) {
        return Err("description contains a character invalid in a Windows filename".to_owned());
    }
    Ok(())
}

fn parse_priority(value: &str) -> AppResult<(f64, String)> {
    let number = value
        .parse::<f64>()
        .map_err(|_| format!("priority '{value}' is not a number"))?;
    if !number.is_finite() || !(0.0..=9.0).contains(&number) {
        return Err("priority must be from 0 through 9".to_owned());
    }
    let mut text = format!("{number:.12}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    Ok((number, text))
}

fn task_filename(task: &Task) -> String {
    let mut parts = vec![task.id.clone()];
    if let Some(parent) = &task.parent {
        parts.push(format!("parent-{parent}"));
    }
    if !task.blocks.is_empty() {
        parts.push(format!("blocks-{}", task.blocks.join("-")));
    }
    parts.push(format!("p{}", task.priority_text));
    parts.push(task.kind.as_str().to_owned());
    parts.push(task.description.clone());
    format!("{}.md", parts.join(", "))
}

fn rendered_task_filename(task: &Task) -> String {
    if !stdout_color() {
        return task_filename(task);
    }
    let mut parts = vec![paint(&task.id, ANSI_ID, true)];
    if let Some(parent) = &task.parent {
        parts.push(paint(&format!("parent-{parent}"), ANSI_PARENT, true));
    }
    if !task.blocks.is_empty() {
        parts.push(paint(
            &format!("blocks-{}", task.blocks.join("-")),
            ANSI_BLOCKS,
            true,
        ));
    }
    parts.push(paint(
        &format!("p{}", task.priority_text),
        ANSI_PRIORITY,
        true,
    ));
    parts.push(paint(task.kind.as_str(), ANSI_KIND, true));
    parts.push(paint(&task.description, ANSI_DESCRIPTION, true));
    format!(
        "{}{}",
        parts.join(&paint(", ", ANSI_DIM, true)),
        paint(".md", ANSI_DIM, true)
    )
}

fn rendered_task_path(path: &Path, task: &Task) -> AppResult<String> {
    if !stdout_color() {
        return Ok(path.display().to_string());
    }
    let filename = file_name(path)?;
    let full = path.display().to_string();
    let prefix = full.strip_suffix(filename).unwrap_or_default();
    Ok(format!(
        "{}{}",
        paint(prefix, ANSI_DIM, true),
        rendered_task_filename(task)
    ))
}

fn sort_tasks(tasks: &mut [&Task]) {
    tasks.sort_by(|left, right| {
        left.priority
            .partial_cmp(&right.priority)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left.id.cmp(&right.id))
    });
}

fn print_markdown_table(tasks: &[&Task], blocked: &HashSet<String>) {
    const HEADERS: [&str; 7] = [
        "ID",
        "Description",
        "Prio",
        "Type",
        "Status",
        "Parent",
        "Blocks",
    ];
    let mut rows = Vec::new();
    for task in tasks {
        let status = if task.state == TaskState::Done {
            "done"
        } else if blocked.contains(&task.id) {
            "blocked"
        } else {
            "ready"
        };
        rows.push([
            markdown_cell(&task.id),
            markdown_cell(&task.description),
            task.priority_text.clone(),
            task.kind.as_str().to_owned(),
            status.to_owned(),
            markdown_cell(task.parent.as_deref().unwrap_or("")),
            markdown_cell(&task.blocks.join(", ")),
        ]);
    }
    let mut widths = HEADERS.map(UnicodeWidthStr::width);
    for row in &rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(UnicodeWidthStr::width(cell.as_str()));
        }
    }
    for width in &mut widths {
        *width = (*width).max(3);
    }
    print_table_row(
        &HEADERS.map(str::to_owned),
        &widths,
        &[CellStyle::Header; 7],
        None,
    );
    print_table_separator(&widths, Some(2));
    for row in rows {
        let status_style = match row[4].as_str() {
            "ready" => CellStyle::Ready,
            "blocked" => CellStyle::Blocked,
            _ => CellStyle::Done,
        };
        print_table_row(
            &row,
            &widths,
            &[
                CellStyle::Id,
                CellStyle::Description,
                CellStyle::Priority,
                CellStyle::Kind,
                status_style,
                CellStyle::Parent,
                CellStyle::Blocks,
            ],
            Some(2),
        );
    }
}

#[derive(Clone, Copy)]
enum CellStyle {
    Header,
    Id,
    Description,
    Priority,
    Kind,
    Ready,
    Blocked,
    Done,
    Parent,
    Blocks,
}

fn print_table_row<const N: usize>(
    cells: &[String; N],
    widths: &[usize; N],
    styles: &[CellStyle; N],
    right_aligned: Option<usize>,
) {
    let color = stdout_color();
    let border = paint("|", ANSI_DIM, color);
    let mut rendered = border.clone();
    for index in 0..N {
        let padding = widths[index].saturating_sub(UnicodeWidthStr::width(cells[index].as_str()));
        let padded = if right_aligned == Some(index) {
            format!("{}{}", " ".repeat(padding), cells[index])
        } else {
            format!("{}{}", cells[index], " ".repeat(padding))
        };
        rendered.push_str(&paint(&padded, cell_ansi(styles[index]), color));
        rendered.push_str(&border);
    }
    println!("{rendered}");
}

fn print_table_separator<const N: usize>(widths: &[usize; N], right_aligned: Option<usize>) {
    let color = stdout_color();
    let border = paint("|", ANSI_DIM, color);
    let mut rendered = border.clone();
    for (index, width) in widths.iter().copied().enumerate() {
        let marker = if right_aligned == Some(index) {
            format!("{}:", "-".repeat(width.saturating_sub(1)))
        } else {
            "-".repeat(width)
        };
        rendered.push_str(&paint(&marker, ANSI_DIM, color));
        rendered.push_str(&border);
    }
    println!("{rendered}");
}

fn cell_ansi(style: CellStyle) -> &'static str {
    match style {
        CellStyle::Header => ANSI_HEADING,
        CellStyle::Id => ANSI_ID,
        CellStyle::Description => ANSI_DESCRIPTION,
        CellStyle::Priority => ANSI_PRIORITY,
        CellStyle::Kind => ANSI_KIND,
        CellStyle::Ready => ANSI_READY,
        CellStyle::Blocked => ANSI_BLOCKED,
        CellStyle::Done => ANSI_DONE,
        CellStyle::Parent => ANSI_PARENT,
        CellStyle::Blocks => ANSI_BLOCKS,
    }
}

fn markdown_cell(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\r', '\n'], "<br>")
}

fn read_new_body(body: Option<String>, body_file: Option<PathBuf>) -> AppResult<String> {
    if let Some(body) = body {
        return Ok(body);
    }
    if let Some(path) = body_file {
        return fs::read_to_string(&path)
            .map_err(|error| format!("cannot read '{}': {error}", path.display()));
    }
    if !io::stdin().is_terminal() {
        let mut text = String::new();
        io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| format!("cannot read stdin: {error}"))?;
        return Ok(text);
    }
    Ok(String::new())
}

fn read_optional_body(
    body: Option<String>,
    body_file: Option<PathBuf>,
) -> AppResult<Option<String>> {
    if let Some(body) = body {
        return Ok(Some(body));
    }
    if let Some(path) = body_file {
        return fs::read_to_string(&path)
            .map(Some)
            .map_err(|error| format!("cannot read '{}': {error}", path.display()));
    }
    Ok(None)
}

fn append_completion_notes(
    body_markdown: &str,
    completion_notes: Option<&str>,
    completed_at: &str,
) -> String {
    let Some(notes) = completion_notes else {
        return body_markdown.to_owned();
    };
    let notes = notes.trim_end_matches(['\r', '\n']);
    let mut body = body_markdown.to_owned();
    if !body.is_empty() {
        if !body.ends_with('\n') {
            body.push('\n');
        }
        if !body.ends_with("\n\n") {
            body.push('\n');
        }
    }
    body.push_str(&format!("## Completion notes — {completed_at}\n\n"));
    body.push_str(notes);
    body
}

fn set_file_modified_time(path: &Path, modified: SystemTime) -> AppResult<()> {
    let file = File::options()
        .write(true)
        .open(path)
        .map_err(|error| format!("cannot open completed task '{}': {error}", path.display()))?;
    file.set_times(FileTimes::new().set_modified(modified))
        .map_err(|error| {
            format!(
                "cannot set completed task modification time '{}': {error}",
                path.display()
            )
        })
}

fn persist_task_change(
    source: &Path,
    destination: &Path,
    body: Option<&str>,
    modified: Option<SystemTime>,
) -> AppResult<()> {
    if modified.is_some() && body.is_none() {
        return Err(
            "internal error: a transactional timestamp requires rewritten task text".to_owned(),
        );
    }
    if source == destination {
        if let Some(body) = body {
            fs::write(source, body.as_bytes())
                .map_err(|error| format!("cannot update '{}': {error}", source.display()))?;
        }
        if let Some(modified) = modified {
            set_file_modified_time(source, modified)?;
        }
        return Ok(());
    }
    if destination.exists() {
        return Err(format!(
            "destination already exists: {}",
            destination.display()
        ));
    }
    if let Some(body) = body {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .map_err(|error| format!("cannot create '{}': {error}", destination.display()))?;
        if let Err(error) = file.write_all(body.as_bytes()) {
            let _ = fs::remove_file(destination);
            return Err(format!("cannot write '{}': {error}", destination.display()));
        }
        drop(file);
        if let Some(modified) = modified
            && let Err(error) = set_file_modified_time(destination, modified)
        {
            let _ = fs::remove_file(destination);
            return Err(error);
        }
        if let Err(error) = fs::remove_file(source) {
            let _ = fs::remove_file(destination);
            return Err(format!(
                "cannot remove old task '{}': {error}",
                source.display()
            ));
        }
    } else {
        fs::rename(source, destination).map_err(|error| {
            format!(
                "cannot rename '{}' to '{}': {error}",
                source.display(),
                destination.display()
            )
        })?;
    }
    Ok(())
}

fn absolute_path(path: &Path) -> AppResult<PathBuf> {
    path.canonicalize()
        .map_err(|error| format!("cannot resolve '{}': {error}", path.display()))
}

#[cfg(test)]
mod timestamp_tests {
    use super::*;

    #[test]
    fn canonical_utc_timestamp_round_trips_known_values() {
        for (milliseconds, expected) in [
            (0, "1970-01-01T00:00:00.000Z"),
            (951_827_696_789, "2000-02-29T12:34:56.789Z"),
            (1_700_000_000_000, "2023-11-14T22:13:20.000Z"),
        ] {
            assert_eq!(unix_ms_to_rfc3339(milliseconds).unwrap(), expected);
            assert_eq!(parse_rfc3339_utc_millis(expected).unwrap(), milliseconds);
        }
    }

    #[test]
    fn canonical_utc_timestamp_rejects_invalid_or_noncanonical_values() {
        for value in [
            "2026-02-29T12:00:00.000Z",
            "2026-01-01T24:00:00.000Z",
            "2026-01-01T00:00:00Z",
            "2026-01-01T00:00:00.000+00:00",
        ] {
            assert!(parse_rfc3339_utc_millis(value).is_err(), "accepted {value}");
        }
    }
}

fn file_name(path: &Path) -> AppResult<&str> {
    path.file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("path has no UTF-8 filename: {}", path.display()))
}
