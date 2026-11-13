use super::binds::{self, ALL};
use super::{CHAT_OPEN, STASH_OPEN, VISIBLE};
use crate::rules::{SCHEMA, Value};
use std::collections::HashMap;
use std::sync::atomic::Ordering;

pub struct UiState {
    loaded: bool,
    name: String,
    port: String,
    password: String,
    admin_password: String,
    max_players: String,
    steam: bool,
    address: String,
    join_password: String,
    chat: String,
    chat_was_open: bool,
    rule_filter: String,
    rule_edit: HashMap<usize, f32>,
    join_tab: bool,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            loaded: false,
            name: String::new(),
            port: "7790".into(),
            password: String::new(),
            admin_password: String::new(),
            max_players: "0".into(),
            steam: true,
            address: String::new(),
            join_password: String::new(),
            chat: String::new(),
            chat_was_open: false,
            rule_filter: String::new(),
            rule_edit: HashMap::new(),
            join_tab: false,
        }
    }

    fn load_defaults(&mut self) {
        let get = |k: &str| crate::ui_default(k);
        if let Some(v) = get("name") {
            self.name = v;
        }
        if let Some(v) = get("port") {
            self.port = v;
        }
        if let Some(v) = get("password") {
            self.password = v.clone();
            self.join_password = v;
        }
        if let Some(v) = get("admin_password") {
            self.admin_password = v;
        }
        if let Some(v) = get("max_players") {
            self.max_players = v;
        }
        if let Some(v) = get("steam") {
            self.steam = v != "false";
        }
        if let Some(v) = get("address") {
            self.address = v;
        }
        self.loaded = crate::ui_default("name").is_some();
    }
}

pub fn draw(ctx: &egui::Context, st: &mut UiState) {
    if !st.loaded {
        st.load_defaults();
    }
    let snap = crate::ui_snapshot();
    chat(ctx, st, &snap);
    if VISIBLE.load(Ordering::Relaxed) {
        main_window(ctx, st, &snap);
    }
    if STASH_OPEN.load(Ordering::Relaxed) {
        stash_window(ctx, &snap);
    }
}

fn chat(ctx: &egui::Context, st: &mut UiState, snap: &crate::UiSnapshot) {
    let open = CHAT_OPEN.load(Ordering::Relaxed);
    let just_opened = open && !st.chat_was_open;
    st.chat_was_open = open;
    let line = |ui: &mut egui::Ui, text: &str, chat: bool, alpha: f32| {
        let color = if chat { egui::Color32::from_rgb(220, 230, 255) } else { egui::Color32::from_rgb(170, 170, 170) };
        ui.label(egui::RichText::new(text).color(color.gamma_multiply(alpha)));
    };
    if !open {
        let recent: Vec<_> = snap.history.iter().filter(|(age, _, _)| *age < 10.0).collect();
        if recent.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::new("ct_chat_feed"))
            .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -240.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_width(460.0);
                for (age, text, is_chat) in recent.iter().rev().take(6).rev() {
                    line(ui, text, *is_chat, ((10.0 - age) / 2.0).clamp(0.0, 1.0));
                }
            });
        return;
    }
    egui::Window::new("Chat")
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -240.0))
        .default_width(460.0)
        .resizable(true)
        .collapsible(false)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(220.0).stick_to_bottom(true).show(ui, |ui| {
                for (_, text, is_chat) in &snap.history {
                    line(ui, text, *is_chat, 1.0);
                }
            });
            let resp = ui.add(egui::TextEdit::singleline(&mut st.chat).desired_width(f32::INFINITY).hint_text("message or /command"));
            if just_opened {
                resp.request_focus();
            }
            if resp.lost_focus() {
                let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                let text = st.chat.trim().to_owned();
                if enter && !text.is_empty() {
                    crate::ui_chat(&text);
                }
                st.chat.clear();
                CHAT_OPEN.store(false, Ordering::SeqCst);
            }
            if !snap.online {
                ui.small("not in a session");
            }
        });
}

