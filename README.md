# CyClaw-Net-VieweR

[![CI](https://github.com/cgfixit/CyClaw-Net-Viewer/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/cgfixit/CyClaw-Net-Viewer/actions/workflows/ci.yml)
[![Rust 1.85.0](https://img.shields.io/badge/rustc-1.85.0-orange.svg)](rust-toolchain.toml)
[![macOS 12+](https://img.shields.io/badge/macOS-12%2B-black.svg)](docs/BUILD.md)

macOS TCP/UDP endpoint viewer for watching process egress—especially while
developing [CyClaw](https://github.com/cgfixit/CyClaw). Inspired by Sysinternals
TCPView. Not affiliated with Microsoft.

## App screenshot

<img src="https://github.com/cgfixit/CyClaw-Net-Viewer/blob/main/docs/image.jpg">

| Process | PID | Proto | Dir | Local | Remote | State | Path |
| --- | ---: | --- | --- | --- | --- | --- | --- |
| nginx | 1042 | TCP4 | Listen | `*:443` | `*:0` | LISTEN | /usr/sbin/nginx |
| curl | 2108 | TCP4 | Out | `127.0.0.1:54321` | `127.0.0.1:443` | ESTABLISHED | /usr/bin/curl |
| mDNSResponder | 301 | UDP4 | Unknown | `*:5353` | `*:0` |  | /usr/sbin/mDNSResponder |

## What it does

- Lists TCP and UDP sockets this Mac will admit through libproc, including IPv4 and IPv6. Columns: process, PID, protocol, direction, local and remote addresses, TCP state.
- Refreshes on a timer (default 1s) with the event colors below.
- Keeps the observed socket IP and port in every cell. Optional reverse DNS is an untrusted label only; it is off by default in the GUI. CLI `-n` stays numeric.
- Ships a double-click `.app` and a Tcpvcon-style CLI in the same `netboard` binary.

See [Direction](docs/DESIGN.md#direction). Refresh,
deletion, and address rules: [Design](docs/DESIGN.md).

## Colors

| Color | Meaning |
| --- | --- |
| Green | New outgoing endpoint |
| Blue | New incoming endpoint or new LISTEN |
| Yellow | Same 4-tuple, TCP state changed |
| Red | Endpoint gone (two refresh ticks, then drop) |
| Orange | Stable TCP/UDP to an off-box peer (not loopback, not `*`) |

Turn on **Off-box only** when watching CyClaw telemetry-kill. Event colors still win over orange.

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
fall back to the host architecture.

Maintainers who need a specific unreleased commit may download the
14-day [Bundle workflow](https://github.com/cgfixit/CyClaw-Net-Viewer/actions/workflows/bundle.yml)
artifact themselves (GitHub sign-in required). Prefer the reviewed run for
the exact commit you need. Those zips are not the public distribution
channel. PR artifacts contain proposed changes and are for review only.

## Build from source

Rust **1.85.0** is the minimum supported version (`rust-toolchain.toml`).
CI also checks current stable on Apple Silicon and Intel. Clone the default
`main` branch:

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

See [docs/BUILD.md](docs/BUILD.md). To verify with the current compiler without
changing the repository pin:

```bash
rustup update stable
RUSTUP_TOOLCHAIN=stable ./scripts/check.sh
cargo +stable build --release --locked
```

## Verify

On macOS, `./scripts/check.sh` runs the same Rust checks as CI: `rustfmt`,
Clippy (`-D warnings`), and `cargo test --locked` (including the native Bash
egress fixture). For a focused observation and cleanup check:

```bash
./scripts/emulate-egress-sandbox.sh
```

The `emulate-egress-sandbox` skill in `.agents/skills` wraps that script.
Shared skills also include `verify-netviewer-rust` and `verify-netviewer-bundle`.
Windows and Linux cannot run this Darwin library; use the
[macOS CI results](https://github.com/cgfixit/CyClaw-Net-Viewer/actions/workflows/ci.yml).

Runtime boundaries (no App Sandbox, confirmed SIGTERM, numeric CLI, private
CSV): [SECURITY.md](SECURITY.md).

## Docs

- [Design](docs/DESIGN.md)
- [Security](SECURITY.md)
- [Build](docs/BUILD.md)
- [Controls](docs/CONTROLS.md)
- [Known issues](docs/KNOWN_ISSUES.md)
- [Egress harness](docs/EGRESS_SANDBOX.md)
- [Contributing](CONTRIBUTING.md)
- [User guide (PDF)](docs/CyClaw-Net-Viewer-User-Guide.pdf)
- [How it works (PDF)](docs/CyClaw-Net-Viewer-How-It-Works.pdf)

## License

MIT. See [LICENSE](LICENSE).
