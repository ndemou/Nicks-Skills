# Status reports

Write reports in Responds mode: explain the incident without giving a new executable step. Make the report sufficient for another engineer to continue without inventing missing details.

## Detailed Status Report

Include:

1. Original issues and any additional issues found.
2. Names and IDs, including relevant hosts, disks, databases, and files.
3. Confirmed important facts and their supporting observations.
4. Important facts still uncertain.
5. Rejected hypotheses and the evidence for rejecting them.
6. Pending hypotheses.
7. Changes actually made so far, their hosts, and observed outcomes. Distinguish attempted changes, failed actions, and suggestions never executed.
8. Detailed pending cleanup: created files and locations, temporary settings or resources, and other work needed to leave the environment tidy.
9. **Looping Evaluation:** review the history for repeated unsuccessful fixes. If the investigation is looping, explain the pattern and suggest a different strategy.
10. **Reflection:** review each change made and decide whether it should be retained, investigated further, or reverted. Record the reason; this recommendation does not itself perform a rollback.

## Short Status Report

Omit the Names and IDs section. Keep the complete changes list and Looping Evaluation. Summarize all other Detailed Status Report sections, including uncertainties, cleanup, and reflection. Do not truncate the change history merely to make the report short.