fn main_window(ctx: &egui::Context, st: &mut UiState, snap: &crate::UiSnapshot) {
    let mut open = true;
    egui::Window::new("Casualties: Together")
        .open(&mut open)
        .default_pos(egui::pos2(40.0, 110.0))
        .default_width(520.0)
        .show(ctx, |ui| {
            ui.label(egui::RichText::new(snap.status.lines().next().unwrap_or("")).strong());
            ui.separator();
            if snap.online {
                session_panel(ui, st, snap);
                ui.separator();
                rules_panel(ui, st, snap);
                saves_panel(ui);
            } else {
                connect_panel(ui, st);
            }
            ui.separator();
            voice_panel(ui);
            keybinds_panel(ui);
            ui.small(format!(
                "{} menu, {} chat, {} talk, {} CPR, {} stash, Esc close",
                binds::name(binds::key(binds::Bind::Menu)),
                binds::name(binds::key(binds::Bind::Chat)),
                binds::name(binds::key(binds::Bind::Ptt)),
                binds::name(binds::key(binds::Bind::Cpr)),
                binds::name(binds::key(binds::Bind::Stash))
            ));
        });
    if !open {
        VISIBLE.store(false, Ordering::SeqCst);
    }
}

fn connect_panel(ui: &mut egui::Ui, st: &mut UiState) {
    ui.horizontal(|ui| {
        ui.label("Nickname");
        if ui.text_edit_singleline(&mut st.name).lost_focus() {
            crate::ui_action(format!("name\t{}", st.name));
        }
    });
    ui.horizontal(|ui| {
        ui.selectable_value(&mut st.join_tab, false, "Host");
        ui.selectable_value(&mut st.join_tab, true, "Join");
    });
    egui::Grid::new("ct_connect").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
        if st.join_tab {
            ui.label("Address");
            ui.add(egui::TextEdit::singleline(&mut st.address).hint_text("ip, ip:port or steam:ID"));
            ui.end_row();
            ui.label("Password");
            ui.add(egui::TextEdit::singleline(&mut st.join_password).password(true));
            ui.end_row();
        } else {
            ui.label("Port");
            ui.text_edit_singleline(&mut st.port);
            ui.end_row();
            ui.label("Password");
            ui.add(egui::TextEdit::singleline(&mut st.password).password(true));
            ui.end_row();
            ui.label("Admin password");
            ui.add(egui::TextEdit::singleline(&mut st.admin_password).password(true));
            ui.end_row();
            ui.label("Max players");
            ui.text_edit_singleline(&mut st.max_players);
            ui.end_row();
            ui.label("Steam");
            ui.checkbox(&mut st.steam, "allow Steam friends to join");
            ui.end_row();
        }
    });
    if st.join_tab {
        if ui.add_enabled(!st.address.trim().is_empty(), egui::Button::new("Connect")).clicked() {
            crate::ui_action(format!("connect\t{}\t{}\t{}", st.address.trim(), st.join_password, st.name));
        }
    } else if ui.button("Host").clicked() {
        crate::ui_action(format!("host\t{}\t{}\t{}\t{}\t{}\t{}", st.port.trim(), st.password, st.admin_password, st.max_players.trim(), st.steam, st.name));
    }
}

