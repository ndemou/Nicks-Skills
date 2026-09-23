<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.593Z","completions":[]} -->

## Goal
Build a disposable Access database candidate from a named baseline, selected Git source files, and the required schema migrations.

## Acceptance
- Records the base commit, baseline hash, application version, and build ID.
- Import objects without risking data loss. Each schema migration must be safe to run more than once.
- Never change the trusted baseline or a database received through the shared drive.
- Save, close, reopen, compile, and re-export the candidate.
- Reject the candidate if any object, schema, code reference, or linked table does not match the manifest.
