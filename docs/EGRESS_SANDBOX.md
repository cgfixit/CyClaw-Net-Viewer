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
OS sandbox blocks all connections. UDP is covered separately in
`tests/live_listen.rs`: a connected UDP socket reports its peer, an
unconnected one reports none, and both have Unknown direction. The same
file checks IPv6 loopback and an IPv4 client on an IPv6 listener. No CyClaw
AI runtime is required.

## Watch and process-tree fixtures

`tests/watch.rs` drives the real binary in `--watch` mode with the same kind
of native Bash loopback fixture, and is part of `cargo test --locked`. Run
it alone with:

```bash
cargo test --locked --test watch
```

A pass requires:

1. A watch filtered to an idle shell reports its `baseline`, then exactly
   one `new` and one `closed` event for the connection the shell opens and
   closes, with the same PID and kernel start time in both.
2. SIGTERM ends an unbounded watch with an `end` record (`interrupted`),
   exit status 0, and nothing on stderr.
3. With `--tree` on a root shell, a connection held by its child is in the
   baseline and attributed to the child. After the root exits and launchd
   adopts the child, a second connection from that child is still reported,
   with the identity it had before. When the child exits the watch ends by
   itself with `scope_exited`.
4. A one-shot `--cli --tree` snapshot of the test process contains that
   process's own loopback connection.
5. The record for a connection names the local process on the other end:
   the listener while the connection is still queued, the accepted socket's
   owner afterwards, marked out of scope when it is outside the tree. A
   `--cli --peers` snapshot shows the same in its `Peer` column.
6. Invalid watch arguments and an unidentifiable `--tree` PID exit 2 before
   anything is sampled, and a reader that closes the pipe ends the watch
   without a panic.
7. With `--policy`, a tree watch of a shell that connects to a loopback
   listener exits 0 when a rule permits that peer, and exits 1 with exactly
   one `violation` record when none does. Endpoint records carry
   `remote_class`, `local_class` and `exposed`.
8. A bounded policy watch stopped by SIGTERM exits 2 with `interrupted`,
   and a policy file that does not parse exits 2 before anything is sampled.
9. Run as an ordinary user, an unscoped watch counts processes it cannot
   read, and a tree watch of a shell that runs `sudo -n /bin/sleep` reports
   that descendant as `unreadable` and ends `incomplete` (exit 2). Both
   skip themselves under root, and the second skips without passwordless
   `sudo`.

These fixtures show that the stream reports what a sample can see. They do
not measure how short a connection can be and still be caught.
