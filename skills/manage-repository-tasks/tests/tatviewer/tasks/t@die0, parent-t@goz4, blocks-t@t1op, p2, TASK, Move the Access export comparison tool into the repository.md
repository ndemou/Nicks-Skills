<!-- tat-metadata: {"version":2,"created_at":"2026-09-19T09:50:29.021Z","completions":[]} -->

## Goal
Move `Compare-AppTextExports.ps1` into the sample application's repository so it is versioned and maintained with the other build tools.

## Acceptance
- The script is stored in the repository's `scripts` directory.
- The existing `Vba` and `All` modes still work, including the rules that ignore known Access-generated noise.
- Tests distinguish real source changes from known noise-only changes.
- Documentation runs the script by its repository-relative path.
