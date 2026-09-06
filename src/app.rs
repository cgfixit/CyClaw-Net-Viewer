use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use eframe::egui::{self, Color32, Key, RichText, Sense, TextEdit, Vec2};
use egui_extras::{Column, TableBuilder};

use crate::diff::{diff, Highlight, Row};
use crate::dns::Resolver;
use crate::kill;
use crate::snapshot::{fmt_addr, is_offbox, snapshot, twin_ipv4, EndpointKey, Proto, TcpState};

pub const APP_TITLE: &str = "CyClaw-Net-Viewer";

struct HighlightPalette {
    new_out: (Color32, Color32),
    new_in: (Color32, Color32),
    changed: (Color32, Color32),
    deleted: (Color32, Color32),
    offbox: (Color32, Color32),
}

impl HighlightPalette {
    const LIGHT: Self = Self {
        new_out: (
            Color32::from_rgb(0xC6, 0xEF, 0xCE),
            Color32::from_rgb(0x00, 0x61, 0x00),
        ),
        new_in: (
            Color32::from_rgb(0xBD, 0xD7, 0xEE),
            Color32::from_rgb(0x1F, 0x4E, 0x79),
        ),
        changed: (
            Color32::from_rgb(0xFF, 0xEB, 0x9C),
            Color32::from_rgb(0x9C, 0x57, 0x00),
        ),
        deleted: (
            Color32::from_rgb(0xFF, 0xC7, 0xCE),
            Color32::from_rgb(0x9C, 0x00, 0x06),
        ),
        offbox: (
            Color32::from_rgb(0xF8, 0xCB, 0xAD),
            Color32::from_rgb(0x84, 0x3C, 0x0B),
        ),
    };
    const DARK: Self = Self {
        new_out: (
            Color32::from_rgb(0x43, 0xA0, 0x47),
            Color32::from_rgb(0xE8, 0xF5, 0xE9),
        ),
        new_in: (
            Color32::from_rgb(0x42, 0xA5, 0xF5),
            Color32::from_rgb(0xE3, 0xF2, 0xFD),
        ),
        changed: (
            Color32::from_rgb(0xFF, 0xB7, 0x4D),
            Color32::from_rgb(0x3E, 0x27, 0x23),
        ),
        deleted: (
            Color32::from_rgb(0xEF, 0x53, 0x50),
            Color32::from_rgb(0xFF, 0xEB, 0xEE),
        ),
        offbox: (
            Color32::from_rgb(0xFF, 0x8A, 0x65),
            Color32::from_rgb(0x3E, 0x27, 0x23),
        ),
    };

