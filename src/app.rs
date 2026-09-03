use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use eframe::egui::{self, Color32, Key, RichText, Sense};
use egui_extras::{Column, TableBuilder};

use crate::diff::{diff, Highlight, Row};
use crate::dns::Resolver;
use crate::kill;
use crate::snapshot::{fmt_addr, is_offbox, snapshot, EndpointKey, Proto, TcpState};

const BG_NEW_OUT: Color32 = Color32::from_rgb(0xC6, 0xEF, 0xCE);
const FG_NEW_OUT: Color32 = Color32::from_rgb(0x00, 0x61, 0x00);
const BG_NEW_IN: Color32 = Color32::from_rgb(0xBD, 0xD7, 0xEE);
const FG_NEW_IN: Color32 = Color32::from_rgb(0x1F, 0x4E, 0x79);
const BG_CHANGED: Color32 = Color32::from_rgb(0xFF, 0xEB, 0x9C);
const FG_CHANGED: Color32 = Color32::from_rgb(0x9C, 0x57, 0x00);
const BG_DELETED: Color32 = Color32::from_rgb(0xFF, 0xC7, 0xCE);
const FG_DELETED: Color32 = Color32::from_rgb(0x9C, 0x00, 0x06);
const BG_OFFBOX: Color32 = Color32::from_rgb(0xF8, 0xCB, 0xAD);
const FG_OFFBOX: Color32 = Color32::from_rgb(0x84, 0x3C, 0x0B);

struct Shared {
    rows: Mutex<Vec<Row>>,
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
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let shared = Arc::new(Shared {
            rows: Mutex::new(Vec::new()),
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
            resolve_names: true,
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
    let mut prev: Vec<Row> = Vec::new();
    loop {
        wait_tick(&shared);
        match snapshot() {
            Ok(eps) => {
                let next = diff(&prev, &eps);
                prev = next.clone();
                *shared.rows.lock().expect("rows") = next;
                *shared.err.lock().expect("err") = None;
            }
            Err(e) => {
                *shared.err.lock().expect("err") = Some(e.0);
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
) -> Option<(Color32, Color32)> {
    match h {
        Highlight::NewOut => Some((BG_NEW_OUT, FG_NEW_OUT)),
        Highlight::NewIn => Some((BG_NEW_IN, FG_NEW_IN)),
        Highlight::Changed => Some((BG_CHANGED, FG_CHANGED)),
        Highlight::Deleted => Some((BG_DELETED, FG_DELETED)),
        Highlight::None if color_offbox && is_offbox(remote) => Some((BG_OFFBOX, FG_OFFBOX)),
        Highlight::None => None,
    }
}

pub fn run() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([800.0, 400.0])
            .with_title("NetBoard"),
        ..Default::default()
    };
    eframe::run_native(
        "NetBoard",
        opts,
        Box::new(|cc| Ok(Box::new(NetBoardApp::new(cc)))),
    )
}

impl eframe::App for NetBoardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.hotkeys(ctx);

        let rows = self.shared.rows.lock().expect("rows").clone();
        let err = self.shared.err.lock().expect("err").clone();
        let paused = self.shared.paused.load(Ordering::Relaxed);

        for r in &rows {
            if self.resolve_names {
                self.resolver.request(r.endpoint.key.local.ip());
                self.resolver.request(r.endpoint.key.remote.ip());
            }
        }

        let mut visible = self.visible(&rows);
        self.sort_rows(&mut visible);

        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            self.menu(ui, &visible);
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
                ui.label("Filter");
                let filter = ui.text_edit_singleline(&mut self.filter);
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
                if let Some(e) = &err {
                    ui.colored_label(Color32::RED, e);
                }
                if let Some(s) = &self.last_save {
                    ui.label(s);
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.table(ui, &visible);
        });

        self.modals(ctx);
    }
}

impl NetBoardApp {
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

