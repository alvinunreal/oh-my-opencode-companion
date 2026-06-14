use std::collections::HashSet;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use eframe::egui;

use crate::gifs::Gifs;
use crate::screen::primary_size;
use crate::state::{read_state, start_watcher, SessionInfo};

const DEFAULT_SIZE: f32 = 120.0;
const GAP: f32 = 10.0;

const SIZE_PRESETS: &[(&str, f32)] = &[
    ("S  ·  80px", 80.0),
    ("M  ·  120px", 120.0),
    ("L  ·  160px", 160.0),
    ("XL  ·  200px", 200.0),
];

const SESSIONS_KEY: &str = "companion_sessions";
const SIZE_KEY: &str = "companion_size";
const MENU_OPEN_KEY: &str = "companion_menu_open";
const MENU_POS_KEY: &str = "companion_menu_pos";

// Per session: (id, cwd, agents, status)
type SessionSnapshot = Vec<(String, String, Vec<String>, String)>;

/// Grid columns for N agents.
fn grid_cols(n: usize) -> usize {
    match n {
        0 | 1 => 1,
        2 | 3 | 4 => 2,
        _ => 3,
    }
}

/// Returns (cols, rows) for N agents.
fn grid_dims(n: usize) -> (usize, usize) {
    let n = n.max(1);
    let cols = grid_cols(n);
    let rows = (n + cols - 1) / cols;
    (cols, rows)
}

/// Cell rects for each agent, with orphan cells centered in the last row.
fn cell_rects(agents: usize, cols: usize, rows: usize, cell: f32) -> Vec<egui::Rect> {
    let mut rects = Vec::with_capacity(agents);
    let full_rows = agents / cols;
    let remainder = agents % cols;

    for row in 0..full_rows {
        for col in 0..cols {
            rects.push(egui::Rect::from_min_size(
                egui::pos2(col as f32 * cell, row as f32 * cell),
                egui::vec2(cell, cell),
            ));
        }
    }

    if remainder > 0 {
        let x_offset = (cols - remainder) as f32 * cell / 2.0;
        for col in 0..remainder {
            rects.push(egui::Rect::from_min_size(
                egui::pos2(x_offset + col as f32 * cell, full_rows as f32 * cell),
                egui::vec2(cell, cell),
            ));
        }
    }

    let _ = rows; // used by caller for window sizing
    rects
}

fn clamp_viewport_pos(pos: egui::Pos2, win_w: f32, win_h: f32, screen: [f32; 2]) -> egui::Pos2 {
    let x_max = (screen[0] - win_w - GAP).max(GAP);
    let y_max = (screen[1] - win_h - GAP).max(GAP);
    egui::pos2(pos.x.clamp(GAP, x_max), pos.y.clamp(GAP, y_max))
}

pub struct CompanionApp {
    state_path: std::path::PathBuf,
    sessions: Vec<SessionInfo>,
    gifs: Gifs,
    rx: Receiver<()>,
    registered: bool,
    positioned: HashSet<String>,
    size: f32,
    screen: [f32; 2],
    position: String,
    has_modern_config: bool,
}

