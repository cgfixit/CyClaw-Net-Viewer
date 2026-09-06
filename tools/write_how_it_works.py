#!/usr/bin/env python3
"""Write CyClaw-Net-Viewer how-it-works PDF into docs/."""

from pathlib import Path

from reportlab.lib import colors
from reportlab.lib.enums import TA_CENTER, TA_JUSTIFY
from reportlab.lib.pagesizes import letter
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import inch
from reportlab.platypus import (
    ListFlowable,
    ListItem,
    PageBreak,
    Paragraph,
    SimpleDocTemplate,
    Spacer,
    Table,
    TableStyle,
)

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs" / "CyClaw-Net-Viewer-How-It-Works.pdf"

INK = colors.HexColor("#1A1A1A")
MUTED = colors.HexColor("#4A4A4A")
RULE = colors.HexColor("#2C5282")
HEAD_BG = colors.HexColor("#1B365D")
PALE = colors.HexColor("#F4F7FB")
GRID = colors.HexColor("#C5CDD6")
HIGH = colors.HexColor("#FFC7CE")
MED = colors.HexColor("#FFEB9C")
LOW = colors.HexColor("#C6EFCE")


def styles():
    s = getSampleStyleSheet()
    s.add(
        ParagraphStyle(
            "CoverTitle",
            parent=s["Title"],
            fontName="Times-Bold",
            fontSize=24,
            leading=28,
            textColor=HEAD_BG,
            alignment=TA_CENTER,
            spaceAfter=8,
        )
    )
    s.add(
        ParagraphStyle(
            "CoverSub",
            parent=s["Normal"],
            fontName="Times-Italic",
            fontSize=12,
            leading=16,
            textColor=MUTED,
            alignment=TA_CENTER,
            spaceAfter=16,
        )
    )
    s.add(
        ParagraphStyle(
            "H1c",
            parent=s["Heading1"],
            fontName="Times-Bold",
            fontSize=16,
            leading=20,
            textColor=HEAD_BG,
            spaceBefore=14,
            spaceAfter=8,
        )
    )
    s.add(
        ParagraphStyle(
            "H2c",
            parent=s["Heading2"],
            fontName="Times-Bold",
            fontSize=13,
            leading=16,
            textColor=RULE,
            spaceBefore=10,
            spaceAfter=6,
        )
    )
    s.add(
        ParagraphStyle(
            "Body",
            parent=s["Normal"],
            fontName="Times-Roman",
            fontSize=10.5,
            leading=14.5,
            textColor=INK,
            alignment=TA_JUSTIFY,
            spaceAfter=8,
        )
    )
    s.add(
        ParagraphStyle(
            "Cell",
            parent=s["Normal"],
            fontName="Times-Roman",
            fontSize=8.5,
            leading=11.5,
            textColor=INK,
        )
    )
    s.add(
        ParagraphStyle(
            "CellB",
            parent=s["Normal"],
            fontName="Times-Bold",
            fontSize=8.5,
            leading=11.5,
            textColor=INK,
        )
    )
    s.add(
        ParagraphStyle(
            "CodeBlock",
            parent=s["Normal"],
            fontName="Courier",
            fontSize=8.5,
            leading=11.5,
            textColor=INK,
            backColor=PALE,
            leftIndent=6,
            rightIndent=6,
            spaceBefore=4,
            spaceAfter=8,
        )
    )
    s.add(
        ParagraphStyle(
            "BulletBody",
            parent=s["Normal"],
            fontName="Times-Roman",
            fontSize=10.5,
            leading=14.5,
            textColor=INK,
        )
    )
    return s


S = styles()


def P(text, style="Body"):
    return Paragraph(text, S[style])


def cell(text, bold=False):
    return Paragraph(text, S["CellB"] if bold else S["Cell"])


def bullets(items):
    return ListFlowable(
        [ListItem(P(i, "BulletBody"), leftIndent=12, bulletColor=HEAD_BG) for i in items],
        bulletType="bullet",
        leftIndent=18,
        spaceAfter=8,
    )