    fn for_mode(dark: bool) -> &'static Self {
        if dark {
            &Self::DARK
        } else {
            &Self::LIGHT
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct Shared {
    rows: Mutex<Arc<Vec<Row>>>,
    err: Mutex<Option<String>>,
    paused: AtomicBool,
    interval_ms: AtomicU64,
    kick: AtomicBool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortCol {
    Process,
    Pid,
    Proto,
    Dir,
    Local,
    Remote,
    State,
    Path,
}

pub struct NetBoardApp {
    shared: Arc<Shared>,
    resolver: Resolver,
    resolve_names: bool,
    filter: String,
    sort: SortCol,
    sort_asc: bool,
    selected: Option<EndpointKey>,
    show_udp: bool,
    show_listen: bool,
    color_offbox: bool,
    offbox_only: bool,
    show_about: bool,
    pending_kill: Option<(u32, String)>,
    last_save: Option<String>,
    interval_choice: u64,
    filter_focused: bool,
}

impl NetBoardApp {
    /// Numeric addresses until the user enables Resolve names.
    const DEFAULT_RESOLVE_NAMES: bool = false;

    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let shared = Arc::new(Shared {
            rows: Mutex::new(Arc::new(Vec::new())),
            err: Mutex::new(None),
            paused: AtomicBool::new(false),
            interval_ms: AtomicU64::new(1000),
            kick: AtomicBool::new(true),
        });
        let ctx = cc.egui_ctx.clone();
        let thread_shared = shared.clone();
        thread::Builder::new()
            .name("netboard-snap".into())
            .spawn(move || snap_loop(thread_shared, ctx))
            .expect("snapshot thread");

        Self {
            shared,
            resolver: Resolver::new(),
            resolve_names: Self::DEFAULT_RESOLVE_NAMES,
            filter: String::new(),
            sort: SortCol::Process,
            sort_asc: true,
            selected: None,
            show_udp: true,
            show_listen: true,
            color_offbox: true,
            offbox_only: false,
            show_about: false,
            pending_kill: None,
            last_save: None,
            interval_choice: 1000,
            filter_focused: false,
        }
    }
}

fn snap_loop(shared: Arc<Shared>, ctx: egui::Context) {
    let mut prev = Arc::new(Vec::new());
    loop {
        wait_tick(&shared);
        let snapped = catch_unwind(AssertUnwindSafe(snapshot));
        match snapped {
            Ok(Ok(eps)) => {
                let next = Arc::new(diff(&prev, &eps));
                // Publish only a complete immutable snapshot. Readers retain
                // their Arc for a full frame; no row data is copied under lock.
                let old = std::mem::replace(&mut *lock(&shared.rows), Arc::clone(&next));
                drop(old); // Free old rows outside the publication lock.
                prev = next;
                *lock(&shared.err) = None;
            }
            Ok(Err(e)) => {
                *lock(&shared.err) = Some(e.0);
            }
            Err(_) => {
                *lock(&shared.err) = Some("snapshot panicked; showing last good rows".into());
            }
        }
        ctx.request_repaint();
    }
}

fn wait_tick(shared: &Shared) {
    let mut waited = 0u64;
    loop {
        if shared.kick.swap(false, Ordering::Relaxed) {
            return;
        }
        if shared.paused.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(50));
            waited = 0;
            continue;
        }
        let iv = shared.interval_ms.load(Ordering::Relaxed).max(50);
        if waited >= iv {
            return;
        }
        thread::sleep(Duration::from_millis(50));
        waited += 50;
    }
}

fn hl_colors(
    h: Highlight,
    remote: std::net::SocketAddr,
    color_offbox: bool,
    dark: bool,
) -> Option<(Color32, Color32)> {
    let palette = HighlightPalette::for_mode(dark);
    match h {
        Highlight::NewOut => Some(palette.new_out),
        Highlight::NewIn => Some(palette.new_in),
        Highlight::Changed => Some(palette.changed),
        Highlight::Deleted => Some(palette.deleted),
        Highlight::None if color_offbox && is_offbox(remote) => Some(palette.offbox),
        Highlight::None => None,
    }
}

fn twin_ipv4_for(key: &EndpointKey, current: &[Row]) -> Option<std::net::Ipv4Addr> {
    twin_ipv4(
        key.pid,
        key.proto,
        key.remote,
        current.iter().map(|r| {
            let k = &r.endpoint.key;
            (k.pid, k.proto, k.remote)
        }),
    )
}

pub fn run() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([800.0, 400.0])
            .with_title(APP_TITLE),
        ..Default::default()
    };
    eframe::run_native(
        APP_TITLE,
        opts,
        Box::new(|cc| Ok(Box::new(NetBoardApp::new(cc)))),
    )
}

impl eframe::App for NetBoardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.hotkeys(ctx);

        let rows = Arc::clone(&lock(&self.shared.rows));
        let err = lock(&self.shared.err).clone();
        let paused = self.shared.paused.load(Ordering::Relaxed);

        self.enqueue_name_lookups(&rows);

        let mut visible = self.visible(&rows);
        self.sort_rows(&mut visible);

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            self.menu(ui, &visible, &rows);
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(paused, if paused { "Paused" } else { "Pause" })
                    .clicked()
                {
                    self.shared.paused.store(!paused, Ordering::Relaxed);
                }
                if ui.button("Refresh").clicked() {
                    self.shared.kick.store(true, Ordering::Relaxed);
                }
                ui.checkbox(&mut self.resolve_names, "Resolve names");
                ui.checkbox(&mut self.color_offbox, "Color remotes");
                ui.checkbox(&mut self.offbox_only, "Off-box only");
                ui.label("Rate");
                egui::ComboBox::from_id_salt("rate")
                    .selected_text(rate_label(self.interval_choice))
                    .show_ui(ui, |ui| {
                        for ms in [500u64, 1000, 2000, 5000] {
                            if ui
                                .selectable_label(self.interval_choice == ms, rate_label(ms))
                                .clicked()
                            {
                                self.interval_choice = ms;
                                self.shared.interval_ms.store(ms, Ordering::Relaxed);
                            }
                        }
                    });
                let filter = ui.add(
                    TextEdit::singleline(&mut self.filter)
                        .hint_text("Filter process, PID, host, port…")
                        .desired_width(240.0),
                );
                self.filter_focused = filter.has_focus();
            });
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let off = visible
                    .iter()
                    .filter(|r| is_offbox(r.endpoint.key.remote))
                    .count();
                ui.label(format!("{} endpoints · {} off-box", visible.len(), off));
                if paused {
                    ui.label("paused");
                }
                ui.separator();
                legend(ui, ui.visuals().dark_mode);
                if let Some(e) = &err {
                    ui.separator();
                    ui.colored_label(Color32::RED, e);
                }
                if let Some(s) = &self.last_save {
                    ui.separator();
                    ui.label(s);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.table(ui, &visible, &rows);
        });

        self.modals(ctx);
    }
}

