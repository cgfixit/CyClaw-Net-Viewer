#!/usr/bin/env python3
"""Fail-closed static checks for CyClaw-Net-Viewer (crate netboard).

Run from the repository root. Stdlib only. Exit 1 on the first batch of
failures after printing every assertion.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path


FORBIDDEN_CRATES = ("tokio", "reqwest", "ureq", "clap", "anyhow", "tracing")


def repo_root() -> Path:
    here = Path.cwd()
    for candidate in (here, *here.parents):
        cargo = candidate / "Cargo.toml"
        if not cargo.is_file():
            continue
        text = cargo.read_text(encoding="utf-8")
        if 'name = "netboard"' in text:
            return candidate
    raise SystemExit("netboard Cargo.toml not found from the current directory")


def read(root: Path, rel: str) -> str:
    path = root / rel
    if not path.is_file():
        raise FileNotFoundError(rel)
    return path.read_text(encoding="utf-8")


def main() -> int:
    root = repo_root()
    results: list[tuple[str, bool, str]] = []

    def check(name: str, ok: bool, detail: str) -> None:
        results.append((name, ok, detail))

    try:
        cargo = read(root, "Cargo.toml")
        lib = read(root, "src/lib.rs")
        kill = read(root, "src/kill.rs")
        export = read(root, "src/export.rs")
        ci = read(root, ".github/workflows/ci.yml")
        toolchain = read(root, "rust-toolchain.toml")
    except FileNotFoundError as err:
        print(f"FAIL missing {err}")
        return 1

    check(
        "macos_compile_error",
        'compile_error!("netboard (CyClaw-Net-Viewer) is macOS-only")' in lib
        and "#[cfg(not(target_os = \"macos\"))]" in lib,
        "src/lib.rs must keep the macOS-only compile_error!",
    )
    check(
        "rust_version",
        'rust-version = "1.85.0"' in cargo,
        'Cargo.toml rust-version must be "1.85.0"',
    )
    check(
        "toolchain_channel",
        'channel = "1.85.0"' in toolchain,
        'rust-toolchain.toml channel must be "1.85.0"',
    )
    check(
        "eframe_pin",
        'eframe = "=0.31.1"' in cargo,
        "Cargo.toml eframe must stay exactly =0.31.1",
    )
    check(
        "egui_extras_pin",
        'version = "=0.31.1"' in cargo and "egui_extras" in cargo,
        "Cargo.toml egui_extras must stay exactly =0.31.1",
    )
    check(
        "image_pin",
        'version = "=0.25.6"' in cargo and "image =" in cargo,
        "Cargo.toml image must stay exactly =0.25.6",
    )

    forbidden_hits = [
        name
        for name in FORBIDDEN_CRATES
        if re.search(rf"(?m)^\s*{re.escape(name)}\s*=", cargo)
    ]
    check(
        "no_forbidden_crates",
        not forbidden_hits,
        "Cargo.toml must not add " + ", ".join(FORBIDDEN_CRATES)
        if not forbidden_hits
        else "forbidden crate(s): " + ", ".join(forbidden_hits),
    )
    check(
        "kill_rejects_pid_zero",
        "if pid == 0" in kill and "checked_target" in kill,
        "src/kill.rs must reject PID 0 before libc::kill",
    )
    check(
        "kill_rejects_launchd",
        "if pid == 1" in kill and "libc::kill" in kill,
        "src/kill.rs must reject launchd (PID 1) before libc::kill",
    )
    check(
        "export_create_new_0600",
        ".create_new(true)" in export and ".mode(0o600)" in export,
        "src/export.rs must use create_new + mode 0o600",
    )
    check(
        "ci_contents_read",
        re.search(r"(?m)^permissions:\s*\r?\n\s+contents:\s+read\s*$", ci) is not None
        and "contents: write" not in ci,
        ".github/workflows/ci.yml must keep top-level contents: read",
    )
    check(
        "ci_no_persist_credentials",
        "persist-credentials: false" in ci and "persist-credentials: true" not in ci,
        ".github/workflows/ci.yml checkout must not persist credentials",
    )

    failed = 0
    for name, ok, detail in results:
        status = "PASS" if ok else "FAIL"
        if not ok:
            failed += 1
        print(f"{status} {name}: {detail}")

    print(f"{len(results) - failed}/{len(results)} passed")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