def table(rows, col_widths, header=True):
    t = Table(rows, colWidths=col_widths, repeatRows=1 if header else 0)
    cmds = [
        ("VALIGN", (0, 0), (-1, -1), "TOP"),
        ("LEFTPADDING", (0, 0), (-1, -1), 5),
        ("RIGHTPADDING", (0, 0), (-1, -1), 5),
        ("TOPPADDING", (0, 0), (-1, -1), 4),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 4),
        ("GRID", (0, 0), (-1, -1), 0.4, GRID),
    ]
    if header:
        cmds += [
            ("BACKGROUND", (0, 0), (-1, 0), HEAD_BG),
            ("TEXTCOLOR", (0, 0), (-1, 0), colors.white),
        ]
        for r in rows[0]:
            if isinstance(r, Paragraph):
                r.style = ParagraphStyle(
                    "Hdr",
                    parent=S["CellB"],
                    textColor=colors.white,
                    fontName="Times-Bold",
                )
    t.setStyle(TableStyle(cmds))
    return t


def header_footer(canvas, doc):
    canvas.saveState()
    w, h = letter
    canvas.setFillColor(HEAD_BG)
    canvas.rect(0, h - 0.42 * inch, w, 0.42 * inch, fill=1, stroke=0)
    canvas.setFillColor(colors.white)
    canvas.setFont("Times-Bold", 9)
    canvas.drawString(0.75 * inch, h - 0.27 * inch, "CyClaw-Net-Viewer")
    canvas.setFont("Times-Roman", 9)
    canvas.drawRightString(w - 0.75 * inch, h - 0.27 * inch, "How it works")
    canvas.setFillColor(HEAD_BG)
    canvas.rect(0, 0, w, 0.4 * inch, fill=1, stroke=0)
    canvas.setFillColor(colors.white)
    canvas.setFont("Times-Roman", 8)
    canvas.drawString(0.75 * inch, 0.17 * inch, "Companion to the User Guide")
    canvas.drawRightString(w - 0.75 * inch, 0.17 * inch, f"Page {doc.page}")
    canvas.restoreState()


