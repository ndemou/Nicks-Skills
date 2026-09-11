# Command delivery

Apply these rules to commands Toula executes and commands supplied to the on-site engineer. Toula may run checks and changes already authorized by the user. Do not introduce an extra confirmation solely because Toula executes an otherwise authorized action.

## Identify the actual target

- Distinguish the execution or management host from the affected target. A workstation running an SSH session, PowerShell remoting, or a management client is not necessarily the host being inspected or changed.
- Obtain identity from the actual target and use it in the step record and returned `Host:` output. Label management-host identity separately when relevant.
- Before every mutation, compare the actual target identity with the verified expected identity. Fail visibly without making changes if identity is unavailable, ambiguous, or different.
- Put the identity guard in the context that performs the mutation: inside a remote session when changing that remote host, for example. A guard checking only the local management computer is insufficient.
- Use an additional verified target identifier when a short hostname cannot uniquely identify the intended machine. Avoid hostname normalization that could make two targets appear identical.
- Guard all mutations, including preparation, backups, snapshots, and rollback. Their effect label does not exempt them from the guard.

## Choose commands for the discovered platform

- Give Windows, macOS, and Linux equal consideration. Select commands from observed OS, shell, and tool availability rather than assuming a platform.
- Default to Windows PowerShell 5.1 on Windows and Bash on macOS/Linux. Use PowerShell 7, CMD, or another shell only when needed and identify the requirement.
- PowerShell 5.1 code must not use `?.`, `??`, the `?:` ternary operator, `&&`, `||`, or `ForEach-Object -Parallel`.
- macOS system Bash may be old. Do not assume Bash 4+ features or GNU versions of standard utilities; account for BSD/GNU differences using supported native options or verified alternatives.
- Prefer CLI output directly in the terminal.
- Prefer simple, readable commands that an IT administrator with limited shell experience can understand.
- Keep relevant output manageable with targeted queries, filters, and tail limits. Do not filter away evidence needed to detect a failure or evaluate a hypothesis.

## Deliver complete, atomic steps

- Supply paste-ready blocks containing actual known inputs. Put user-supplied values in clearly named variables near the top when useful, with correct shell quoting.
- Never deliver unresolved placeholders or interactive input requests such as `Read-Host`. Ask for missing values first when discovery cannot readily obtain them.
- Symbolic names in this reference are authoring guidance only; replace them before executing or delivering commands.
- Keep user-facing blocks to approximately 20 lines, excluding minimal identity/framing lines if necessary. Do not hide complexity in dense one-liners to meet that limit.
- Split before or after an operation that requires human verification. Wait for that verification before dependent work continues.
- If a command is expected to run or require a wait longer than approximately 30 seconds, deliver it as a separate step and wait for its result before the next dependent step. Use a short sleep only when a known wait below that threshold is needed.
- Warn when any command is expected to take more than 30 minutes.
- Before execution, check the command's logic, platform compatibility, target, and supplied values. Do not run incident commands merely to test a template.

## Frame code and surface failures

- Begin each block with a bright green preamble containing the step ID, actual target hostname, current date/time, and a brief purpose. Use a clear separator so pasted output remains identifiable.
- End each block with a green footer containing the same step ID, actual target hostname, and current date/time, explicitly marking the end of the step.
- On Windows use PowerShell's green console text; in suitable macOS/Linux terminals use ANSI bright green and reset the color afterward. Preserve the textual fields if the output surface cannot render color.
- Print the actual identity as `Host: <actual hostname>` with the value resolved at execution time; never present the expected name as if it were observed.
- A footer means the block ended, not that it succeeded. Keep failures visible and do not print an unverified success claim.
- Do not silently suppress errors except to reduce a high volume of expected errors. Explain that suppression in the step's Warnings section and preserve unexpected failures.
- If a catch-all is necessary, print full exception details. On shells without structured exceptions, preserve useful error output and the failing operation's exit status.
- Process host mismatches and code errors using the precedence in [action-mode.md](action-mode.md), including its treatment of partial changes.

## Files and cleanup

- Prefer terminal output to creating files.
- Files expected to be strictly under 1 MB may be created in the actual OS temporary directory without a separate file-creation question: `$Env:TEMP` on Windows, or the resolved OS temporary directory on macOS/Linux, commonly exposed through `TMPDIR`.
- Verify the chosen temporary directory exists and belongs to the intended execution context. For remote work, resolve it on the host where the file will be created.
- For files outside that directory, files of 1 MB or more, or files whose expected size cannot be bounded below 1 MB, ask for consent with the expected size or range and save location unless that file creation is already specifically authorized.
- Existing authorization persists; do not ask again for the same agreed file creation. Do not infer permission for arbitrary large files from general permission to troubleshoot.
- Record temporary artifacts requiring removal in the cleanup list, especially large files, with their actual host and location. Keep backups or recovery artifacts until their retention or removal is appropriate for the agreed troubleshooting work.
