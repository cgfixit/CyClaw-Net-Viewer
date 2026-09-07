---
name: emulate-egress-sandbox
description: Emulate native Bash TCP traffic and grade its visibility and cleanup through CyClaw-Net-Viewer on macOS. Use for NetViewer egress smoke tests, sandbox observation, or a controlled LAN connection check.
---

# Emulate egress with NetViewer

Locate the CyClaw-Net-Viewer checkout (Cargo package `netboard`), read its
AGENTS.md, and record HEAD, macOS architecture, and `rustc --version`.
Run commands from that checkout, including when this skill is installed elsewhere.

Run `./scripts/emulate-egress-sandbox.sh` on macOS with native `/bin/bash`.
The script builds with Cargo.lock and runs `tests/egress_sandbox.rs`. It starts
a loopback listener and a Bash `/dev/tcp` client, then requires an empty idle
baseline, an ESTABLISHED outgoing socket attributed to the child PID, matching
numeric CSV from the real NetViewer CLI, and disappearance after descriptor
closure while the same child remains alive. Timeouts or missing observations
fail; unsupported platforms fail explicitly. The harness cleans up its own
children on success and assertion failure. No root or DNS is needed.

For an explicitly requested off-machine exercise, use an operator-controlled
IPv4 listener that accepts a TCP connection and keeps it open for at least
30 seconds. Set `NETVIEWER_EGRESS_PEER=192.168.1.20:45678` (replace with the
actual authorized peer) for that invocation only. This sends the fixed line
`netviewer-egress-test`; never send credentials or real application data.
Do not substitute a public service. See [the harness guide](../../../docs/EGRESS_SANDBOX.md)
in the repository for listener setup and interpretation.

Report PASS/FAIL, exact compiler/architecture and revision, loopback versus
off-machine mode, and which assertions ran. Windows/Linux source inspection
is not a runtime pass. Loopback proves observation, not actual off-machine
egress. NetViewer is a socket viewer, not a firewall, packet capture, or OS
sandbox: neither an empty snapshot nor this test proves all egress is blocked.
Never weaken socket visibility checks or disable system protections to pass.