    fn menu(&mut self, ui: &mut egui::Ui, visible: &[Row]) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Save CSV").clicked() {
                    self.save_csv(visible);
                    ui.close_menu();
                }
                let can_close = self.selected_established(visible).is_some();
                ui.add_enabled_ui(can_close, |ui| {
                    if ui.button("Close Connection…").clicked() {
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

    fn table(&mut self, ui: &mut egui::Ui, visible: &[Row]) {
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
            .header(22.0, |mut row| {
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
                    let (bg, fg) =
                        match hl_colors(r.highlight, r.endpoint.key.remote, self.color_offbox) {
                            Some(c) => c,
                            None => (Color32::TRANSPARENT, default_fg),
                        };
                    let local = self.fmt_ep(r.endpoint.key.local);
                    let remote = self.fmt_ep(r.endpoint.key.remote);
                    let pid_s = r.endpoint.key.pid.to_string();
                    let cells = [
                        r.endpoint.process.as_str(),
                        pid_s.as_str(),
                        r.endpoint.proto_label(),
                        r.endpoint.dir_label(),
                        local.as_str(),
                        remote.as_str(),
                        r.endpoint.state_label(),
                        r.endpoint.path.as_str(),
                    ];
                    for c in cells {
                        row.col(|ui| {
                            if bg != Color32::TRANSPARENT {
                                ui.painter().rect_filled(ui.max_rect(), 0.0, bg);
                            }
                            ui.colored_label(fg, c);
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

    fn fmt_ep(&self, addr: std::net::SocketAddr) -> String {
        let name = if self.resolve_names {
            self.resolver.get(addr.ip())
        } else {
            None
        };
        fmt_addr(addr, name.as_deref())
    }

    fn visible(&self, rows: &[Row]) -> Vec<Row> {
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
                let local = self.fmt_ep(r.endpoint.key.local);
                let remote = self.fmt_ep(r.endpoint.key.remote);
                let blob = format!(
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
                blob.to_ascii_lowercase().contains(&f)
            })
            .cloned()
            .collect()
    }

    fn sort_rows(&self, rows: &mut [Row]) {
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

    fn selected_row<'a>(&self, visible: &'a [Row]) -> Option<&'a Row> {
        let k = self.selected.as_ref()?;
        visible.iter().find(|r| &r.endpoint.key == k)
    }

    fn selected_established<'a>(&self, visible: &'a [Row]) -> Option<&'a Row> {
        self.selected_row(visible)
            .filter(|r| r.endpoint.state == Some(TcpState::Established))
    }

    fn ask_kill_selected(&mut self) {
        let rows = self.shared.rows.lock().expect("rows").clone();
        let vis = self.visible(&rows);
        if let Some(r) = self.selected_row(&vis) {
            self.pending_kill = Some((r.endpoint.key.pid, r.endpoint.process.clone()));
        }
    }

    fn save_csv(&mut self, visible: &[Row]) {
        let name = csv_filename();
        let mut buf = String::from("Process,PID,Proto,Dir,Local,Remote,State,Path\n");
        for r in visible {
            let local = self.fmt_ep(r.endpoint.key.local);
            let remote = self.fmt_ep(r.endpoint.key.remote);
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
        match std::fs::write(&name, buf) {
            Ok(()) => self.last_save = Some(format!("saved {name}")),
            Err(e) => self.last_save = Some(format!("save failed: {e}")),
        }
    }

    fn modals(&mut self, ctx: &egui::Context) {
        if self.show_about {
            egui::Window::new("About NetBoard")
                .collapsible(false)
                .open(&mut self.show_about)
                .show(ctx, |ui| {
                    ui.label(RichText::new("NetBoard 0.1.0").strong());
                    ui.label("Live TCP/UDP endpoints on macOS.");
                    ui.label("MIT. Not affiliated with Microsoft or Sysinternals.");
                    ui.label("Orange = TCP/UDP to an off-box remote (loopback/* stay uncolored). Event colors override: green new out, blue new in, yellow state change, red gone.");
                    ui.label("ICMP/ping is not a TCP/UDP socket and will not appear.");
                    ui.label("Darwin cannot delete another process's TCB; Close Connection terminates the owning process after confirm.");
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
    let t = unsafe { libc::time(std::ptr::null_mut()) };
    let ptr = unsafe { libc::localtime(&t) };
    if ptr.is_null() {
        return "netboard.csv".into();
    }
    let tm = unsafe { *ptr };
    format!(
        "netboard-{:04}{:02}{:02}-{:02}{:02}{:02}.csv",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}