impl CompanionApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let state_path = crate::state::state_file_path();
        let state = read_state(&state_path);
        let sessions = state.sessions;

        let mut initial_size = DEFAULT_SIZE;
        let mut position = "bottom-right".to_string();
        let has_modern_config = state.config.is_some();
        if let Some(ref cfg) = state.config {
            initial_size = match cfg.size.as_str() {
                "small" => 80.0,
                "medium" => 120.0,
                "large" => 160.0,
                _ => 120.0,
            };
            position = cfg.position.clone();
        }

        let rx = start_watcher(state_path.clone());

        Self {
            state_path,
            sessions,
            gifs: Gifs::new(),
            rx,
            registered: false,
            positioned: HashSet::new(),
            size: initial_size,
            screen: primary_size(),
            position,
            has_modern_config,
        }
    }

    fn poll(&mut self) {
        if self.rx.try_recv().is_ok() {
            while self.rx.try_recv().is_ok() {}
            let state = read_state(&self.state_path);
            self.sessions = state.sessions;
            self.has_modern_config = state.config.is_some();
            if let Some(ref cfg) = state.config {
                self.position = cfg.position.clone();
            } else {
                self.position = "bottom-right".to_string();
            }
        }

        // Liveness check runs every tick, not just on file changes: when an
        // opencode process is killed it never rewrites the state file, so
        // waiting on the watcher would leave its window open forever.
        let has_modern = self.has_modern_config;
        self.sessions
            .retain(|s| s.pid.map(is_pid_alive).unwrap_or(!has_modern));
        let live: HashSet<_> = self.sessions.iter().map(|s| s.session_id.clone()).collect();
        self.positioned.retain(|id| live.contains(id));
    }

    fn initial_pos(&self, index: usize, win_w: f32, win_h: f32) -> [f32; 2] {
        let slot = index as f32;
        let (x, y) = match self.position.as_str() {
            "bottom-left" => {
                let x = GAP + (win_w + GAP) * slot;
                let y = self.screen[1] - win_h - GAP;
                (x, y)
            }
            "top-right" => {
                let x = self.screen[0] - (win_w + GAP) * (slot + 1.0);
                let y = GAP;
                (x, y)
            }
            "top-left" => {
                let x = GAP + (win_w + GAP) * slot;
                let y = GAP;
                (x, y)
            }
            _ => {
                // "bottom-right"
                let x = self.screen[0] - (win_w + GAP) * (slot + 1.0);
                let y = self.screen[1] - win_h - GAP;
                (x, y)
            }
        };
        let x_max = (self.screen[0] - win_w - GAP).max(GAP);
        let y_max = (self.screen[1] - win_h - GAP).max(GAP);
        [x.clamp(GAP, x_max), y.clamp(GAP, y_max)]
    }
}

impl eframe::App for CompanionApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();

        let quit = ctx.data(|d| {
            d.get_temp::<bool>(egui::Id::new("companion_quit"))
                .unwrap_or(false)
        });
        if quit || (self.registered && self.sessions.is_empty()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if !self.registered {
            self.gifs.register(ctx);
            ctx.data_mut(|d| d.insert_temp(egui::Id::new(SIZE_KEY), self.size));
            self.registered = true;
        }

        self.size = ctx.data(|d| d.get_temp(egui::Id::new(SIZE_KEY)).unwrap_or(DEFAULT_SIZE));

        // Store registered GIF URIs (with unknown agents falling back to the
        // orchestrator GIF) so cells never point at an unregistered image.
        let snapshot: SessionSnapshot = self
            .sessions
            .iter()
            .map(|s| {
                let uris: Vec<String> = if s.active_agents.is_empty() {
                    vec![self.gifs.uri("intro")]
                } else {
                    s.active_agents.iter().map(|a| self.gifs.uri(a)).collect()
                };
                (s.session_id.clone(), s.cwd.clone(), uris, s.status.clone())
            })
            .collect();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(SESSIONS_KEY), snapshot));

        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |_| {});

        for (i, session) in self.sessions.iter().enumerate() {
            let vid = egui::ViewportId::from_hash_of(&session.session_id);
            let sid = session.session_id.clone();
            let size = self.size;
            let screen = self.screen;

            let agents: Vec<String> = if session.active_agents.is_empty() {
                vec!["intro".to_string()]
            } else {
                session.active_agents.clone()
            };
            let n = agents.len().max(1);
            let (cols, rows) = grid_dims(n);
            let win_w = size * cols as f32;
            let win_h = size * rows as f32;

            let is_first = !self.positioned.contains(&session.session_id);
            if is_first {
                self.positioned.insert(session.session_id.clone());
            }

            let mut builder = egui::ViewportBuilder::default()
                .with_title(&session.project_name())
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_active(false)
                .with_inner_size([win_w, win_h]);

            if is_first {
                builder = builder.with_position(self.initial_pos(i, win_w, win_h));
            }

            ctx.show_viewport_deferred(vid, builder, move |ctx, _class| {
                render_session_window(ctx, &sid, size, screen);
            });
        }

        // Size-picker popup
        let menu_open: bool =
            ctx.data(|d| d.get_temp(egui::Id::new(MENU_OPEN_KEY)).unwrap_or(false));
        if menu_open {
            let pos: [f32; 2] = ctx.data(|d| {
                d.get_temp(egui::Id::new(MENU_POS_KEY))
                    .unwrap_or([200.0, 200.0])
            });

            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("size_picker"),
                egui::ViewportBuilder::default()
                    .with_title("size")
                    .with_decorations(false)
                    .with_always_on_top()
                    .with_inner_size([160.0, 190.0])
                    .with_position(pos),
                |ctx, _| render_size_picker(ctx),
            );
        }

        ctx.request_repaint_after(Duration::from_millis(150));
    }
}

