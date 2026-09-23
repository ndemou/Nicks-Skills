<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.748Z","completions":[]} -->

## Goal
Create a read-only package for sending or receiving an Access build through the shared exchange folder.

## Acceptance
- Include the front-end Access database, `transfer.json`, a notes template, file hashes, source commit, application version, build ID, and test report.
- Verify the copied package before publishing it.
- Never include the live back-end database, credentials, lock files, or business data.
- Use a unique dated folder and never overwrite an existing package.
