<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.338Z","completions":[]} -->

## Goal
Prevent generated or live Access database files from being committed to Git.

## Acceptance
- Add *.accdb, *.laccdb, and *.mdb to .gitignore.
- Add an exception only when the repository intentionally includes a reviewed test fixture.
- Confirm that the ignore rules do not hide a file already tracked by Git.
- Document that Access database deliverables belong in the package exchange, not in Git.

## Completion notes — 2026-09-19T13:26:11.841Z

Added repository-wide ignore rules for `*.accdb`, `*.laccdb`, and `*.mdb`. Confirmed that Git tracks none of these files. Documented that database deliverables must use immutable exchange packages instead of Git.
