# TaskFence Report: `test-task`

## Summary

- Task ID: `test-task`
- Goal: Run test task
- Final status: Succeeded
- Runner exit: code 0
- Audit events: 4

## Task Input

- Task file: `/tmp/task.yaml`
- Workspace: `/workspace/repo`
- Container workspace: `/workspace`

## Agent and Model

- Agent kind: `Generic`
- Command: `npm`
- Args: `test`

## Policy Summary

- Allowed decisions: 1
- Approval-required decisions: 0
- Denied decisions: 0

## Sandbox Summary

- Sandbox kind: `Docker`
- Image: `taskfence/runner:latest`
- Network default: `Deny`

## Timeline

| Time | Event |
| --- | --- |
| 2024-02-03 4:05:00.0 +00:00:00 | task created: Run test task |
| 2024-02-03 4:06:00.0 +00:00:00 | policy decision allow: command npm test |
| 2024-02-03 4:07:00.0 +00:00:00 | runner exit code 0 |
| 2024-02-03 4:08:00.0 +00:00:00 | status changed to Succeeded |

## Commands

| Command | Decision | Reason |
| --- | --- | --- |
| npm test | allow | command matched allow rule |

## Tool Calls

None recorded.

## Approvals

None recorded.

## Denied Actions

None recorded.

## Network Destinations

None recorded.

## File Changes

- Diff artifact: `/tmp/taskfence-golden/task/diff.patch`
- dirty_before_run: true
- warning: workspace was dirty before the task; final diffs are not attributed exclusively to the agent

## Test Results

No structured test results recorded.

## Artifacts

| Kind | Path |
| --- | --- |
| task_dir | /tmp/taskfence-golden/task |
| resolved_task | /tmp/taskfence-golden/task/task.resolved.json |
| events | /tmp/taskfence-golden/task/events.jsonl |
| stdout | /tmp/taskfence-golden/task/stdout.log |
| stderr | /tmp/taskfence-golden/task/stderr.log |
| diff | /tmp/taskfence-golden/task/diff.patch |
| report | /tmp/taskfence-golden/task/report.md |

## Residual Risks

No residual risks were identified from structured evidence.
