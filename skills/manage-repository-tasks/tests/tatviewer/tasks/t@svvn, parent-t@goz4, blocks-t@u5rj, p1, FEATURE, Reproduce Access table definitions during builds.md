<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.096Z","completions":[]} -->

## Current behavior
The text importer silently skips `TableDef_` files, which describe Access tables. As a result, the source snapshot cannot rebuild or verify the database schema by itself.

## Goal
Handle local and linked table definitions, indexes, and relevant relationships explicitly. Do not import business data.

## Acceptance
- A build recreates every required definition or reports exactly which schema feature it cannot handle.
- Validation checks linked tables without storing environment-specific secrets or changing production targets.
- Table definition snapshots remain available for review, but the build does not assume that a snapshot is complete.
- Export and re-import tests cover representative local and linked tables.