impl NetBoardApp {
    fn enqueue_name_lookups(&self, rows: &[Row]) {
        if !self.resolve_names {
            return;
        }
        for r in rows {
            self.resolver.request(r.endpoint.key.local.ip());
            self.resolver.request(r.endpoint.key.remote.ip());
        }
    }

    fn hotkeys(&mut self, ctx: &egui::Context) {
        let mut pause = false;
        let mut refresh = false;
        let mut names = false;
        let mut kill = false;
        ctx.input(|i| {
            if i.key_pressed(Key::Space) {
                pause = true;
            }
            if i.key_pressed(Key::R) {
                refresh = true;
            }
            if i.key_pressed(Key::N) {
                names = true;
            }
            if i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace) {
                kill = true;
            }
            if i.key_pressed(Key::Slash) {
                // focus handled by clicking filter; slash is documented
            }
        });
        if self.filter_focused {
            return;
        }
        if pause {
            let p = self.shared.paused.load(Ordering::Relaxed);
            self.shared.paused.store(!p, Ordering::Relaxed);
        }
        if refresh {
            self.shared.kick.store(true, Ordering::Relaxed);
        }
        if names {
            self.resolve_names = !self.resolve_names;
        }
        if kill {
            self.ask_kill_selected();
        }
    }

    fn menu(&mut self, ui: &mut egui::Ui, visible: &[&Row], current: &[Row]) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Save CSV").clicked() {
                    self.save_csv(visible, current);
                    ui.close_menu();
                }
                let can_close = self.selected_established(visible).is_some();
                ui.add_enabled_ui(can_close, |ui| {
                    if ui.button("Terminate process…").clicked() {
                        self.ask_kill_selected();
                        ui.close_menu();
                    }
                });
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("Options", |ui| {
                ui.checkbox(&mut self.resolve_names, "Resolve names");
                ui.checkbox(&mut self.show_listen, "Show listeners");
                ui.checkbox(&mut self.show_udp, "Show UDP");
                ui.checkbox(&mut self.color_offbox, "Color remotes");
                ui.checkbox(&mut self.offbox_only, "Off-box only");
                ui.separator();
                ui.label("Refresh rate");
                for ms in [500u64, 1000, 2000, 5000] {
                    if ui
                        .selectable_label(self.interval_choice == ms, rate_label(ms))
                        .clicked()
                    {
                        self.interval_choice = ms;
                        self.shared.interval_ms.store(ms, Ordering::Relaxed);
                    }
                }
            });
            ui.menu_button("Help", |ui| {
                if ui.button("About").clicked() {
                    self.show_about = true;
                    ui.close_menu();
                }
            });
        });
    }

    fn table(&mut self, ui: &mut egui::Ui, visible: &[&Row], current: &[Row]) {
        let dark = ui.visuals().dark_mode;
        let default_fg = ui.visuals().text_color();
        let mut clicked: Option<EndpointKey> = None;
        let mut kill: Option<(u32, String)> = None;
        let mut copy_line = None;
        let mut copy_remote = None;
        let mut reveal: Option<String> = None;
        let mut new_sort = None;

        let mut header = |ui: &mut egui::Ui, col: SortCol, label: &str| {
            let mark = if self.sort == col {
                if self.sort_asc {
                    " ▾"
                } else {
                    " ▴"
                }
            } else {
                ""
            };
            if ui.button(format!("{label}{mark}")).clicked() {
                new_sort = Some(col);
            }
        };

        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .min_scrolled_height(120.0)
            .sense(Sense::click())
            .column(Column::initial(140.0))
            .column(Column::initial(60.0))
            .column(Column::initial(60.0))
            .column(Column::initial(60.0))
            .column(Column::initial(220.0))
            .column(Column::initial(220.0))
            .column(Column::initial(110.0))
            .column(Column::remainder())
            .header(24.0, |mut row| {
                row.col(|ui| header(ui, SortCol::Process, "Process"));
                row.col(|ui| header(ui, SortCol::Pid, "PID"));
                row.col(|ui| header(ui, SortCol::Proto, "Proto"));
                row.col(|ui| header(ui, SortCol::Dir, "Dir"));
                row.col(|ui| header(ui, SortCol::Local, "Local"));
                row.col(|ui| header(ui, SortCol::Remote, "Remote"));
                row.col(|ui| header(ui, SortCol::State, "State"));
                row.col(|ui| header(ui, SortCol::Path, "Path"));
            })
            .body(|body| {
                body.rows(22.0, visible.len(), |mut row| {
                    let i = row.index();
                    let r = &visible[i];
                    let selected = self.selected.as_ref() == Some(&r.endpoint.key);
                    row.set_selected(selected);
                    let (bg, fg) = match hl_colors(
                        r.highlight,
                        r.endpoint.key.remote,
                        self.color_offbox,
                        dark,
                    ) {
                        Some(c) => c,
                        None => (Color32::TRANSPARENT, default_fg),
                    };
                    let local = self.fmt_ep(r.endpoint.key.local, None);
                    let remote = self.fmt_ep(
                        r.endpoint.key.remote,
                        twin_ipv4_for(&r.endpoint.key, current),
                    );
                    let pid_s = r.endpoint.key.pid.to_string();
                    let cells = [
                        (r.endpoint.process.as_str(), false),
                        (pid_s.as_str(), true),
                        (r.endpoint.proto_label(), true),
                        (r.endpoint.dir_label(), false),
                        (local.as_str(), true),
                        (remote.as_str(), true),
                        (r.endpoint.state_label(), true),
                        (r.endpoint.path.as_str(), false),
                    ];
                    for (c, mono) in cells {
                        row.col(|ui| {
                            if bg != Color32::TRANSPARENT {
                                ui.painter().rect_filled(ui.max_rect(), 0.0, bg);
                            }
                            let mut t = RichText::new(c).color(fg);
                            if mono {
                                t = t.monospace();
                            }
                            ui.label(t);
                        });
                    }
                    let resp = row.response();
                    if resp.clicked() {
                        clicked = Some(r.endpoint.key.clone());
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Copy line").clicked() {
                            copy_line = Some(format!(
                                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                                r.endpoint.process,
                                r.endpoint.key.pid,
                                r.endpoint.proto_label(),
                                r.endpoint.dir_label(),
                                local,
                                remote,
                                r.endpoint.state_label(),
                                r.endpoint.path
                            ));
                            ui.close_menu();
                        }
                        if ui.button("Copy remote").clicked() {
                            copy_remote = Some(remote.clone());
                            ui.close_menu();
                        }
                        if ui.button("Terminate process…").clicked() {
                            kill = Some((r.endpoint.key.pid, r.endpoint.process.clone()));
                            ui.close_menu();
                        }
                        if !r.endpoint.path.is_empty() && ui.button("Reveal in Finder").clicked() {
                            reveal = Some(r.endpoint.path.clone());
                            ui.close_menu();
                        }
                    });
                });
            });

        if let Some(col) = new_sort {
            if self.sort == col {
                self.sort_asc = !self.sort_asc;
            } else {
                self.sort = col;
                self.sort_asc = true;
            }
        }
        if let Some(k) = clicked {
            self.selected = Some(k);
        }
        if let Some(s) = copy_line {
            ui.ctx().copy_text(s);
        }
        if let Some(s) = copy_remote {
            ui.ctx().copy_text(s);
        }
        if let Some(k) = kill {
            self.pending_kill = Some(k);
        }
        if let Some(p) = reveal {
            let _ = std::process::Command::new("open").args(["-R", &p]).spawn();
        }
    }

    fn fmt_ep(&self, addr: std::net::SocketAddr, extra_ipv4: Option<std::net::Ipv4Addr>) -> String {
        if self.resolve_names {
            let r = self.resolver.get(addr.ip());
            fmt_addr(
                addr,
                r.as_ref().map(|x| x.host.as_str()),
                r.as_ref().and_then(|x| x.ipv4).or(extra_ipv4),
            )
        } else {
            fmt_addr(addr, None, extra_ipv4)
        }
    }

    fn visible<'a>(&self, rows: &'a [Row]) -> Vec<&'a Row> {
        let f = self.filter.to_ascii_lowercase();
        rows.iter()
            .filter(|r| {
                if !self.show_udp && r.endpoint.key.proto == Proto::Udp {
                    return false;
                }
                if !self.show_listen && r.endpoint.dir == crate::Dir::Listen {
                    return false;
                }
                if self.offbox_only && !is_offbox(r.endpoint.key.remote) {
                    return false;
                }
                if f.is_empty() {
                    return true;
                }
                let local = self.fmt_ep(r.endpoint.key.local, None);
                let remote =
                    self.fmt_ep(r.endpoint.key.remote, twin_ipv4_for(&r.endpoint.key, rows));
                let mut blob = format!(
                    "{} {} {} {} {} {} {} {}",
                    r.endpoint.process,
                    r.endpoint.key.pid,
                    r.endpoint.proto_label(),
                    r.endpoint.dir_label(),
                    local,
                    remote,
                    r.endpoint.state_label(),
                    r.endpoint.path
                );
                blob.make_ascii_lowercase();
                blob.contains(&f)
            })
            .collect()
    }

    fn sort_rows(&self, rows: &mut [&Row]) {
        rows.sort_by(|a, b| {
            let ord = match self.sort {
                SortCol::Process => a.endpoint.process.cmp(&b.endpoint.process),
                SortCol::Pid => a.endpoint.key.pid.cmp(&b.endpoint.key.pid),
                SortCol::Proto => a.endpoint.proto_label().cmp(b.endpoint.proto_label()),
                SortCol::Dir => a.endpoint.dir_label().cmp(b.endpoint.dir_label()),
                SortCol::Local => a.endpoint.key.local.cmp(&b.endpoint.key.local),
                SortCol::Remote => a.endpoint.key.remote.cmp(&b.endpoint.key.remote),
                SortCol::State => a.endpoint.state_label().cmp(b.endpoint.state_label()),
                SortCol::Path => a.endpoint.path.cmp(&b.endpoint.path),
            };
            if self.sort_asc {
                ord
            } else {
                ord.reverse()
            }
        });
    }

    fn selected_row<'a>(&self, visible: &[&'a Row]) -> Option<&'a Row> {
        let k = self.selected.as_ref()?;
        visible.iter().copied().find(|r| &r.endpoint.key == k)
    }

    fn selected_established<'a>(&self, visible: &[&'a Row]) -> Option<&'a Row> {
        self.selected_row(visible)
            .filter(|r| r.endpoint.state == Some(TcpState::Established))
    }

    fn ask_kill_selected(&mut self) {
        let rows = Arc::clone(&lock(&self.shared.rows));
        let vis = self.visible(&rows);
        if let Some(r) = self.selected_row(&vis) {
            self.pending_kill = Some((r.endpoint.key.pid, r.endpoint.process.clone()));
        }
    }

    fn save_csv(&mut self, visible: &[&Row], current: &[Row]) {
        let name = csv_filename();
        let mut buf = String::from("Process,PID,Proto,Dir,Local,Remote,State,Path\n");
        for r in visible {
            let local = self.fmt_ep(r.endpoint.key.local, None);
            let remote = self.fmt_ep(
                r.endpoint.key.remote,
                twin_ipv4_for(&r.endpoint.key, current),
            );
            buf.push_str(&format!(
                "{},{},{},{},{},{},{},{}\n",
                crate::csv_escape(&r.endpoint.process),
                r.endpoint.key.pid,
                r.endpoint.proto_label(),
                r.endpoint.dir_label(),
                crate::csv_escape(&local),
                crate::csv_escape(&remote),
                r.endpoint.state_label(),
                crate::csv_escape(&r.endpoint.path)
            ));
        }
        match crate::export::save_new(std::path::Path::new(&name), &buf) {
            Ok(()) => self.last_save = Some(format!("saved {name}")),
            Err(e) => self.last_save = Some(format!("save failed: {e}")),
        }
    }

    fn modals(&mut self, ctx: &egui::Context) {
        if self.show_about {
            egui::Window::new(format!("About {APP_TITLE}"))
                .collapsible(false)
                .open(&mut self.show_about)
                .show(ctx, |ui| {
                    ui.label(RichText::new(format!("{APP_TITLE} 0.1.0")).strong());
                    ui.label("Live TCP/UDP endpoints on macOS, for CyClaw telemetry-kill watching.");
                    ui.label("MIT. Not affiliated with Microsoft or Sysinternals.");
                    ui.label("Orange = TCP/UDP to an off-box remote (loopback/* stay uncolored). Event colors override: green new out, blue new in, yellow state change, red gone.");
                    ui.label("ICMP/ping is not a TCP/UDP socket and will not appear.");
                    ui.label("Darwin cannot delete another process's TCB; Terminate process sends SIGTERM to the owning process after confirm.");
                });
        }
        if let Some((pid, name)) = self.pending_kill.clone() {
            let mut open = true;
            egui::Window::new("Terminate process")
                .collapsible(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Terminate {name} (PID {pid})?\nThis ends the process, not just the socket.\nDarwin has no TCPView-style TCB delete."
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Terminate").clicked() {
                            match kill::terminate(pid) {
                                Ok(()) => self.last_save = Some(format!("SIGTERM {name} ({pid})")),
                                Err(e) => self.last_save = Some(e),
                            }
                            self.pending_kill = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending_kill = None;
                        }
                    });
                });
            if !open {
                self.pending_kill = None;
            }
        }
    }
}

