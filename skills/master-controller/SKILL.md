---
name: master-controller
description: Use the Master-Controller PowerShell interface to execute PowerShell, CMD, or batch scripts through a paired worker runner, collect the returned output, or provide the worker runner for deployment. Apply when execution must go through Master-Controller.ps1 or the paired runner must be installed; do not use for ordinary local shell execution.
---

# Master Controller

Use [scripts/Master-Controller.ps1](scripts/Master-Controller.ps1) as the command-line interface. Resolve it to an absolute path before invocation. The canonical paired runner is [scripts/Worker-Runner.ps1](scripts/Worker-Runner.ps1). Do not explain or modify either script unless the user explicitly asks.

To submit code, the runner directory must already exist and its paired worker runner must be active. If the user has not supplied the runner directory and the current context does not identify it, ask for the path. Use this interface only for actions the user has authorized on the runner's host.

## Present or deploy the worker runner

When the user asks for the worker runner source or needs to install it on another machine, read `scripts/Worker-Runner.ps1` in full. Present its exact contents in a PowerShell code block for copy-paste. Do not reconstruct it from memory or silently alter it.

Tell the user to save it as `worker-runner.ps1`, change to the directory it should watch, and run it from there. It watches its current directory for `code-to-run.ps1`, `code-to-run.cmd`, or `code-to-run.bat`.

## Run a script

Pass an existing `.ps1`, `.cmd`, or `.bat` file through `-ScriptPath`. Use `-RunnerDirectory` for the paired runner directory. `-TimeoutSeconds` is optional and defaults to 120 seconds.

```powershell
& '<absolute-skill-path>\scripts\Master-Controller.ps1' `
    -ScriptPath 'C:\path\to\script.ps1' `
    -RunnerDirectory 'C:\it\temp' `
    -TimeoutSeconds 120
```

Use the same command for CMD code with a `.cmd` or `.bat` file. If the user supplies code instead of a file, save it in a task-scoped file with the correct extension first. Do not write it directly into the runner directory.

Run only one request at a time for each runner directory. Choose a longer timeout when the requested operation may exceed two minutes.

## Interpret the result

Wait for the controller to exit. Its standard output is the requested script's returned text. Inspect that text for errors before deciding that the operation succeeded because the controller does not return the requested script's process exit code separately.

If the controller exits with an error or times out, preserve the exact error and do not claim success. A timeout does not cancel the submitted script, so do not resubmit it automatically.
