# Policy and verdict

`netboard --cli --watch --policy FILE` checks every endpoint in the watched
scope against rules you write, and ends with a verdict and an exit code:

| Exit | Verdict | Meaning |
| --- | --- | --- |
| 0 | `pass` | Every sample succeeded, every process in scope was readable, and nothing seen broke a rule |
| 1 | `violation` | Something seen broke a rule |
| 2 | `incomplete` | Nothing seen broke a rule, but part of the scope or the time went unseen. Also every usage error, including a policy file that does not parse |

The codes match [yara-gate](https://github.com/cgfixit/yara-gate)'s (0 clean,
1 block, 2 incomplete), so one wrapper can gate on both. One difference:
yara-gate lets incomplete outrank findings, and this lets a violation
outrank incomplete. A connection seen is a fact; what went unseen can only
add to it. The `end` record still lists every incomplete reason.

`pass` means *no violation seen*, not *no violation*. A watch samples. A
connection that opens and closes between two samples is never seen; `end`
gives `max_gap_ms`, the longest unobserved interval. Use the shortest
`--interval` you can afford when the verdict gates something.

## The file

```text
netboard-policy 1
# Comments start with #, anywhere on a line.
allow loopback proto tcp port 11434
allow 192.168.1.1 proto udp port 53
allow 10.0.0.0/8 port 8000-8999
listen loopback
```

The first non-comment line must be `netboard-policy 1`. After it, each line
is one rule: a verb, a target, then optional `proto tcp|udp` and
`port N` or `port N-M` in either order.

| Verb | Permits | Target is matched against | `port` is |
| --- | --- | --- | --- |
| `allow` | a connection (a socket with a peer) | the peer's address | the peer's port |
| `listen` | a receiver: a TCP listener, or an unconnected UDP socket bound to a port | the local address | the local port |

A target is `any`, an address class, one address, or a CIDR network. IPv4
and IPv4-mapped IPv6 are the same address. `wildcard` (`0.0.0.0` or `::`) is
a `listen` target only.

Nothing else needs a rule: an unbound UDP socket or a TCP socket that
neither listens nor has a peer waits for nobody and talks to nobody.

The policy is default-deny. An empty policy makes every connection and every
receiver a violation. A file over 64 KiB, over 512 rules, not UTF-8, or not
a regular file is refused before anything is sampled, with exit 2 and a
message that names the line, never the line's text.

### Address classes

| Class | Ranges |
| --- | --- |
| `wildcard` | `0.0.0.0`, `::` |
| `loopback` | `127.0.0.0/8`, `::1` |
| `lan` | RFC 1918, `169.254.0.0/16`, `fe80::/10`, `fc00::/7` |
| `multicast` | `224.0.0.0/4`, `255.255.255.255`, `ff00::/8` |
| `public` | globally routable unicast, `2000::/3`, and the NAT64 prefix `64:ff9b::/96` |
| `other` | `100.64.0.0/10` (CGNAT and many VPN overlays), documentation, benchmarking and reserved ranges |

Classes describe address ranges, not routes. `lan` is usually the local
network but can be a VPN; a Tailscale peer in `100.64.0.0/10` is `other`, so
it never passes as LAN by accident. Every `endpoint` record carries
`remote_class` (null without a peer), `local_class`, and `exposed`: true for
a receiver bound to a wildcard or a non-loopback address, which another
machine can reach.

## Records

A violation is written once per endpoint, right after the `endpoint` record
that reported it:

```json
{"type":"violation","ts":"2026-11-17T03:04:07.004Z","breach":"connection","process":{"pid":2108,"start":"2026-11-17T03:04:04.512345Z","name":"my-agent","path":"/usr/local/bin/my-agent"},"proto":"tcp","local":{"ip":"192.0.2.10","port":54321},"remote":{"ip":"198.51.100.7","port":443},"state":"SYN_SENT","remote_class":"public","local_class":"lan"}
```

`breach` is `connection` or `listener`. A connection attempt counts: a
`SYN_SENT` socket to a peer no rule permits is a violation whether or not it
connects. A socket that changes state is checked again, so one that binds a
port and starts listening later is caught when it starts.

The `start` record gives `"policy":{"rules":N}`. The `end` record gives
`"policy":{"verdict":…,"violations":N,"incomplete":[…]}`, where the reasons
are:

- `failed_samples`: at least one sample failed, so an interval went unseen.
- `unreadable_processes`: a process in scope could not be read (below).
- `interrupted`: a watch with `--for` was stopped before its time was up.
  A watch without `--for` ends normally on Ctrl-C or SIGTERM, or when its
  tree exits.

Without `--policy` the `policy` fields are null and the exit code keeps its
old meaning: 0 unless a sample failed.

## Processes the watch cannot read

macOS lets a user read only their own processes' descriptors. Run as an
ordinary user, a watch of every process cannot see launchd, the window
server, or anything else another user owns, and before this release those
processes silently held no sockets. They now count: `baseline` and `end`
give `unreadable_processes`, and a scoped watch writes a `process` record
with `"event":"unreadable"` for each one in scope.

- **Unscoped**, without root, the count is never zero, so a verdict can
  never be `pass`. That is correct: the watch did not see the machine. Use
  `--tree`, or run as root.
- **`--tree`** counts any descendant it cannot read, such as a setuid
  program or a command run through `sudo`. It finds them through the kernel's
  child lists, which need no permission over the child, and follows their
  children too. They are never tracked members, because membership needs a
  readable start time, so their sockets are not in the stream.
- **A name filter** matches an unreadable process by name or by the last
  part of its executable path, since the name of another user's process may
  not be readable.

A tree that loses a descendant to a double fork (the child's parent exits
before a sample sees it) loses it without a count. See
[known issues](KNOWN_ISSUES.md).

## Using it

Watch a command from its first socket and gate on the verdict:

```bash
umask 077
netboard --cli --watch --interval 200 --tree $$ --policy agent.policy > run.jsonl & watcher=$!
until grep -q '"type":"baseline"' run.jsonl; do sleep 0.1; done
my-agent --task "$TASK"
kill -TERM "$watcher"; wait "$watcher"; verdict=$?
```

A watch without `--for` is not interrupted by that SIGTERM: it ends
normally, so `verdict` is 0, 1 or 2 as above. `grep '"type":"violation"'
run.jsonl` lists what broke the policy.

Starting points, not recommendations: [CG-agent-harness](examples/cg-agent-harness.policy)
and [CyClaw](examples/cyclaw.policy). Check their ports against your own
configuration before relying on them.

## What a verdict does not establish

- It only observes. Nothing is blocked, and no process is ever signalled.
- A connection to a permitted local peer is not proof that nothing left the
  machine: the peer may forward it. The `peer` field names that process; add
  it as another `--tree` root to hold it to the same policy.
- A connection shorter than the sampling interval can pass unseen.
- Process names and paths in violation records are chosen by whoever started
  the process. Treat them as untrusted text.
