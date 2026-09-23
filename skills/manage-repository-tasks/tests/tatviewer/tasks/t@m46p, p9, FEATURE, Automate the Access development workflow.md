<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:28.695Z","completions":[]} -->

## Goal
Automate the full development workflow for an Access application. The workflow must export source, merge changes from another database, build and test a candidate, apply schema changes, package the result, and synchronize source through Git and the shared exchange folder.

## Scope
This parent task groups all Access workflow automation. Its child tasks cover reliable source export and import, reproducible builds, automated tests, manifests, safe schema changes, package exchange, the temporary baseline workflow, and publication of the shared Git base.

## Done when
- Every descendant task is complete.
- The tools can build the same candidate from Git source or an incoming Access database file without changing the original file or trusted baseline.
- Automated checks reject incomplete exports, unsafe imports, unexpected schema or object changes, and failed tests.
- A successful candidate can be committed, pushed, and packaged with enough build and test information to trace its origin.