fn render_session_window(ctx: &egui::Context, session_id: &str, _size: f32, screen: [f32; 2]) {
    let sessions: SessionSnapshot =
        ctx.data(|d| d.get_temp(egui::Id::new(SESSIONS_KEY)).unwrap_or_default());
    let (_, cwd, agents, _) = sessions
        .iter()
        .find(|(id, _, _, _)| id == session_id)
        .cloned()
        .unwrap_or_default();

    // Snapshot holds ready-to-use GIF URIs.
    let uris: Vec<String> = if agents.is_empty() {
        vec!["bytes://intro.gif".to_string()]
    } else {
        agents
    };

    let project = std::path::Path::new(&cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let current_size: f32 =
        ctx.data(|d| d.get_temp(egui::Id::new(SIZE_KEY)).unwrap_or(DEFAULT_SIZE));

    let n = uris.len().max(1);
    let (cols, rows) = grid_dims(n);
    let win_w = current_size * cols as f32;
    let win_h = current_size * rows as f32;

    // Resize viewport when size or agent count changes.
    let layout_key = egui::Id::new(session_id).with("layout");
    let applied: (u32, u32, u32) = ctx.data(|d| d.get_temp(layout_key).unwrap_or((0, 0, 0)));
    let current = (current_size as u32, cols as u32, rows as u32);
    if applied != current {
        let old_outer_rect = ctx.input(|i| i.viewport().outer_rect);
        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(win_w, win_h)));
        if let Some(rect) = old_outer_rect {
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(clamp_viewport_pos(
                rect.min, win_w, win_h, screen,
            )));
        }
        ctx.data_mut(|d| d.insert_temp(layout_key, current));
    }

    // Right-click → open size picker
    if ctx.input(|i| i.pointer.secondary_released()) {
        let win_origin = ctx
            .input(|i| i.viewport().outer_rect.map(|r| r.min))
            .unwrap_or_default();
        let cursor = ctx.input(|i| i.pointer.interact_pos()).unwrap_or_default();
        ctx.data_mut(|d| {
            d.insert_temp(
                egui::Id::new(MENU_POS_KEY),
                [win_origin.x + cursor.x, win_origin.y + cursor.y],
            );
            d.insert_temp(egui::Id::new(MENU_OPEN_KEY), true);
        });
    }

    // Drag
    if ctx.input(|i| i.pointer.primary_down()) {
        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::TRANSPARENT)
                .inner_margin(egui::Margin::ZERO),
        )
        .show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let rects = cell_rects(n, cols, rows, current_size);

            for (i, uri) in uris.iter().enumerate() {
                if let Some(&cell) = rects.get(i) {
                    ui.put(
                        cell,
                        egui::Image::new(uri)
                            .fit_to_exact_size(egui::vec2(current_size, current_size)),
                    );
                }
            }

            // Project label — overlaid on bottom strip of the window
            let label_h = (current_size * 0.15).clamp(13.0, 30.0);
            let font_size = (current_size * 0.09).clamp(9.0, 13.0);
            let full_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(win_w, win_h));
            let strip = egui::Rect::from_min_size(
                egui::pos2(0.0, win_h - label_h),
                egui::vec2(win_w, label_h),
            );
            ui.painter()
                .rect_filled(strip, 0.0, egui::Color32::from_black_alpha(185));

            let fid = egui::FontId::proportional(font_size);
            let label = fit_text(ctx, &project, &fid, win_w - 10.0);
            ui.painter().text(
                egui::pos2(full_rect.center().x, strip.center().y),
                egui::Align2::CENTER_CENTER,
                &label,
                fid,
                egui::Color32::WHITE,
            );
        });

    ctx.request_repaint_after(Duration::from_millis(50));
}