fn session_panel(ui: &mut egui::Ui, st: &mut UiState, snap: &crate::UiSnapshot) {
    ui.horizontal(|ui| {
        ui.label("Nickname");
        let edit = ui.add(egui::TextEdit::singleline(&mut st.name).desired_width(160.0).char_limit(24));
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if (ui.button("rename").clicked() || enter) && !st.name.trim().is_empty() {
            crate::ui_action(format!("name\t{}", st.name));
            crate::ui_command(&format!("name {}", st.name));
        }
    });
    ui.horizontal(|ui| {
        if ui.button(if snap.is_host { "Stop hosting" } else { "Disconnect" }).clicked() {
            crate::ui_action("disconnect".into());
        }
        for line in snap.status.lines().filter(|l| l.contains("traffic") || l.contains("world #")) {
            ui.small(line.trim());
        }
    });
    egui::Grid::new("ct_players").num_columns(4).striped(true).spacing([14.0, 4.0]).show(ui, |ui| {
        ui.strong("Player");
        ui.strong("Ping");
        ui.strong("Role");
        ui.strong("");
        ui.end_row();
        for p in &snap.players {
            let me = p.id == snap.local_id;
            let talking = crate::voice::with_mixer(|m| m.level(p.id, crate::voice::STREAM_VOICE)) > 0.02 || (me && crate::voice_ffi::ct_voice_mic_level() > 0.0 && crate::voice_ffi::transmitting());
            let label = if me { format!("{} (you)", p.name) } else { p.name.clone() };
            if talking {
                ui.colored_label(egui::Color32::from_rgb(120, 220, 120), label);
            } else {
                ui.label(label);
            }
            ui.label(format!("{} ms", p.ping_ms));
            ui.label(if p.id == snap.host_id { "host" } else if p.admin { "admin" } else { "" });
            ui.horizontal(|ui| {
                if !me {
                    let muted = crate::voice::with_mixer(|m| m.muted.contains(&p.id));
                    if ui.small_button(if muted { "unmute" } else { "mute" }).clicked() {
                        crate::voice_ffi::ct_voice_toggle_mute(p.id);
                    }
                }
                if snap.is_admin && !me && p.id != snap.host_id {
                    if ui.small_button("kick").clicked() {
                        crate::ui_command(&format!("kick id:{}", p.id));
                    }
                    if ui.small_button("ban").clicked() {
                        crate::ui_command(&format!("ban id:{}", p.id));
                    }
                    if snap.is_host && ui.small_button(if p.admin { "deadmin" } else { "admin" }).clicked() {
                        crate::ui_command(&format!("{} id:{}", if p.admin { "deadmin" } else { "admin" }, p.id));
                    }
                    if snap.is_host && ui.small_button("give host").clicked() {
                        crate::ui_command(&format!("givehost id:{}", p.id));
                    }
                }
            });
            ui.end_row();
        }
    });
}

fn rules_panel(ui: &mut egui::Ui, st: &mut UiState, snap: &crate::UiSnapshot) {
    let Some(rules) = &snap.rules else { return };
    egui::CollapsingHeader::new("Rules").default_open(false).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.text_edit_singleline(&mut st.rule_filter);
        });
        if !snap.is_admin {
            ui.small("only the host and admins can change rules");
        }
        let filter = st.rule_filter.to_ascii_lowercase();
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            egui::Grid::new("ct_rules").num_columns(2).striped(true).show(ui, |ui| {
                for (i, r) in SCHEMA.iter().enumerate() {
                    if !filter.is_empty() && !r.name.to_ascii_lowercase().contains(&filter) {
                        continue;
                    }
                    let changed = rules.at(i) != r.default;
                    ui.label(if changed { egui::RichText::new(r.name).strong() } else { egui::RichText::new(r.name) });
                    ui.add_enabled_ui(snap.is_admin, |ui| match rules.at(i) {
                        Value::Bool(b) => {
                            let mut v = b;
                            if ui.checkbox(&mut v, "").changed() {
                                crate::ui_command(&format!("rule {} {}", r.name, v as u8));
                            }
                        }
                        other => {
                            let mut v = *st.rule_edit.get(&i).unwrap_or(&other.as_f32());
                            let speed = ((r.max - r.min) / 200.0).max(0.01);
                            let integer = matches!(other, Value::U8(_) | Value::U16(_));
                            let mut drag = egui::DragValue::new(&mut v).range(r.min..=r.max).speed(if integer { speed.max(0.2) } else { speed }).suffix(r.unit);
                            if integer {
                                drag = drag.fixed_decimals(0);
                            }
                            let resp = ui.add(drag);
                            if resp.dragged() || resp.has_focus() {
                                st.rule_edit.insert(i, v);
                            }
                            if resp.drag_stopped() || resp.lost_focus() {
                                st.rule_edit.remove(&i);
                                if (v - other.as_f32()).abs() > f32::EPSILON {
                                    crate::ui_command(&format!("rule {} {}", r.name, v));
                                }
                            }
                        }
                    });
                    ui.end_row();
                }
            });
        });
    });
}

fn keybinds_panel(ui: &mut egui::Ui) {
    egui::CollapsingHeader::new("Keybinds").default_open(false).show(ui, |ui| {
        let capturing = binds::capturing();
        egui::Grid::new("ct_binds").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
            for (b, _, label, _) in ALL {
                ui.label(label);
                let text = if capturing == Some(b) { "press a key...".to_owned() } else { binds::name(binds::key(b)) };
                if ui.button(text).clicked() {
                    binds::capture(Some(b));
                }
                ui.end_row();
            }
        });
        if ui.small_button("reset to defaults").clicked() {
            binds::reset();
        }
    });
}

