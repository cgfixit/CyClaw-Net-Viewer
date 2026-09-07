# Egress observation harness

Run from the repository root on macOS with native Bash and rustup:

```bash
./scripts/emulate-egress-sandbox.sh
RUSTUP_TOOLCHAIN=stable ./scripts/emulate-egress-sandbox.sh
```

The harness uses the real `netboard` collector and CLI. A native `/bin/bash`
process opens `/dev/tcp` to an ephemeral loopback listener, sends the fixed
line `netviewer-egress-test`, and holds the descriptor open until instructed
to close it. This needs no root, external service, DNS, Python, or firewall
changes. It is also part of ordinary `cargo test --locked` and macOS CI.

A pass requires:

1. Numeric CLI CSV shows no sockets for the idle Bash child.
2. The collector sees its exact local/remote tuple, PID, TCP4, Out direction,
   and ESTABLISHED state; the listener receives the synthetic payload.
3. Both established-only and all-endpoint CLI modes report that same tuple.
4. Closing the descriptor removes it from the collector and CLI while the
   Bash owner remains alive; the child then exits successfully.

Each observation and subprocess wait is bounded to 15 seconds. Assertion
failure unwinds a child guard that kills/reaps only processes created by
the harness; Rust owns and closes the loopback sockets. No socket exports
or process paths are written to disk or printed in success output. A failure
returns nonzero, including on unsupported platforms. Keep failure logs private
if they contain local diagnostic metadata. Interrupting Cargo externally is
not an assertion unwind; normal pipe closure also releases the Bash fixture.

## Optional controlled LAN exercise

On another machine you control, start a listener that keeps its accepted
connection open. For example, with macOS native netcat:

```bash
sleep 60 | nc -l 45678
```

Then use its numeric IPv4 address on the Mac running NetViewer:

```bash
NETVIEWER_EGRESS_PEER=192.168.1.20:45678 ./scripts/emulate-egress-sandbox.sh
```

Replace the example address with the authorized peer. The harness validates
IPv4 and a nonzero port; it does not determine who owns the destination.
Use only a controlled host, and stop its listener afterward. LAN mode checks
the outgoing connection and cleanup; receiving the payload is automated only
for the default loopback fixture. A refused or prematurely closed connection
fails instead of silently switching to loopback. The normal CI matrix leaves
this environment variable unset.

## What the result establishes

The default test emulates outgoing traffic on loopback. It does not test a
physical network interface. LAN mode can exercise an off-machine route, but
NetViewer remains a socket observer, not an egress filter or packet capture.
It cannot prove absence of short-lived traffic between snapshots or that an
OS sandbox blocks all connections. UDP remote peers are unavailable through
the current Darwin socket library and are covered separately as Unknown
direction in `tests/live_listen.rs`. No CyClaw AI runtime is required.