fn render_size_picker(ctx: &egui::Context) {
    let size: f32 = ctx.data(|d| d.get_temp(egui::Id::new(SIZE_KEY)).unwrap_or(DEFAULT_SIZE));
    let frames_key = egui::Id::new("menu_frames");
    let frames: u32 = ctx.data(|d| d.get_temp(frames_key).unwrap_or(0));
    ctx.data_mut(|d| d.insert_temp(frames_key, frames + 1));

    let close = ctx.input(|i| i.key_pressed(egui::Key::Escape))
        || (frames > 1 && !ctx.input(|i| i.focused));

    if close {
        ctx.data_mut(|d| {
            d.insert_temp(egui::Id::new(MENU_OPEN_KEY), false);
            d.insert_temp(frames_key, 0u32);
        });
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        return;
    }

    egui::CentralPanel::default()
        .frame(
            egui::Frame::none()
                .fill(egui::Color32::from_rgb(28, 28, 28))
                .inner_margin(egui::Margin::same(8.0)),
        )
        .show(ctx, |ui| {
            ui.label(
                egui::RichText::new("Window size")
                    .size(11.0)
                    .color(egui::Color32::from_rgb(160, 160, 160)),
            );
            ui.add_space(4.0);

            for (label, preset) in SIZE_PRESETS {
                let active = (size - preset).abs() < 0.5;
                let text = if active {
                    egui::RichText::new(*label)
                        .size(12.0)
                        .strong()
                        .color(egui::Color32::WHITE)
                } else {
                    egui::RichText::new(*label)
                        .size(12.0)
                        .color(egui::Color32::from_rgb(200, 200, 200))
                };
                if ui
                    .add_sized([144.0, 20.0], egui::Button::new(text).frame(false))
                    .clicked()
                {
                    ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::new(SIZE_KEY), *preset);
                        d.insert_temp(egui::Id::new(MENU_OPEN_KEY), false);
                        d.insert_temp(frames_key, 0u32);
                    });
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            if ui
                .add_sized(
                    [144.0, 20.0],
                    egui::Button::new(
                        egui::RichText::new("Close companion")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(220, 100, 100)),
                    )
                    .frame(false),
                )
                .clicked()
            {
                ctx.data_mut(|d| {
                    d.insert_temp(egui::Id::new(MENU_OPEN_KEY), false);
                    d.insert_temp(frames_key, 0u32);
                    d.insert_temp(egui::Id::new("companion_quit"), true);
                });
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
}

fn fit_text(ctx: &egui::Context, text: &str, font_id: &egui::FontId, max_width: f32) -> String {
    let measure = |s: &str| -> f32 {
        ctx.fonts(|f| f.layout_no_wrap(s.to_string(), font_id.clone(), egui::Color32::WHITE))
            .rect
            .width()
    };
    if measure(text) <= max_width {
        return text.to_string();
    }
    let ellipsis = "…";
    let budget = (max_width - measure(ellipsis)).max(0.0);
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut lo = 0usize;
    let mut hi = chars.len();
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        let end = chars[mid - 1].0 + chars[mid - 1].1.len_utf8();
        if measure(&text[..end]) <= budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if lo == 0 {
        return ellipsis.to_string();
    }
    let end = chars[lo - 1].0 + chars[lo - 1].1.len_utf8();
    format!("{}{ellipsis}", &text[..end])
}

#[cfg(unix)]
fn is_pid_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

#[cfg(not(unix))]
fn is_pid_alive(_pid: u32) -> bool {
    true
}