fn stash_window(ctx: &egui::Context, snap: &crate::UiSnapshot) {
    let mut open = true;
    egui::Window::new("Shared stash")
        .open(&mut open)
        .default_pos(egui::pos2(600.0, 110.0))
        .default_width(360.0)
        .show(ctx, |ui| {
            let enabled = snap.rules.as_ref().is_some_and(|r| r.flag(crate::rules::Id::SharedStash));
            if !snap.online || !enabled {
                ui.label("The shared stash is off. The host can enable the SharedStash rule.");
                return;
            }
            if ui.button("Put the item in my hand here").clicked() {
                crate::ui_action("deposit".into());
            }
            ui.separator();
            let items = crate::ui_stash();
            if items.is_empty() {
                ui.label("empty");
            }
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                egui::Grid::new("ct_stash").num_columns(3).striped(true).show(ui, |ui| {
                    for it in &items {
                        ui.label(&it.prefab);
                        ui.label(format!("{:.0}%  by {}", it.condition * 100.0, it.by));
                        if ui.small_button("take").clicked() {
                            crate::ui_stash_take(it.id);
                        }
                        ui.end_row();
                    }
                });
            });
        });
    if !open {
        STASH_OPEN.store(false, Ordering::SeqCst);
    }
}

fn voice_panel(ui: &mut egui::Ui) {
    egui::CollapsingHeader::new("Voice").default_open(false).show(ui, |ui| {
        let mut s = crate::voice::settings();
        let before = (s.mode, s.mic.clone(), s.gain, s.threshold, s.volume, s.music_volume);
        ui.horizontal(|ui| {
            ui.label("Mode");
            ui.selectable_value(&mut s.mode, 0, "off");
            ui.selectable_value(&mut s.mode, 1, format!("push to talk ({})", binds::name(binds::key(binds::Bind::Ptt))));
            ui.selectable_value(&mut s.mode, 2, "open mic");
        });
        let devices = crate::voice::DEVICES.lock().map(|d| d.clone()).unwrap_or_default();
        let shown = if s.mic.is_empty() { "default".to_owned() } else { s.mic.clone() };
        egui::ComboBox::from_label("Microphone").selected_text(shown).width(260.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut s.mic, String::new(), "default");
            for d in &devices {
                ui.selectable_value(&mut s.mic, d.clone(), d);
            }
        });
        let level = crate::voice_ffi::ct_voice_mic_level();
        let talking = crate::voice_ffi::transmitting();
        ui.add(egui::ProgressBar::new(level.clamp(0.0, 1.0)).desired_width(260.0).text(if talking { "transmitting" } else { "mic level" }));
        ui.add(egui::Slider::new(&mut s.gain, 0.0..=4.0).text("mic gain"));
        if s.mode == 2 {
            ui.add(egui::Slider::new(&mut s.threshold, 0.0..=0.5).text("open mic threshold"));
        }
        ui.add(egui::Slider::new(&mut s.volume, 0.0..=2.0).text("voice volume"));
        ui.add(egui::Slider::new(&mut s.music_volume, 0.0..=2.0).text("MP3 volume"));
        if before != (s.mode, s.mic.clone(), s.gain, s.threshold, s.volume, s.music_volume) {
            crate::voice::update_settings(|cur| *cur = s);
        }
    });
}

fn saves_panel(ui: &mut egui::Ui) {
    egui::CollapsingHeader::new("Saves").default_open(false).show(ui, |ui| {
        if ui.button("Send my save to everyone").clicked() {
            crate::ui_action("send_save".into());
        }
        let files = crate::ui_files();
        for p in &files.progress {
            let frac = if p.total > 0 { p.done as f32 / p.total as f32 } else { 1.0 };
            ui.add(egui::ProgressBar::new(frac).text(format!("{} {} {}/{} KB", if p.sending { "sending" } else { "receiving" }, p.name, p.done / 1024, p.total / 1024)));
        }
        for path in &files.received {
            ui.horizontal(|ui| {
                ui.label(path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
                if ui.small_button("use as my save").clicked() {
                    crate::ui_action(format!("use_save\t{}", path.display()));
                }
            });
        }
    });
}