def story():
    usable = 7.0 * inch
    out = []

    out += [
        Spacer(1, 0.35 * inch),
        P("CyClaw-Net-Viewer", "CoverTitle"),
        P("How it works", "CoverTitle"),
        P(
            "Why Rust, how the Mac .app launches, what the code is doing,<br/>"
            "and which dependencies can bite later.",
            "CoverSub",
        ),
        P(
            "Read this after the User Guide. That one is clicks and colors. "
            "This one is for reviewing the source in this repository with two altitudes: "
            "ELI5, then Tech 101 with real file and crate names.",
            "Body",
        ),
        Spacer(1, 10),
        P("1. ELI5", "H1c"),
        P(
            "Your Mac already keeps a list of every phone line (TCP) and every walkie-talkie "
            "channel (UDP) that programs have open. CyClaw-Net-Viewer is a live scoreboard. "
            "About once a second it asks the operating system: who is talking, on which port, "
            "to whom, and which app owns that socket? Then it draws a table and colors the rows "
            "when something new appears, changes, or hangs up.",
            "Body",
        ),
        P(
            "Ping is a postcard (ICMP). This app does not list postcards. It only lists phone "
            "lines. That is why pinging google.com did not light up red. curl or the real CyClaw "
            "process would, because those open TCP.",
            "Body",
        ),
        P(
            "Orange means “this line goes off the machine.” Green/blue/yellow/red are the "
            "Sysinternals-style events (new out, new in, state change, gone). The Finder icon "
            "is just a folder named .app with the compiled program inside. Double-click runs "
            "that program. There is no Python hiding in the bundle.",
            "Body",
        ),
    ]

    out += [
        P("2. Why Rust instead of Python", "H1c"),
        P(
            "Python would have been the fast way to parse lsof in a terminal. It was the wrong "
            "shape for a double-click desktop table that walks every process every second and "
            "must not inherit CyClaw’s own dependency graph.",
            "Body",
        ),
    ]
    out.append(
        table(
            [
                [cell("Need", True), cell("Rust here", True), cell("Python would have been", True)],
                [
                    cell("Ship to a Mac"),
                    cell("One Mach-O binary plus Info.plist. No interpreter on the target."),
                    cell("py2app / PyInstaller: a Python runtime, site-packages, and a fat .app. Easy to break Gatekeeper and PATH."),
                ],
                [
                    cell("1 Hz snapshot of all PIDs"),
                    cell("Direct libproc FFI. No GIL. No shelling out to lsof and parsing columns."),
                    cell("subprocess + parse, or ctypes to the same C APIs with more glue and worse packing bugs."),
                ],
                [
                    cell("Watch CyClaw (a Python app)"),
                    cell("Different language and crate tree. This viewer does not import OpenTelemetry, httpx, or CyClaw extras."),
                    cell("Same ecosystem. Easy to accidentally pull a telemetry SDK into the watcher."),
                ],
                [
                    cell("Reproducible compiler"),
                    cell("rust-toolchain.toml pins rustc 1.85.0."),
                    cell("Whatever python3 is on PATH, plus pip drift."),
                ],
                [
                    cell("Universal binary"),
                    cell("Two cargo --target builds and lipo."),
                    cell("Two Pythons or a universal CPython embed; messy."),
                ],
                [
                    cell("Cost we paid"),
                    cell("eframe/icu MSRV pins; netstat2 bindgen at compile time; more files than a 40-line script."),
                    cell("Faster first prototype if the only goal was “print lsof in a window.”"),
                ],
            ],
            [1.35 * inch, 2.7 * inch, usable - 4.05 * inch],
        )
    )
    out += [
        Spacer(1, 8),
        P(
            "Bottom line: Python is excellent for CyClaw itself and for one-off checks. "
            "Rust is why this viewer is a ~6 MB native .app you can leave running at 1 Hz "
            "without dragging a second copy of the observed stack along for the ride.",
            "Body",
        ),
    ]

    out += [
        P("3. What makes it a standalone Mac app", "H1c"),
        P(
            "Not Tauri, not Electron, not SwiftUI, not py2app. Four pieces stacked:",
            "Body",
        ),
        P("3.1 Compile a native binary", "H2c"),
        P(
            "rustc 1.85.0 builds crate netboard into an executable named netboard. "
            "scripts/make-app.sh sets MACOSX_DEPLOYMENT_TARGET=12.0 so the Mach-O’s "
            "minos is 12.0 (Monterey). It builds aarch64-apple-darwin and, when the SDK "
            "allows, x86_64-apple-darwin, then lipo -create so one file is Intel and Apple Silicon.",
            "Body",
        ),
        P("3.2 Draw a window with eframe (egui)", "H2c"),
        P(
            "eframe 0.31.1 is a small Rust GUI framework. egui is immediate-mode UI (every frame "
            "the code says “here is a table”). winit talks to AppKit for the window. glow is "
            "OpenGL for pixels. That is why you get a real macOS window without a WebView and "
            "without bundling Chromium.",
            "Body",
        ),
        P("3.3 Wrap it in a .app bundle", "H2c"),
        P(
            "macOS does not launch a raw binary from Finder the way you want for a product. "
            "A .app is a directory. Ours is:",
            "Body",
        ),
        P(
            "CyClaw-Net-Viewer.app/<br/>"
            "&nbsp;&nbsp;Contents/<br/>"
            "&nbsp;&nbsp;&nbsp;&nbsp;Info.plist&nbsp;&nbsp;&nbsp;# name, bundle id local.cyclaw.netviewer, min OS 12.0<br/>"
            "&nbsp;&nbsp;&nbsp;&nbsp;MacOS/netboard&nbsp;&nbsp;# the lipo’d executable",
            "CodeBlock",
        ),
        P(
            "scripts/make-app.sh copies the binary, copies Info.plist, then ad-hoc signs: "
            "codesign --force --sign -. That is a local signature with no Developer ID and "
            "no Apple notarization. Gatekeeper’s first-open rule is Right-click → Open. "
            "There is no App Sandbox. Sandboxing would hide other processes’ sockets and "
            "defeat the tool.",
            "Body",
        ),
        P(
            "Double-click = launchd/Finder exec of Contents/MacOS/netboard. No Python, "
            "no Node, no JVM.",
            "Body",
        ),
    ]

    out += [
        P("4. Tech 101 — what the code is doing", "H1c"),
        P(
            "All of this lives under src/. Binary entry is main.rs. "
            "Library modules are re-exported from lib.rs.",
            "Body",
        ),
        P("4.1 The loop", "H2c"),
    ]
    out.append(
        table(
            [
                [cell("Step", True), cell("Who", True), cell("What", True)],
                [
                    cell("1. Kernel"),
                    cell("Darwin libproc"),
                    cell("Every process’s file descriptors, including sockets, with local/remote and TCP state."),
                ],
                [
                    cell("2. snapshot.rs"),
                    cell("netstat2 + libc"),
                    cell("Turn those FDs into Endpoint structs: pid, proto, addrs, process name, path, In/Out/Listen/Unknown."),
                ],
                [
                    cell("3. diff.rs"),
                    cell("pure Rust"),
                    cell("Compare to last tick. New / changed / gone (gone lingers two ticks)."),
                ],
                [
                    cell("4. dns.rs"),
                    cell("8 worker threads"),
                    cell("PTR getnameinfo, then a forward A lookup so a v6 row can still show hostname (IPv4)."),
                ],
                [
                    cell("5. app.rs"),
                    cell("eframe table"),
                    cell("Paint rows. Orange if off-box and no event color. Snapshot runs on a background thread."),
                ],
            ],
            [1.15 * inch, 1.45 * inch, usable - 2.6 * inch],
        )
    )
    out += [
        Spacer(1, 8),
        P("4.2 Files to read, in order", "H2c"),
    ]
    out.append(
        table(
            [
                [cell("File", True), cell("Job", True)],
                [
                    cell("main.rs"),
                    cell("If argv has --cli, print a snapshot and exit. Else app::run() opens the window."),
                ],
                [
                    cell("snapshot.rs"),
                    cell("get_sockets_info(IPv4|IPv6, TCP|UDP). Map netstat2 TCP states onto our enum. proc_name / proc_pidpath per PID (cached per tick). TCP direction: LISTEN → Listen, SYN_SENT → Out, else In if that PID also listens on the local port. UDP is Unknown because peers are unavailable. is_offbox: not unspecified, not loopback (v4-mapped loopback counts as local)."),
                ],
                [
                    cell("diff.rs"),
                    cell("Identity key = (pid, proto, ip version, local, remote). New key → NewIn or NewOut; Unknown has no direction flash. Same key, different TCP state → Changed. Missing from next snapshot → Deleted linger=2, then 1, then drop. Returning after delete is New, not Changed."),
                ],
                [
                    cell("app.rs"),
                    cell("eframe::run_native(\"CyClaw-Net-Viewer\"). A thread calls snapshot+diff, stores Vec&lt;Row&gt; in a mutex (poison → into_inner; panic → catch_unwind and keep last good rows). The UI thread paints TableBuilder, legend, Off-box only filter. Close Connection is kill.rs SIGTERM after a modal."),
                ],
                [
                    cell("dns.rs"),
                    cell("Resolver::request queues IPs. Workers call getnameinfo(NI_NAMEREQD), then to_socket_addrs for IPv4. UI shows hostname (1.2.3.4):port when both exist."),
                ],
                [
                    cell("cli.rs / kill.rs"),
                    cell("Tcpvcon-style flags -a -c -n. terminate() refuses pid 0 and our own pid."),
                ],
            ],
            [1.35 * inch, usable - 1.35 * inch],
        )
    )
    out += [
        Spacer(1, 8),
        P(
            "Off-box orange is a paint overlay, not a fifth diff event. Highlight::None plus "
            "is_offbox(remote) plus Color remotes → orange. New/changed/gone still win.",
            "Body",
        ),
    ]

    out += [
        P("5. Dependencies that could be a risk later", "H1c"),
        P(
            "Runtime is the Mach-O plus macOS libproc and OpenGL. No Python, no Node, "
            "no telemetry SDK in this process. The risks are mostly compile-time pins and "
            "the GUI stack’s appetite for newer rustc.",
            "Body",
        ),
    ]

    risk_header = [
        cell("Piece", True),
        cell("Role", True),
        cell("Risk", True),
        cell("Why it can hurt later", True),
    ]
    risk_rows = [risk_header]

    def risk(piece, role, level, why, fill):
        lvl = Paragraph(
            f"<b>{level}</b>",
            ParagraphStyle("rl", parent=S["CellB"], alignment=TA_CENTER),
        )
        inner = Table([[lvl]], colWidths=[0.85 * inch])
        inner.setStyle(
            TableStyle(
                [
                    ("BACKGROUND", (0, 0), (-1, -1), fill),
                    ("VALIGN", (0, 0), (-1, -1), "MIDDLE"),
                    ("ALIGN", (0, 0), (-1, -1), "CENTER"),
                    ("TOPPADDING", (0, 0), (-1, -1), 4),
                    ("BOTTOMPADDING", (0, 0), (-1, -1), 4),
                ]
            )
        )
        return [cell(piece, True), cell(role), inner, cell(why)]

    risk_rows += [
        risk(
            "eframe / egui / winit / objc2 / glow",
            "Window and table",
            "High",
            "New egui releases raise MSRV. We already had to pin icu/idna/image/writeable so rustc 1.85 still builds. Apple AppKit bindings churn. Do not cargo update eframe casually.",
            HIGH,
        ),
        risk(
            "netstat2 0.11",
            "Socket list via libproc",
            "Med",
            "Uses bindgen + libclang at compile time (needs Xcode CLT). Quiet crate. UDP remotes are missing. If xnu struct layouts change, bindgen may still compile and return garbage.",
            MED,
        ),
        risk(
            "Pinned icu_* / idna_adapter / url / image / writeable",
            "MSRV shims for eframe",
            "Med",
            "They exist only so 1.85 compiles. A blanket cargo update drops the pins and the build dies. Leave the exact versions in Cargo.toml.",
            MED,
        ),
        risk(
            "webbrowser (egui-winit links)",
            "Unused “open URL” helper",
            "Low",
            "We do not open links, but the feature pulled the icu/idna stack. Harmless at runtime; annoying at compile pin time.",
            LOW,
        ),
        risk(
            "arboard",
            "Copy line / copy remote",
            "Low",
            "Clipboard. Small. Could lag behind objc2 versions if eframe jumps.",
            LOW,
        ),
        risk(
            "libc",
            "proc_name, proc_pidpath, getnameinfo, kill",
            "Low",
            "Thin FFI. Darwin symbols have been stable for years. Wrong struct packing would be our bug, not libc’s.",
            LOW,
        ),
        risk(
            "Ad-hoc codesign, no notarization",
            "Finder launch",
            "Med",
            "Gatekeeper and future macOS releases can get stricter with unsigned apps. Right-click → Open works today; it is not a promise.",
            MED,
        ),
        risk(
            "No App Sandbox",
            "See other processes’ sockets",
            "Low (by design)",
            "Do not “harden” by enabling the sandbox. That would empty the table. The risk is App Store rules, which this app is not aiming at.",
            LOW,
        ),
    ]
    out.append(
        table(
            risk_rows,
            [1.55 * inch, 1.15 * inch, 0.95 * inch, usable - 3.65 * inch],
        )
    )
    out += [
        Spacer(1, 8),
        P(
            "Build machine only: clang/libclang for netstat2’s bindgen. The .app you copy "
            "to another Mac does not include clang, Rust, or Python.",
            "Body",
        ),
        P("Practical rule", "H2c"),
        bullets(
            [
                "Do not run cargo update without checking rustc 1.85.0 still builds.",
                "Do not bump eframe past 0.31.x while the toolchain is 1.85.0.",
                "If netstat2 bitrots, replace it with a small hand-written libproc walker (the Darwin headers are in the macOS SDK).",
                "Never add an HTTP client, OpenTelemetry, or “just one analytics call” to this binary. It exists to watch those things, not to be one.",
            ]
        ),
    ]

    out += [
        P("6. What we deliberately did not use", "H1c"),
        bullets(
            [
                "Tauri / Electron / a local web page — extra Chromium and a second language.",
                "PyInstaller / py2app — see section 2.",
                "Godot — the gamedev skill’s Mac ship bar applied; the engine did not.",
                "Network Extension / pcap / PKTAP — overkill for a netstat table; needs entitlements.",
                "True TCB close — Darwin has no SetTcpEntry. SIGTERM is the honest substitute.",
            ]
        ),
        P(
            "User guide: docs/CyClaw-Net-Viewer-User-Guide.pdf. "
            "This document: docs/CyClaw-Net-Viewer-How-It-Works.pdf.",
            "Body",
        ),
    ]
    return out


def main():
    OUT.parent.mkdir(parents=True, exist_ok=True)
    doc = SimpleDocTemplate(
        str(OUT),
        pagesize=letter,
        leftMargin=0.75 * inch,
        rightMargin=0.75 * inch,
        topMargin=0.65 * inch,
        bottomMargin=0.6 * inch,
        title="CyClaw-Net-Viewer How It Works",
        author="CyClaw-Net-Viewer",
        subject="Architecture, Rust vs Python, Mac .app, dependency risks",
    )
    doc.build(story(), onFirstPage=header_footer, onLaterPages=header_footer)
    print(OUT)


if __name__ == "__main__":
    main()
