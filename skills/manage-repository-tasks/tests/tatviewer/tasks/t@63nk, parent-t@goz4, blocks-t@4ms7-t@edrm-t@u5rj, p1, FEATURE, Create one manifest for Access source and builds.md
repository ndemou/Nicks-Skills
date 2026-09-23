<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.252Z","completions":[]} -->

## Goal
Create one machine-readable manifest that identifies the source files and verifies each important part of an Access build candidate.

## The manifest records
- The Git base commit, build ID, application version, creation time, and SHA-256 hash of the Access database file.
- Every expected object, its type and encoding, its file hash, and the object totals.
- VBA references, relevant database and startup settings, linked-table definitions, and relationships.
- Runtime values, temporary objects, credentials, and business data that were intentionally excluded.

## Acceptance
The export, build, test, and transfer tools use the same versioned manifest format. They reject unsupported versions and any value that does not match the candidate.

## Completion notes — 2026-09-19T13:13:06.760Z

Created version 1 of `access-manifest.json`. It records the build source, database identity, object hashes and counts, references, settings, linked tables, fields, indexes, relationships, and exclusions. The builder produces the same manifest for the same input, and the validator rejects mismatches. Import, export, install, and compile tests now use this manifest. The full test suite passed.
