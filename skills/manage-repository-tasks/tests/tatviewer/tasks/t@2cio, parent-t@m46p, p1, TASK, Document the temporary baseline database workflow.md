<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.894Z","completions":[]} -->

## Goal
Document the temporary workflow used until the build is fully reproducible. Review text exports in Git, but start each build from a trusted Access database file whose hash is recorded.

## Acceptance
- State where the baseline database is stored and how to record its hash and source commit.
- Explain how to create a branch, run tests, commit, push, compare an incoming database with the baseline and generated candidate, resolve conflicts, and roll back.
- Keep only verified work on the shared branch. Push successful commits before another computer uses them as its starting point.
- List the user-interface and business workflows that still require a manual smoke test.

## Completion notes — 2026-09-19T12:56:22.138Z

Created a read-only sample baseline in the shared test folder and recorded its SHA-256 hash, byte count, application version, source commit, and matching manifest. Documented the Git workflow, the three-way database comparison, conflict handling, rollback, and required manual tests.
