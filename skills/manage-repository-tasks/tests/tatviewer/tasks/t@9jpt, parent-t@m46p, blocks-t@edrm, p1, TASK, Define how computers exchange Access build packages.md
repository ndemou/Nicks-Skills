<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.815Z","completions":[]} -->

## Goal
Define predictable folders and rules for exchanging Access build packages between computers.

## Acceptance
- Define inbox, outbox, and archive folders. State who may write to and read from each folder.
- Give every package a unique, permanent name.
- Copy an incoming Access database file locally and verify its SHA-256 hash before opening it.
- Record the base Git commit, application version, build ID, author, timestamp, change summary, and known changed objects in `transfer.json`.
- Never edit or run a database directly from the shared drive.
- Explain what access approval is needed when the sandbox cannot reach the shared drive.

## Completion notes — 2026-09-19T13:24:34.192Z

Created inbox, outbox, and archive folders in the shared test area. The guide defines folder ownership, permanent ZIP names, the `transfer.json` format, local copying and hash verification, handoff rules, and the access approval required by the sandbox.
