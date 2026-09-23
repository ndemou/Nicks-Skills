<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.673Z","completions":[]} -->

## Goal
Run every automated check required before an Access build candidate can be accepted.

## Acceptance
- Run the Python tests, compile the VBA code, test the Access fixtures and scope rules, and run affected integration tests.
- Export the candidate again and compare the normalized source with the input. Report every unexpected object change.
- Verify the manifest, code references, linked tables, startup settings, and database schema.
- Save a machine-readable result and a short report for people.
- Return a failure if any required check fails.
