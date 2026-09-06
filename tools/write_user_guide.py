#!/usr/bin/env python3
"""Write CyClaw-Net-Viewer user guide PDF into docs/."""

from pathlib import Path

from reportlab.lib import colors
from reportlab.lib.enums import TA_CENTER, TA_JUSTIFY, TA_LEFT
from reportlab.lib.pagesizes import letter
from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
from reportlab.lib.units import inch
from reportlab.platypus import (
    KeepTogether,
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
OUT = ROOT / "docs" / "CyClaw-Net-Viewer-User-Guide.pdf"

# App palette
C_NEW_OUT = colors.HexColor("#C6EFCE")
C_NEW_OUT_FG = colors.HexColor("#006100")
C_NEW_IN = colors.HexColor("#BDD7EE")
C_NEW_IN_FG = colors.HexColor("#1F4E79")
C_CHANGED = colors.HexColor("#FFEB9C")
C_CHANGED_FG = colors.HexColor("#9C5700")
C_GONE = colors.HexColor("#FFC7CE")
C_GONE_FG = colors.HexColor("#9C0006")
C_OFFBOX = colors.HexColor("#F8CBAD")
C_OFFBOX_FG = colors.HexColor("#843C0B")
INK = colors.HexColor("#1A1A1A")
MUTED = colors.HexColor("#4A4A4A")
RULE = colors.HexColor("#2C5282")
HEAD_BG = colors.HexColor("#1B365D")
PALE = colors.HexColor("#F4F7FB")
GRID = colors.HexColor("#C5CDD6")


def styles():
    s = getSampleStyleSheet()
    s.add(
        ParagraphStyle(
            "CoverTitle",
            parent=s["Title"],
            fontName="Times-Bold",
            fontSize=26,
            leading=30,
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
            spaceAfter=18,
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
            spaceBefore=16,
            spaceAfter=8,
            borderPadding=0,
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
            spaceBefore=12,
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
            fontSize=9,
            leading=12,
            textColor=INK,
        )
    )
    s.add(
        ParagraphStyle(
            "CellB",
            parent=s["Normal"],
            fontName="Times-Bold",
            fontSize=9,
            leading=12,
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
    s.add(
        ParagraphStyle(
            "Footer",
            parent=s["Normal"],
            fontName="Times-Italic",
            fontSize=8,
            textColor=MUTED,
            alignment=TA_CENTER,
        )
    )
    s.add(
        ParagraphStyle(
            "Caption",
            parent=s["Normal"],
            fontName="Times-Italic",
            fontSize=9,
            leading=12,
            textColor=MUTED,
            alignment=TA_LEFT,
            spaceAfter=10,
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
        ("LEFTPADDING", (0, 0), (-1, -1), 6),
        ("RIGHTPADDING", (0, 0), (-1, -1), 6),
        ("TOPPADDING", (0, 0), (-1, -1), 5),
        ("BOTTOMPADDING", (0, 0), (-1, -1), 5),
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


def color_row(fill, fg, name, meaning):
    swatch = Table(
        [[Paragraph("&nbsp;", S["Cell"])]],
        colWidths=[0.38 * inch],
        rowHeights=[0.22 * inch],
    )
    swatch.setStyle(
        TableStyle(
            [
                ("BACKGROUND", (0, 0), (-1, -1), fill),
                ("BOX", (0, 0), (-1, -1), 0.6, fg),
            ]
        )
    )
    name_p = Paragraph(
        f"<b>{name}</b>",
        ParagraphStyle("cn", parent=S["CellB"], textColor=fg),
    )
    return [swatch, name_p, cell(meaning)]


def header_footer(canvas, doc):
    canvas.saveState()
    w, h = letter
    canvas.setFillColor(HEAD_BG)
    canvas.rect(0, h - 0.42 * inch, w, 0.42 * inch, fill=1, stroke=0)
    canvas.setFillColor(colors.white)
    canvas.setFont("Times-Bold", 9)
    canvas.drawString(0.75 * inch, h - 0.27 * inch, "CyClaw-Net-Viewer")
    canvas.setFont("Times-Roman", 9)
    canvas.drawRightString(w - 0.75 * inch, h - 0.27 * inch, "User Guide")
    canvas.setFillColor(HEAD_BG)
    canvas.rect(0, 0, w, 0.4 * inch, fill=1, stroke=0)
    canvas.setFillColor(colors.white)
    canvas.setFont("Times-Roman", 8)
    canvas.drawString(0.75 * inch, 0.17 * inch, "Not affiliated with Microsoft or Sysinternals")
    canvas.drawRightString(w - 0.75 * inch, 0.17 * inch, f"Page {doc.page}")
    canvas.restoreState()


def story():
    usable = 7.0 * inch
    out = []

    out += [
        Spacer(1, 1.4 * inch),
        P("CyClaw-Net-Viewer", "CoverTitle"),
        P("User Guide", "CoverTitle"),
        P(
            "A TCPView-class live TCP/UDP endpoint viewer for macOS,<br/>"
            "built for CyClaw telemetry-kill verification.",
            "CoverSub",
        ),
        Spacer(1, 12),
        P(
            "Version 0.1.0 &nbsp;·&nbsp; macOS 12+ (Monterey) &nbsp;·&nbsp; "
            "Apple Silicon and Intel &nbsp;·&nbsp; MIT License",
            "CoverSub",
        ),
        Spacer(1, 36),
        P(
            "This guide describes how to install, read, and use CyClaw-Net-Viewer "
            "to watch local TCP and UDP sockets, including off-box remotes that "
            "matter when proving a telemetry kill switch. It is not affiliated with "
            "Microsoft, Sysinternals, or TCPView. Windows TCPView remains the "
            "inspiration for the table and the color events; Darwin cannot copy "
            "every Windows kernel trick, and those limits are called out where they matter.",
            "Body",
        ),
        PageBreak(),
    ]

    # 1
    out += [
        P("1. What this app is", "H1c"),
        P(
            "CyClaw-Net-Viewer is a native macOS desktop tool that lists every TCP and UDP "
            "endpoint it can see on this Mac, the process that owns it, and whether the peer "
            "is on-box or off-box. It refreshes about once a second (adjustable) and paints "
            "rows when sockets appear, change state, disappear, or talk to a remote address.",
            "Body",
        ),
        P(
            "Think of it as a more readable, live subset of what <font face='Courier'>netstat</font> "
            "and <font face='Courier'>lsof -i</font> already know, with Sysinternals-style "
            "highlighting. The crate and binary are still named <font face='Courier'>netboard</font>; "
            "the window, bundle, and this guide use <b>CyClaw-Net-Viewer</b>.",
            "Body",
        ),
        P("It is for", "H2c"),
        bullets(
            [
                "Watching which apps hold ESTABLISHED TCP sessions to the internet.",
                "Confirming CyClaw (or any local process) is not opening unexpected off-box sockets.",
                "Catching short-lived connects: new rows flash green, closed rows linger red.",
                "Exporting a CSV snapshot for notes or a ticket.",
            ]
        ),
        P("It is not", "H2c"),
        bullets(
            [
                "A packet sniffer. It does not show ICMP, ping, or payload.",
                "A firewall. It cannot block a flow.",
                "A perfect clone of Windows TCPView’s “Close Connection.” Darwin has no public TCB-delete for another process’s socket.",
                "A cloud service. Nothing is uploaded. Reverse DNS is a local lookup.",
            ]
        ),
    ]

    # 2
    out += [
        P("2. Install and first launch", "H1c"),
        P(
            "Requirements: macOS 12 (Monterey) or newer, Intel or Apple Silicon. "
            "The shipped <font face='Courier'>.app</font> is a universal binary "
            "(arm64 + x86_64) with deployment target 12.0. Do not enable App Sandbox "
            "or Hardened Runtime; those hide other processes’ sockets.",
            "Body",
        ),
        P("Double-click the app", "H2c"),
        P(
            "The packaged app is <font face='Courier'>dist/CyClaw-Net-Viewer.app</font> "
            "in the project, or a DMG from <font face='Courier'>./scripts/make-app.sh --dmg</font>. "
            "The binary is ad-hoc signed, not notarized. Gatekeeper will block the first "
            "double-click.",
            "Body",
        ),
        bullets(
            [
                "Finder: Right-click CyClaw-Net-Viewer.app → Open → Open.",
                "After that, ordinary double-click works for this user account.",
                "If macOS still refuses, System Settings → Privacy &amp; Security → Open Anyway.",
            ]
        ),
        P("Build from source", "H2c"),
        P(
            "Rust 1.85.0 is pinned (<font face='Courier'>rust-toolchain.toml</font>). "
            "Homebrew’s newer rustc is not the pin. Put rustup on PATH, then:",
            "Body",
        ),
        P(
            "export PATH=\"$HOME/.cargo/bin:$PATH\"<br/>"
            "cd Mac-NetViewer-EZview<br/>"
            "cargo run<br/>"
            "cargo run -- --cli -n -a<br/>"
            "./scripts/make-app.sh",
            "CodeBlock",
        ),
        P(
            "Viewing never requires root. SIP-protected processes may omit sockets unless "
            "you later choose to run as root; the table is then just more complete, not a different product.",
            "Body",
        ),
    ]

    # 3
    out += [
        P("3. Window tour", "H1c"),
        P(
            "The window title is <b>CyClaw-Net-Viewer</b>. A menu bar (File, Options, Help), "
            "a one-line toolbar, a sortable table, and a status bar with a color legend fill the rest.",
            "Body",
        ),
        P("Toolbar", "H2c"),
    ]
    out.append(
        table(
            [
                [cell("Control", True), cell("What it does", True)],
                [cell("Pause"), cell("Stop the one-second snapshot loop. Space also toggles pause unless the filter box is focused.")],
                [cell("Refresh"), cell("Take a snapshot now. Key R.")],
                [cell("Resolve names"), cell("Default on. Shows hostname (IPv4):port when PTR/A exist. Key N.")],
                [cell("Color remotes"), cell("Default on. Stable off-box TCP/UDP stays orange.")],
                [cell("Off-box only"), cell("Hide listeners, loopback, and *:*. Use this for telemetry-kill watching.")],
                [cell("Rate"), cell("0.5s, 1s (default), 2s, or 5s.")],
                [cell("Filter"), cell("Case-insensitive substring on process, PID, proto, direction, addresses, state, path.")],
            ],
            [1.5 * inch, usable - 1.5 * inch],
        )
    )
    out += [
        Spacer(1, 8),
        P("Columns", "H2c"),
    ]
    out.append(
        table(
            [
                [cell("Column", True), cell("Meaning", True)],
                [cell("Process"), cell("Short name from proc_name. “?” if the PID could not be named.")],
                [cell("PID"), cell("Owning process id. 0 means the kernel did not attach a PID.")],
                [cell("Proto"), cell("TCP4, TCP6, UDP4, or UDP6.")],
                [cell("Dir"), cell("TCP: Listen, In (local listener heuristic), or Out (including SYN_SENT). UDP: Unknown because peers are unavailable.")],
                [cell("Local"), cell("Local address and port. Unspecified shows as *:port.")],
                [cell("Remote"), cell("Peer address and port. Off-box remotes are the telemetry-kill signal.")],
                [cell("State"), cell("TCP FSM name (ESTABLISHED, LISTEN, TIME_WAIT, …). Empty for UDP.")],
                [cell("Path"), cell("Full executable path from proc_pidpath, when available.")],
            ],
            [1.2 * inch, usable - 1.2 * inch],
        )
    )
    out += [
        Spacer(1, 6),
        P(
            "Click a header to sort; click again to reverse. Click a row to select. "
            "Right-click for Copy line, Copy remote, Terminate process, and Reveal in Finder. "
            "The status bar shows endpoint count, how many are off-box, and the color legend.",
            "Body",
        ),
    ]

    # 4
    out += [
        P("4. Color scheme", "H1c"),
        P(
            "Event colors follow Sysinternals TCPView. CyClaw-Net-Viewer adds a persistent "
            "orange for any TCP/UDP peer that is not unspecified and not loopback, so a "
            "phone-home that is already ESTABLISHED does not look like a blank row. "
            "Event colors always win over orange.",
            "Body",
        ),
    ]
    out.append(
        table(
            [
                [cell("Swatch", True), cell("Name", True), cell("When you see it", True)],
                color_row(C_NEW_OUT, C_NEW_OUT_FG, "New out", "New outgoing endpoint since the last tick (including SYN_SENT)."),
                color_row(C_NEW_IN, C_NEW_IN_FG, "New in", "New incoming endpoint or new LISTEN."),
                color_row(C_CHANGED, C_CHANGED_FG, "Changed", "Same 4-tuple, TCP state changed (for example ESTABLISHED → CLOSE_WAIT)."),
                color_row(C_GONE, C_GONE_FG, "Gone", "Endpoint vanished. Shown for two refresh ticks (~2s at 1s rate), then dropped."),
                color_row(C_OFFBOX, C_OFFBOX_FG, "Off-box", "Stable TCP/UDP to a remote that is not this Mac. Stays lit while the socket exists."),
            ],
            [0.7 * inch, 1.1 * inch, usable - 1.8 * inch],
        )
    )
    out += [
        Spacer(1, 8),
        P(
            "Priority if more than one would apply: gone &gt; new &gt; changed &gt; off-box. "
            "A row cannot be both new and gone. Loopback and *:port stay uncolored when stable "
            "so the orange set is actually egress.",
            "Body",
        ),
        P(
            "ICMP and ping never appear. They are not TCP or UDP sockets. To exercise colors, "
            "use <font face='Courier'>curl</font>, <font face='Courier'>nc host 443</font>, "
            "or start the process under test.",
            "Body",
        ),
    ]

    # 5
    out += [
        P("5. Addresses and DNS", "H1c"),
        P(
            "With Resolve names on, each cell prefers a human form. Reverse DNS (PTR) runs "
            "on a background pool. If PTR returns a hostname, a forward A lookup tries to "
            "attach an IPv4. IPv6 sockets stay in the table (they are real); the cell can still "
            "show an IPv4 next to the name when DNS has one.",
            "Body",
        ),
    ]
    out.append(
        table(
            [
                [cell("What DNS knows", True), cell("Cell looks like", True)],
                [cell("Hostname and IPv4 (socket is v4, or A record from the name)"), cell("<font face='Courier'>mail.google.com (142.250.105.101):993</font>")],
                [cell("Hostname, IPv6 only"), cell("<font face='Courier'>mail.google.com ([2607:f8b0:…]):993</font>")],
                [cell("IPv4-mapped IPv6, no name"), cell("<font face='Courier'>8.8.8.8:443</font>")],
                [cell("No PTR"), cell("Numeric IPv4 or compressed IPv6 with port")],
                [cell("Unspecified"), cell("<font face='Courier'>*:443</font>")],
            ],
            [3.2 * inch, usable - 3.2 * inch],
        )
    )
    out += [
        Spacer(1, 8),
        P(
            "Dual-stack apps often show two rows (TCP4 and TCP6) to the same service. That is "
            "correct, not a duplicate bug. Uncheck Resolve names (or press N) for pure numerics. "
            "Failed lookups are not retried until the app restarts.",
            "Body",
        ),
    ]

    # 6
    out += [
        P("6. CyClaw telemetry-kill recipe", "H1c"),
        P(
            "CyClaw’s telemetry-kill switch is supposed to stop vendor SDKs from phoning home. "
            "This app is the eyeball check on the socket table. Do not turn telemetry on to "
            "test the kill switch. Watch whether CyClaw’s process grows off-box rows.",
            "Body",
        ),
        P("Setup", "H2c"),
        bullets(
            [
                "Launch CyClaw-Net-Viewer. Turn on Off-box only and Color remotes.",
                "Set rate to 0.5s or 1s. Clear the filter, or set it to the CyClaw process name or PID.",
                "Note the orange baseline: browsers, Mail, and the system will already have remotes. That is normal.",
            ]
        ),
        P("Run the subject", "H2c"),
        bullets(
            [
                "Start CyClaw (or the binary under test) with telemetry-kill as you would in production.",
                "Watch for new green Out rows whose Path is the CyClaw binary, then orange while they live.",
                "Filter on the CyClaw PID once you know it (Activity Monitor or the Path column).",
            ]
        ),
        P("How to read the result", "H2c"),
    ]
    out.append(
        table(
            [
                [cell("What you see", True), cell("What it means", True)],
                [
                    cell("No new orange/green rows whose Path is CyClaw"),
                    cell("Socket-level pass for this run. Pair with the static otel-hardening checker; sockets are not the whole story (DNS-over-HTTPS inside another process, delayed connects)."),
                ],
                [
                    cell("Green then orange to a vendor host from CyClaw"),
                    cell("Kill switch did not hold for that path. Copy the line (right-click) and keep the remote hostname/IPv4."),
                ],
                [
                    cell("Red after you quit CyClaw"),
                    cell("Those sockets closed. Expected. Red lingers two ticks."),
                ],
                [
                    cell("Ping to the vendor lights nothing"),
                    cell("Expected. Ping is ICMP. Use curl or the real process."),
                ],
            ],
            [2.6 * inch, usable - 2.6 * inch],
        )
    )
    out += [
        Spacer(1, 8),
        P(
            "Always keep the static CyClaw checker in the loop. In a CyClaw checkout run:",
            "Body",
        ),
        P("python .claude/skills/otel-hardening/check_otel.py", "CodeBlock"),
        P(
            "This GUI only sees sockets that exist on this Mac at refresh time. "
            "Sockets are not the whole story: delayed connects and traffic inside another process will not show as CyClaw.",
            "Body",
        ),
    ]

    # 7
    out += [
        P("7. Keyboard and menus", "H1c"),
    ]
    out.append(
        table(
            [
                [cell("Key / menu", True), cell("Action", True)],
                [cell("Space"), cell("Pause / resume (ignored while the filter box is focused).")],
                [cell("R"), cell("Refresh now.")],
                [cell("N"), cell("Toggle Resolve names.")],
                [cell("Delete or Backspace"), cell("Terminate the selected process, after confirm.")],
                [cell("Click header"), cell("Sort; click again to reverse.")],
                [cell("File → Save CSV"), cell("Write cyclaw-net-viewer-YYYYMMDD-HHMMSS.csv in the current working directory (often your home if you launched the .app from Finder).")],
                [cell("File → Close Connection"), cell("Enabled only for a selected ESTABLISHED TCP row. Same as terminate process.")],
                [cell("File → Quit"), cell("Exit.")],
                [cell("Options"), cell("Resolve names, Show listeners, Show UDP, Color remotes, Off-box only, refresh rate.")],
                [cell("Help → About"), cell("Version, color summary, Darwin TCB note.")],
            ],
            [2.1 * inch, usable - 2.1 * inch],
        )
    )

    # 8
    out += [
        P("8. Close Connection on Darwin", "H1c"),
        P(
            "Windows TCPView can abort another process’s ESTABLISHED TCP by deleting the "
            "kernel TCB (<font face='Courier'>SetTcpEntry</font>). macOS has no public equivalent "
            "that is safe and unsandboxed. CyClaw-Net-Viewer therefore asks you to confirm "
            "SIGTERM of the owning PID. That ends the whole process, not just one socket.",
            "Body",
        ),
        bullets(
            [
                "The dialog states this clearly. Cancel if you only meant to drop one connection.",
                "The app refuses PID 0 and refuses to signal itself.",
                "There is no RST injector and no sudo helper in this version.",
            ]
        ),
    ]

    # 9
    out += [
        P("9. Command-line mode (Tcpvcon-style)", "H1c"),
        P(
            "The same binary prints a snapshot without the GUI. Default is ESTABLISHED TCP only, "
            "matching Tcpvcon. Reverse DNS is on unless you pass <font face='Courier'>-n</font>.",
            "Body",
        ),
        P(
            "netboard --cli [-a] [-c] [-n] [process|pid]<br/><br/>"
            "-a &nbsp;&nbsp; all TCP and UDP endpoints<br/>"
            "-c &nbsp;&nbsp; CSV (Process,PID,Proto,Dir,Local,Remote,State,Path)<br/>"
            "-n &nbsp;&nbsp; numeric addresses, no DNS<br/>"
            "filter &nbsp; optional PID (digits) or process-name substring",
            "CodeBlock",
        ),
        P(
            "Examples: <font face='Courier'>netboard --cli -n -a</font> for a fast full dump; "
            "<font face='Courier'>netboard --cli -c python</font> for CSV of processes whose "
            "name contains “python”. GUI is the default when <font face='Courier'>--cli</font> is omitted.",
            "Body",
        ),
    ]

    # 10
    out += [
        P("10. Limits and known issues", "H1c"),
    ]
    out.append(
        table(
            [
                [cell("Limit", True), cell("Detail", True)],
                [cell("SIP / TCC"), cell("Protected processes often omit FDs without root. The app still runs unprivileged.")],
                [cell("UDP remotes"), cell("The socket enumerator does not expose connected UDP peers. Those rows show *:0 / Unknown; local bindings do not establish packet direction.")],
                [cell("No byte counters"), cell("Darwin socket_info does not give cheap cumulative bytes without packet tap.")],
                [cell("First tick"), cell("Walking every PID can take a beat; then it settles at the chosen rate.")],
                [cell("Unsigned app"), cell("Ad-hoc codesign only. Notarization is out of scope.")],
                [cell("Mutex / panic"), cell("If a snapshot panics, the last good rows stay on screen and the status bar reports it.")],
            ],
            [1.5 * inch, usable - 1.5 * inch],
        )
    )

    # 11
    out += [
        P("11. Optional later work (not in 0.1.0)", "H1c"),
        P(
            "The app is enough for live watching and a kill-switch eyeball check. If you want "
            "more later, these are the useful ones — not a promise they will ship:",
            "Body",
        ),
        bullets(
            [
                "Remember Off-box only, rate, and window size across launches.",
                "Filter by executable path, not only short process name.",
                "Pair dual-stack TCP4/TCP6 to the same peer on one row (without dropping IPv6).",
                "Connected UDP remotes if a future enumerator exposes them.",
                "Notarized Developer ID build for painless Gatekeeper.",
                "Optional root RST closer (still not a real TCB delete).",
            ]
        ),
        P(
            "Do not add ICMP to the table, App Sandbox, or a webview rewrite. Those fight the "
            "point of a local, unsandboxed socket list.",
            "Body",
        ),
        P("12. Credits", "H1c"),
        P(
            "Inspiration: Sysinternals TCPView by Mark Russinovich. This tool is an independent "
            "macOS implementation under the MIT License and is not affiliated with Microsoft. "
            "Socket listing uses Darwin libproc (the same family of APIs as lsof). "
            "Source lives in this repository. Crate name: <font face='Courier'>netboard</font>.",
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
        title="CyClaw-Net-Viewer User Guide",
        author="CyClaw-Net-Viewer",
        subject="macOS TCP/UDP endpoint viewer user guide",
    )
    doc.build(story(), onFirstPage=header_footer, onLaterPages=header_footer)
    print(OUT)


if __name__ == "__main__":
    main()
