<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:28.929Z","completions":[]} -->

## Current behavior
`ExportDatabaseObjects` continues after an object fails to export. The resulting folder can look complete even though files are missing.

## Intended behavior
Report every export error and fail if any expected object is missing. Never publish an incomplete export or use it to replace the trusted snapshot.

## Acceptance
- Check the expected object list and object counts.
- Report added, removed, failed, and duplicate objects.
- A simulated `SaveAsText` failure returns a failure and leaves the trusted snapshot unchanged.
- A complete export passes manifest and hash validation.

## Completion notes — 2026-09-19T12:51:26.255Z

Changed `ExportDatabaseObjects` to write into a `.partial` folder and publish the export only after every object succeeds. It now reports failed, missing, added, and duplicate objects and checks the final manifest and hashes. Tests with 32-bit Access covered both failure and success, and the full test suite passed.
