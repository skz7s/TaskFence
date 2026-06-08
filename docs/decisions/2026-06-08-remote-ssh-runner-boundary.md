# Remote SSH Runner Boundary

## Context

Phase 5 of the remaining capability plan adds the first live remote runner
backend. Generic SSH can start a command on a remote host and capture process
output, but it does not by itself create a filesystem sandbox, hide remote host
secrets, enforce disabled/default-deny network policy, enforce domain
allowlists, mount the local TaskFence gateway spool, or transport remote file
diffs back into local evidence.

## Decision

TaskFence supports `sandbox.type: remote_ssh` only as an explicitly declared
operator-confined backend. A task must provide `sandbox.ssh.host`, an absolute
remote `workspace`, absolute local `identity_file`, absolute local
`known_hosts_file`, `isolated_workspace: true`, `isolated_secrets: true`,
`terminates_remote_processes: true`, `network_policy: uncontrolled_allow`,
`permissions.network.default: allow`, no domain allowlist, no host environment
allowlist, no local gateway spool/listener tools, and
`audit.capture.file_diff: false`.

The SSH runner invokes the host `ssh` executable with batch mode, strict host
key checking, identity-only authentication, no SSH agent forwarding, and safe
shell quoting around the remote `cd <workspace> && exec <agent command>` line.
It captures stdout, stderr, exit code, and local timeout evidence. Kubernetes,
microVM, managed cloud, and unsupported sandbox types remain fail-closed
capability contracts.

## Consequences

This gives operators a real remote execution path without claiming isolation
that generic SSH cannot prove. The remote host or account is responsible for
the workspace and secret boundary. Tasks needing disabled/default-deny network
control, domain allowlists, gateway-mounted tool mediation, host env forwarding,
or remote file diff transport must use another runner or wait for a backend
that can enforce those controls.

## Validation Or Rollback Notes

Validation is covered by targeted config, runner, CLI, and core tests plus the
Phase 5 validation command. Roll back by removing the `RemoteSshRunner`
dispatch and returning `remote_ssh` to the unsupported capability report path;
existing Docker behavior remains independent.
