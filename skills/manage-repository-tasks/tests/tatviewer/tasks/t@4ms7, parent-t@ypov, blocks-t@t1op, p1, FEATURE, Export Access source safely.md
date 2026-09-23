<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.519Z","completions":[]} -->

## Goal
Copy a source Access database to a local work folder and open the copy with startup automation disabled. Export every supported object, remove only documented Access-generated noise, and create the shared manifest.

## Acceptance
- Never export directly from the shared drive or change the supplied database.
- Use 32-bit Access automation when the installed version requires it.
- Check that the export is complete and that encodings, exclusions, and hashes match the manifest.
- Exclude credentials, runtime data, and other sensitive values.
- The same input produces the same output, so Git reviews and three-way comparisons show only real changes.

## Completion notes — 2026-09-19T13:22:27.108Z

Implemented `scripts/Export-AccessSource.ps1`. It validates manifest version 1, verifies the source hash before and after export, works from a local copy, disables startup automation, removes only a test macro, keeps only approved configuration values, checks encodings and credentials, and writes sanitized metadata. Two exports of the same database produced identical files and tree hashes. The source database remained unchanged, and the full test suite passed.
