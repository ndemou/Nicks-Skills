<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.173Z","completions":[]} -->

## Goal
Store each intentional schema change as an ordered Microsoft Data Access Objects migration script. Each script must be safe to run more than once. Do not rely on table snapshots or manual database edits alone.

## Acceptance
- Each migration has a stable ID, and the database records when it has been applied.
- Running an applied migration again makes no further changes.
- The migration checks the schema before and after its changes.
- If a migration fails, the development database can be restored.
- Tests cover the first run, a repeated run, and an invalid starting schema.
- Business data is not committed to Git.