fn legend(ui: &mut egui::Ui, dark: bool) {
    let palette = HighlightPalette::for_mode(dark);
    let items = [
        ("New out", palette.new_out.0),
        ("New in", palette.new_in.0),
        ("Changed", palette.changed.0),
        ("Gone", palette.deleted.0),
        ("Off-box", palette.offbox.0),
    ];
    for (label, color) in items {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(10.0, 10.0), Sense::hover());
        ui.painter().rect_filled(rect, 2.0, color);
        ui.label(label);
    }
}

fn rate_label(ms: u64) -> &'static str {
    match ms {
        500 => "0.5s",
        1000 => "1s",
        2000 => "2s",
        5000 => "5s",
        _ => "1s",
    }
}

fn csv_filename() -> String {
    // SAFETY: a null output pointer asks time() to return the value only.
    let t = unsafe { libc::time(std::ptr::null_mut()) };
    let mut tm = std::mem::MaybeUninit::<libc::tm>::uninit();
    // SAFETY: both pointers are valid for the call. localtime_r writes to our
    // own storage, avoiding localtime's shared static buffer across threads.
    let ptr = unsafe { libc::localtime_r(&t, tm.as_mut_ptr()) };
    if ptr.is_null() {
        return "cyclaw-net-viewer.csv".into();
    }
    // SAFETY: the non-null return confirms the output was initialized.
    let tm = unsafe { tm.assume_init() };
    format!(
        "cyclaw-net-viewer-{:04}{:02}{:02}-{:02}{:02}{:02}.csv",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filtering_sorting_and_selection_borrow_a_stable_frame() {
        use crate::{Dir, Endpoint, IpVer};
        let endpoint = |pid| Endpoint {
            key: EndpointKey {
                pid,
                proto: Proto::Udp,
                ip_ver: IpVer::V4,
                local: "127.0.0.1:50000".parse().unwrap(),
                remote: "0.0.0.0:0".parse().unwrap(),
            },
            state: None,
            dir: Dir::Unknown,
            process: "Example café".into(),
            path: "/Applications/Example.app".into(),
        };
        let initial = Arc::new(diff(&[], &[endpoint(2), endpoint(1)]));
        let mut app = NetBoardApp {
            shared: Arc::new(Shared {
                rows: Mutex::new(Arc::clone(&initial)),
                err: Mutex::new(None),
                paused: AtomicBool::new(false),
                interval_ms: AtomicU64::new(1000),
                kick: AtomicBool::new(false),
            }),
            resolver: Resolver::new(),
            resolve_names: false,
            filter: "unknown".into(),
            sort: SortCol::Pid,
            sort_asc: true,
            selected: Some(initial[0].endpoint.key.clone()),
            show_udp: true,
            show_listen: false,
            color_offbox: true,
            offbox_only: false,
            show_about: false,
            pending_kill: None,
            last_save: None,
            interval_choice: 1000,
            filter_focused: false,
        };
        let frame = Arc::clone(&lock(&app.shared.rows));
        assert!(Arc::ptr_eq(&initial, &frame));
        let mut visible = app.visible(&frame);
        app.sort_rows(&mut visible);
        assert_eq!(visible.len(), 2); // Unknown UDP survives hiding listeners.
        assert!(std::ptr::eq(visible[0], &frame[1]));
        assert!(std::ptr::eq(app.selected_row(&visible).unwrap(), &frame[0]));
        *lock(&app.shared.rows) = Arc::new(Vec::new());
        assert_eq!(visible[1].endpoint.key.pid, 2); // Publication cannot change this frame.
        for (query, count) in [
            ("", 2),
            ("EXAMPLE", 2),
            ("2 UDP4", 1),
            ("127.0.0.1:50000", 2),
            ("50000 *:0", 2),
            ("/APPLICATIONS/EXAMPLE.APP", 2),
            ("CAFé", 2),
            ("CAFÉ", 0), // Matching folds ASCII only, preserving non-ASCII text.
            ("absent", 0),
        ] {
            app.filter = query.into();
            assert_eq!(app.visible(&frame).len(), count, "filter: {query}");
        }
        app.filter.clear();
        app.show_udp = false;
        assert!(app.visible(&frame).is_empty());
    }

    #[test]
    fn resolve_names_defaults_off_and_does_not_queue_lookups() {
        use crate::{Dir, Endpoint, IpVer};
        let endpoint = Endpoint {
            key: EndpointKey {
                pid: 1,
                proto: Proto::Tcp,
                ip_ver: IpVer::V4,
                local: "127.0.0.1:50000".parse().unwrap(),
                remote: "192.0.2.1:443".parse().unwrap(),
            },
            state: Some(TcpState::Established),
            dir: Dir::Out,
            process: "example".into(),
            path: "/example".into(),
        };
        let rows = Arc::new(diff(&[], &[endpoint]));
        let app = NetBoardApp {
            shared: Arc::new(Shared {
                rows: Mutex::new(Arc::clone(&rows)),
                err: Mutex::new(None),
                paused: AtomicBool::new(false),
                interval_ms: AtomicU64::new(1000),
                kick: AtomicBool::new(false),
            }),
            resolver: Resolver::new(),
            resolve_names: NetBoardApp::DEFAULT_RESOLVE_NAMES,
            filter: String::new(),
            sort: SortCol::Process,
            sort_asc: true,
            selected: None,
            show_udp: true,
            show_listen: true,
            color_offbox: true,
            offbox_only: false,
            show_about: false,
            pending_kill: None,
            last_save: None,
            interval_choice: 1000,
            filter_focused: false,
        };
        app.enqueue_name_lookups(&rows);
        assert!(!app.resolver.contains("127.0.0.1".parse().unwrap()));
        assert!(!app.resolver.contains("192.0.2.1".parse().unwrap()));
    }

    #[test]
    fn both_palettes_cover_events_and_preserve_offbox_precedence() {
        let remote = "192.0.2.1:443".parse().unwrap();
        for dark in [false, true] {
            let p = HighlightPalette::for_mode(dark);
            for (highlight, expected) in [
                (Highlight::NewIn, p.new_in),
                (Highlight::NewOut, p.new_out),
                (Highlight::Changed, p.changed),
                (Highlight::Deleted, p.deleted),
            ] {
                assert_eq!(hl_colors(highlight, remote, true, dark), Some(expected));
                assert_eq!(hl_colors(highlight, remote, false, dark), Some(expected));
            }
            assert_eq!(
                hl_colors(Highlight::None, remote, true, dark),
                Some(p.offbox)
            );
            assert_eq!(hl_colors(Highlight::None, remote, false, dark), None);
            assert_eq!(
                hl_colors(
                    Highlight::None,
                    "127.0.0.1:443".parse().unwrap(),
                    true,
                    dark
                ),
                None
            );
        }
    }
}
