# CyClaw-Net-Viewer

[![CI](https://github.com/cgfixit/Mac-NetViewer-EZview/actions/workflows/ci.yml/badge.svg)](https://github.com/cgfixit/Mac-NetViewer-EZview/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/github/license/cgfixit/Mac-NetViewer-EZview)](LICENSE)
[![Rust 1.85.0](https://img.shields.io/badge/rustc-1.85.0-orange.svg)](rust-toolchain.toml)
[![macOS 12+](https://img.shields.io/badge/macOS-12%2B-black.svg)](docs/BUILD.md)

Live TCP/UDP endpoint table for macOS. Process, PID, protocol, direction, local and remote addresses, TCP state. Inspired by Sysinternals TCPView. Not affiliated with Microsoft.

Crate and binary name: `netboard`. Window and bundle name: **CyClaw-Net-Viewer**. Repo: [cgfixit/Mac-NetViewer-EZview](https://github.com/cgfixit/Mac-NetViewer-EZview).

## What it does

- Lists TCP and UDP sockets this Mac will admit through libproc, including IPv4 and IPv6.
- Refreshes on a timer (default 1s). New outgoing rows are green. New incoming or listen rows are blue. State changes are yellow. Closed rows linger red for two ticks. Off-box remotes stay orange while they exist.
- Resolves names in the background. Cells prefer `hostname (IPv4):port` when PTR and A records exist. IPv6 sockets stay in the table.
- Ships a double-click `.app` and a Tcpvcon-style CLI in the same binary.

## What it does not

- ICMP / ping. Those are not TCP or UDP sockets.
- Firewall or packet capture.
- Windows-style TCB delete. **Close Connection** confirms `SIGTERM` of the owning process.

## Colors

| Color | Meaning |
| --- | --- |
| Green | New outgoing endpoint |
| Blue | New incoming endpoint or new LISTEN |
| Yellow | Same 4-tuple, TCP state changed |
| Red | Endpoint gone (two refresh ticks, then drop) |
| Orange | Stable TCP/UDP to an off-box peer (not loopback, not `*`) |

Turn on **Off-box only** when watching CyClaw telemetry-kill. Event colors still win over orange.

## Run the bundled app

No Rust required.

```bash
git clone https://github.com/cgfixit/Mac-NetViewer-EZview.git
open Mac-NetViewer-EZview/dist/CyClaw-Net-Viewer.app
```

The binary is ad-hoc signed, not notarized. First launch: Finder, Right-click `CyClaw-Net-Viewer.app`, Open. Do not App-Sandbox the bundle. That hides other processes' sockets.

Requires macOS 12 or newer, Intel or Apple Silicon. The checked-in app is a universal `arm64` + `x86_64` Mach-O when both targets built.

## Build from source

Rust **1.85.0** is pinned (`rust-toolchain.toml`). Homebrew rustc is newer and is not the pin.

```bash
export PATH="$HOME/.cargo/bin:$PATH"
rustup toolchain install 1.85.0
cargo test
cargo run
cargo run -- --cli -n -a
./scripts/make-app.sh          # dist/CyClaw-Net-Viewer.app
./scripts/make-app.sh --dmg    # dist/CyClaw-Net-Viewer.dmg
```

See [docs/BUILD.md](docs/BUILD.md).

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
