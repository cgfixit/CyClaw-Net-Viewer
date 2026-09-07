# CyClaw-Net-Viewer

[![Rust 1.85.0](https://img.shields.io/badge/rustc-1.85.0-orange.svg)](rust-toolchain.toml)
[![macOS 12+](https://img.shields.io/badge/macOS-12%2B-black.svg)](docs/BUILD.md)
[![License: MIT](https://img.shields.io/github/license/cgfixit/CyClaw-Net-Viewer)](LICENSE)

Live TCP/UDP endpoint table for macOS. Process, PID, protocol, direction, local and remote addresses, TCP state. Name resolution is off by default in the GUI; enabling **Resolve names**, or running the CLI without `-n`, sends PTR queries through the system resolver. Inspired by Sysinternals TCPView. Not affiliated with Microsoft.

Binary/crate: `netboard`. Product: **CyClaw-Net-Viewer**. The GitHub repository is `cgfixit/CyClaw-Net-Viewer`.

## App screenshot

<img src="https://github.com/cgfixit/CyClaw-Net-Viewer/blob/main/docs/image.jpg">

| Process | PID | Proto | Dir | Local | Remote | State | Path |
| --- | ---: | --- | --- | --- | --- | --- | --- |
| nginx | 1042 | TCP4 | Listen | `*:443` | `*:0` | LISTEN | /usr/sbin/nginx |
| curl | 2108 | TCP4 | Out | `127.0.0.1:54321` | `127.0.0.1:443` | ESTABLISHED | /usr/bin/curl |
| mDNSResponder | 301 | UDP4 | Unknown | `*:5353` | `*:0` |  | /usr/sbin/mDNSResponder |

## What it does

- Lists TCP and UDP sockets this Mac will admit through libproc, including IPv4 and IPv6.
- Refreshes on a timer (default 1s). New outgoing rows are green. New incoming or listen rows are blue. State changes are yellow. Closed rows linger red on the first two refreshes where they are absent, then disappear on the third. Off-box remotes stay orange while they exist.
- Address cells retain the observed socket IP and port, with an optional `hostname (IP):port` label when **Resolve names** is on. Native IPv6 stays bracketed; IPv4-mapped addresses unwrap to IPv4. Other connections and DNS answers never replace the observed address. Name resolution is off by default.
- Builds a double-click `.app` and a Tcpvcon-style CLI in the same binary.

UDP direction is **Unknown**: the socket library exposes local bindings but
omits remote peers, including for connected UDP. Unknown rows get no new-in/out
flash and remain visible when **Show listeners** is off. TCP direction is a
listening-port heuristic; see [Direction](docs/DESIGN.md#direction).

## What it does not

- ICMP / ping. Those are not TCP or UDP sockets.
- Firewall or packet capture.
- Windows-style TCB delete. **Terminate process** confirms `SIGTERM` of the owning process.

## Colors

| Color | Meaning |
| --- | --- |
| Green | New outgoing endpoint |
| Blue | New incoming endpoint or new LISTEN |
| Yellow | Same 4-tuple, TCP state changed |
| Red | Endpoint gone (two refresh ticks, then drop) |
| Orange | Stable TCP/UDP to an off-box peer (not loopback, not `*`) |

Turn on **Off-box only** when watching CyClaw telemetry-kill. Event colors still win over orange.

## Get the app

Download `CyClaw-Net-Viewer.zip` and `CyClaw-Net-Viewer.zip.sha256` from the
latest [GitHub Release](https://github.com/cgfixit/CyClaw-Net-Viewer/releases/latest).
No Rust is required to run a downloaded app. Build from source (below) if
you prefer not to use a prebuilt binary.

Verify and unpack on macOS:

```bash
shasum -a 256 -c CyClaw-Net-Viewer.zip.sha256
ditto -x -k CyClaw-Net-Viewer.zip .
codesign --verify --deep --strict CyClaw-Net-Viewer.app
open CyClaw-Net-Viewer.app
```

The binary is ad-hoc signed, not notarized. Gatekeeper may require an
explicit per-app approval; follow the options provided by your macOS
version. Do not disable Gatekeeper system-wide. Ad-hoc signatures and
checksums do not identify a trusted publisher. Do not App-Sandbox the
bundle: that hides other processes' sockets.

Requires macOS 12 or newer, Intel or Apple Silicon. Release and CI
bundles verify universal `arm64` + `x86_64` builds. Local builds can
fall back to the host architecture. Generated apps are no longer
checked into Git, so old binaries cannot silently accompany new
source fixes.

Maintainers who need a specific unreleased commit may download the
14-day [Bundle workflow](https://github.com/cgfixit/CyClaw-Net-Viewer/actions/workflows/bundle.yml)
artifact themselves (GitHub sign-in required). Prefer the reviewed run for
the exact commit you need. Those zips are not the public distribution
channel. PR artifacts contain proposed changes and are for review only.

## Build from source

Rust **1.85.0** is the minimum supported version and remains the reproducible
default in `rust-toolchain.toml`. CI also checks **current stable Rust** on
Apple Silicon and Intel Macs: formatting, strict Clippy, tests (including
native Bash traffic observed through NetViewer), and locked release builds.
Weekly runs detect compatibility changes as stable advances. See the
[CI results](https://github.com/cgfixit/CyClaw-Net-Viewer/actions/workflows/ci.yml)
for exact compiler versions and revisions; this is macOS support, not a
Windows/Linux port or an automated interactive GUI test.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
git clone https://github.com/cgfixit/CyClaw-Net-Viewer.git CyClaw-Net-Viewer
cd CyClaw-Net-Viewer
rustup toolchain install 1.85.0
./scripts/check.sh
cargo run --locked
cargo run --locked -- --cli -n -a
./scripts/make-app.sh          # dist/CyClaw-Net-Viewer.app
./scripts/make-app.sh --dmg    # dist/CyClaw-Net-Viewer.dmg
```

See [docs/BUILD.md](docs/BUILD.md).

To verify with the current compiler without changing the repository pin:

```bash
rustup update stable
RUSTUP_TOOLCHAIN=stable ./scripts/check.sh
cargo +stable build --release --locked
```

## Egress verification and agent skills

Run `./scripts/emulate-egress-sandbox.sh` on macOS for a bounded PASS/FAIL
test of Bash TCP traffic, PID attribution in the collector and numeric CLI,
and endpoint removal after closure. It uses loopback by default; see the
[egress harness guide](docs/EGRESS_SANDBOX.md) for an optional controlled LAN
exercise and the limits of socket observation.

Three shared skills live in `.agents/skills`: `emulate-egress-sandbox`,
`verify-netviewer-rust`, and `verify-netviewer-bundle`. Open this repository
as your agent workspace to discover them, or copy a skill directory into
your personal skills directory. Their scripts and guides stay in this repo.

## Privacy and exports

Name resolution is off by default in the GUI. Enabling **Resolve names**
uses the system resolver for reverse DNS labels, without an additional
forward lookup of the returned name. Keep it off, or use CLI `-n`, to avoid new
viewer-initiated lookups.
Already queued GUI lookups may finish. The app has no analytics service.

**Save CSV** writes the visible rows into the current working directory
with owner-only permissions and refuses to overwrite an existing file or
symlink. If two saves occur in the same second, wait a second and retry.
CLI `-c` writes to stdout; shell redirection controls its file permissions.
Formula-like text is prefixed with an apostrophe in both export paths;
import untrusted CSV columns as text when using a spreadsheet. Exports
contain process names, paths, and network addresses: treat them as private.

## Project layout and contributing

`src/` contains the Rust GUI, CLI, socket snapshots, DNS, diffing, and safety
helpers. `tests/` holds integration tests, `scripts/` holds validation and
packaging, `docs/` holds guides, and `tools/` holds optional PDF generators.

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup and checks, [AGENTS.md](AGENTS.md)
for agent guidance, and [.codex/README.md](.codex/README.md) for Codex setup.

## CLI

```text
netboard --cli [-a] [-c] [-n] [process|pid]
```

| Flag | Effect |
| --- | --- |
| (none) | GUI |
| `--cli` | Snapshot to stdout. Default is ESTABLISHED TCP only |
| `-a` | All TCP and UDP endpoints |
| `-c` | CSV |
| `-n` | Numeric addresses, no reverse DNS |
| `process` or `pid` | Optional filter |

## Docs

- [User guide (PDF)](docs/CyClaw-Net-Viewer-User-Guide.pdf)
- [How it works (PDF)](docs/CyClaw-Net-Viewer-How-It-Works.pdf)
- [Controls](docs/CONTROLS.md)
- [Design](docs/DESIGN.md)
- [Known issues](docs/KNOWN_ISSUES.md)
- [Security](SECURITY.md)

## License

MIT. See [LICENSE](LICENSE).
