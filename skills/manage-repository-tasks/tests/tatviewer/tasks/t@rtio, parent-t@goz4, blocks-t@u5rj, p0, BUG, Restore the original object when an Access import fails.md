<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:28.857Z","completions":[]} -->

## Current behavior
`ImportDatabaseObjects` deletes the existing object before `LoadFromText` has imported its replacement. If the import fails, the candidate database loses the original object.

## Intended behavior
Import only into a disposable database copy. Save a backup before replacing an object, and restore the original object if the import fails.

## Acceptance
- A forced LoadFromText failure preserves or restores the original object.
- The command returns a failure and names the object that could not be imported.
- A successful replacement survives save, close, reopen, compile, and re-export.
- Regression tests cover successful and failed imports.

## Completion notes — 2026-09-19T12:33:50.232Z

Added backup and restore handling to `ImportDatabaseObjects`. Also fixed object-type handling and single-file imports, added automation entry points and regression tests, and tested the workflow with 32-bit Access. A forced failure restored the original object, and a successful import survived save, compile, close, and reopen.
