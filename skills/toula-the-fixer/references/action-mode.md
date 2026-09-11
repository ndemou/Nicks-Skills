# Action mode

## Step design

- Keep fix steps short and atomic. Make one change at a time unless combining changes is safe and necessary. Verify the result before the next dependent change.
- Target one host per step. For the same human-run action across several hosts, record completed hosts and use this reminder with actual names: `Multi-host action done on HOST1, HOST2; please repeat on: **HOST3**, HOST4`. Never ask the engineer to reuse a block whose change guard still names a different host; provide the newly bound block when needed.
- Prefer CLI actions. Request GUI actions or screenshots only when necessary.
- Apply the command and file rules in [command-delivery.md](command-delivery.md) to both direct execution and commands delivered to the engineer.
- A proposed repair needs a reason supported by evidence and an observable success check. If the issue is resolved, verify the original symptom and review pending cleanup and temporary changes before closing the incident.

## Risk and effect

Assess the whole step, including resource load and potential interruption. Use one risk and one effect label. A read-only action is not automatically risk-free.

| Risk | Meaning |
| --- | --- |
| 🟩 Zero | Routine observation with no material expected operational impact. |
| 🟨🟨 Low | Small, bounded operational risk with straightforward recovery. |
| 🟧🟧🟧 Moderate | Meaningful service impact or recovery effort is possible. |
| 🟥🟥🟥🟥 High | Substantial outage, data loss, or difficult recovery is possible. |

| Effect | Meaning |
| --- | --- |
| Read-Only | No intended persistent state change. |
| Prepare | Only preparation such as a backup or snapshot. |
| Rollback | Only reversal of earlier changes. |
| Change | A repair/configuration change, or a mixed step that includes one. |

Prepare and Rollback also mutate state: apply the same host guard, authorization, and file rules. If a step mixes Read-Only commands with a mutation, classify the step by its mutation. Split mixed Prepare/Rollback work when possible; otherwise use Change.

Risk labels describe the action; they do not grant authorization. For an action outside the current authorized scope, explain the specific change and impact and obtain the needed decision before execution.

## Processing results: precedence matters

1. **Check host identity first.** Read the `Host: ...` output and compare it with the intended host. For direct execution, also verify the actual tool connection or target. If identity is missing or ambiguous, establish it before drawing host-specific conclusions or changing state.
   - If diagnostics came from a confirmed wrong host, say: “You collected diagnostics from the wrong computer; please act on NAME”, replacing NAME with the correct computer name, then include the full State. Omit routine interpretation and do not incorporate those results as facts about the intended host.
   - If a change ran on the wrong host, report what may have changed, the evidence, likely risks, and suggested mitigations in detail. If the assistant performed it, own that mistake explicitly. Assess partial effects even if the command failed; do not let another formatting rule hide the incident.
2. **Interpret the result.** Be brief for expected outcomes; explain anomalies or decisive findings in more detail. Separate observations from hypotheses and uncertainty.
3. **Correct faulty commands.** If the assistant's command had a syntax or logic error and there are no unresolved mutation effects, provide `Hopefully Fixed Code` and the corrected paste-ready block, retaining its host guard and framing, plus the full State. Omit routine explanation. For a command that may have partially changed state, assess the effects first and avoid blindly repeating it. Do not treat command failure as evidence against the underlying hypothesis.
4. **Continue with the next action**, unless the response is limited to correcting the host or code above, the user has switched modes, or a decision/input is pending. Human execution waits for the returned result; direct execution follows the pacing defined in SKILL.md.

## Compact response with full State

Keep the explanation and action compact, and include the complete State in every Action-mode response, including host and code corrections. Omit optional warnings or findings when they do not apply:

1. **Findings:** briefly interpret the previous result. Use a Unicode marker such as `⚠` or `✓` for a consequential finding, without overstating certainty.
2. **Action — step ID, target host, purpose:** one risk/effect banner, then the command block or a concise account of a directly executed action and its result. Example banner: `🟨🟨 Low | Change`.
3. **Warnings / assumptions:** include only relevant items. Highlight assumptions with material consequences. Warn when any command is expected to run for more than 30 minutes; justify intentional high-volume error suppression here. Give warnings before the command or tool execution they concern.
4. **State:** always include the full current record with the subsections below. Retain earlier relevant facts and attempts, not only changes since the last message.

State subsections: **Hosts**, **Hypotheses Rejected**, **Hypotheses Validated**, **Important Facts Learned**, **Fixes Tried**, and **Pending Cleanup Actions**. Hosts is a comma-separated list; the other lists are numbered, with fewer than 40 words per item. Use “None yet” or “None pending” for an empty subsection. Distinguish attempted fixes from verified changes and outcomes. A hypothesis is validated only as strongly as the evidence supports.

Do not repeat the risk banner after the action or append the original P.S. by default. In direct execution, interpret results between steps and include full State in user-facing action updates; this record requirement does not add a user-approval pause after each tool call.

For a Short or Detailed Status Report, follow [status-reports.md](status-reports.md), including its complete change history and looping evaluation.
