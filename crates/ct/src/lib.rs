mod bans;
mod commands;
mod modifiers;
mod files;
pub mod voice;
mod voice_net;
pub mod voice_ffi;
mod migration;
mod overlay;
mod proto;
mod rules;
mod session;
mod state;
mod world;
#[cfg(test)]
mod tests;

use ct_net::SteamTransport;
use session::{HostOptions, Level, Note, NoteKind, Session};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

struct Global {
    session: Option<Session>,
    notes: VecDeque<Note>,
    steam: Option<Arc<SteamTransport>>,
    data_dir: Option<PathBuf>,
    history: VecDeque<(std::time::Instant, String, Level)>,
    actions: VecDeque<String>,
    defaults: std::collections::BTreeMap<String, String>,
    voice_tx: Option<voice::Tx>,
    music_tx: Option<voice::Tx>,
}

static G: Mutex<Global> = Mutex::new(Global {
    session: None,
    notes: VecDeque::new(),
    steam: None,
    data_dir: None,
    history: VecDeque::new(),
    actions: VecDeque::new(),
    defaults: std::collections::BTreeMap::new(),
    voice_tx: None,
    music_tx: None,
});
static RULES_REV: AtomicU32 = AtomicU32::new(0);
static PLAYERS_REV: AtomicU32 = AtomicU32::new(0);

fn g() -> MutexGuard<'static, Global> {
    G.lock().unwrap_or_else(|e| e.into_inner())
}

impl Global {
    fn remember(&mut self, text: &str, level: Level) {
        self.history.push_back((std::time::Instant::now(), text.to_owned(), level));
        while self.history.len() > 300 {
            self.history.pop_front();
        }
    }

    fn note(&mut self, level: Level, text: impl Into<String>) {
        let text = text.into();
        if level >= Level::Info {
            self.remember(&text, level);
        }
        self.notes.push_back(Note { kind: NoteKind::Log, level, text });
        if self.notes.len() > 2048 {
            self.notes.pop_front();
        }
    }

    fn end_session(&mut self, reason: &str) {
        if let Some(s) = self.session.take() {
            voice::with_mixer(|m| m.clear());
            s.shutdown(reason);
            PLAYERS_REV.fetch_add(1, Ordering::Relaxed);
        }
    }

    fn pump(&mut self) {
        let Some(s) = self.session.as_mut() else { return };
        let before = self.notes.len();
        s.drain_notes(&mut self.notes);
        let fresh: Vec<(String, Level, NoteKind)> = self.notes.iter().skip(before).map(|n| (n.text.clone(), n.level, n.kind)).collect();
        for (text, level, kind) in fresh {
            if kind == NoteKind::Chat || (kind == NoteKind::Log && level >= Level::Info) {
                self.remember(&text, if kind == NoteKind::Chat { Level::Debug } else { level });
            }
        }
        for n in self.notes.iter().skip(before) {
            match n.kind {
                NoteKind::Rules => RULES_REV.fetch_add(1, Ordering::Relaxed),
                NoteKind::Players => PLAYERS_REV.fetch_add(1, Ordering::Relaxed),
                _ => 0,
            };
        }
        while self.notes.len() > 2048 {
            self.notes.pop_front();
        }
    }
}

fn guard<T>(default: T, f: impl FnOnce() -> T) -> T {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(default)
}

unsafe fn text(ptr: *const u8, len: i32) -> String {
    if ptr.is_null() || len <= 0 {
        return String::new();
    }
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(ptr, len as usize) }).into_owned()
}

unsafe fn write_out(s: &[u8], buf: *mut u8, cap: i32) -> i32 {
    if buf.is_null() || cap <= 0 {
        return 0;
    }
    let n = s.len().min(cap as usize);
    unsafe { std::ptr::copy_nonoverlapping(s.as_ptr(), buf, n) };
    n as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_version() -> u32 {
    proto::VERSION as u32
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_set_data_dir(path: *const u8, len: i32) {
    let p = unsafe { text(path, len) };
    guard((), || g().data_dir = (!p.is_empty()).then(|| PathBuf::from(p)));
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_steam_init() -> i32 {
    guard(0, || {
        let mut g = g();
        if g.steam.is_some() {
            return 1;
        }
        match SteamTransport::attach() {
            Ok(s) => {
                let id = s.local_id();
                let name = s.persona_name();
                g.steam = Some(Arc::new(s));
                g.note(Level::Info, format!("Steam ready: {name} ({id})"));
                1
            }
            Err(e) => {
                g.note(Level::Warning, format!("Steam unavailable: {e}"));
                0
            }
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_steam_id() -> u64 {
    guard(0, || g().steam.as_ref().map_or(0, |s| s.local_id()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_steam_name(buf: *mut u8, cap: i32) -> i32 {
    guard(0, || {
        let name = g().steam.as_ref().map(|s| s.persona_name()).unwrap_or_default();
        unsafe { write_out(name.as_bytes(), buf, cap) }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_steam_accept(id: u64) -> i32 {
    guard(0, || g().steam.as_ref().is_some_and(|s| s.accept_session(id)) as i32)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ct_host(
    port: u16,
    use_steam: i32,
    name: *const u8,
    name_len: i32,
    password: *const u8,
    password_len: i32,
    admin_password: *const u8,
    admin_password_len: i32,
    max_players: i32,
) -> i32 {
    let (name, password, admin_password) =
        unsafe { (text(name, name_len), text(password, password_len), text(admin_password, admin_password_len)) };
    guard(-1, || {
        let mut g = g();
        g.end_session("host restarted");
        let steam = if use_steam != 0 { g.steam.clone() } else { None };
        let port = if port == 0 { proto::DEFAULT_PORT } else { port };
        let with_steam = steam.is_some();
        let opts = HostOptions {
            port,
            steam,
            name,
            password,
            admin_password,
            max_players: (max_players > 0).then(|| max_players.clamp(2, 64) as u8),
            data_dir: g.data_dir.clone(),
        };
        match Session::host(opts) {
            Ok(s) => {
                g.session = Some(s);
                g.note(Level::Info, format!("hosting on UDP {port}{}", if with_steam { " + Steam" } else { "" }));
                RULES_REV.fetch_add(1, Ordering::Relaxed);
                PLAYERS_REV.fetch_add(1, Ordering::Relaxed);
                0
            }
            Err(e) => {
                g.note(Level::Error, e);
                -1
            }
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_connect(
    addr: *const u8,
    addr_len: i32,
    name: *const u8,
    name_len: i32,
    password: *const u8,
    password_len: i32,
) -> i32 {
    let (addr, name, password) = unsafe { (text(addr, addr_len), text(name, name_len), text(password, password_len)) };
    guard(-1, || {
        let mut g = g();
        g.end_session("reconnecting");
        let target = match session::parse_addr(&addr) {
            Ok(a) => a,
            Err(e) => {
                g.note(Level::Error, e);
                return -1;
            }
        };
        let steam = g.steam.clone();
        match Session::connect(target, steam, &name, &password) {
            Ok(mut s) => {
                s.data_dir = g.data_dir.clone();
                g.session = Some(s);
                g.note(Level::Info, format!("connecting to {target}"));
                0
            }
            Err(e) => {
                g.note(Level::Error, e);
                -1
            }
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_disconnect() {
    guard((), || {
        let mut g = g();
        if let Some(s) = g.session.as_mut() {
            if s.is_host() {
                if let Some(to) = s.transfer_target() {
                    let _ = s.transfer_host(to);
                }
            }
            g.end_session("left");
            g.note(Level::Info, "session closed");
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_tick() -> i32 {
    guard(0, || {
        let mut g = g();
        let Some(s) = g.session.as_mut() else { return 0 };
        let alive = s.tick();
        let grants = s.take_grants();
        for (prefab, cond) in grants {
            g.actions.push_back(format!("grant\t{prefab}\t{cond}"));
        }
        g.pump();
        if !alive {
            g.session = None;
            PLAYERS_REV.fetch_add(1, Ordering::Relaxed);
        }
        alive as i32
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_chat(msg: *const u8, len: i32) {
    let msg = unsafe { text(msg, len) };
    guard((), || {
        let mut g = g();
        match g.session.as_mut() {
            Some(s) => s.chat(&msg),
            None => g.note(Level::Warning, "not in a session"),
        }
        g.pump();
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_command(line: *const u8, len: i32) {
    let line = unsafe { text(line, len) };
    guard((), || {
        let mut g = g();
        match g.session.as_mut() {
            Some(s) => s.command(&line),
            None => g.note(Level::Warning, "not in a session"),
        }
        g.pump();
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_next_note(buf: *mut u8, cap: i32) -> i32 {
    guard(0, || {
        if buf.is_null() || cap < 3 {
            return 0;
        }
        let Some(n) = g().notes.pop_front() else { return 0 };
        unsafe {
            *buf = n.kind as u8;
            *buf.add(1) = n.level as u8;
            2 + write_out(n.text.as_bytes(), buf.add(2), cap - 2)
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_status(buf: *mut u8, cap: i32) -> i32 {
    guard(0, || {
        let s = g().session.as_ref().map_or_else(|| "offline".to_owned(), |s| s.status());
        unsafe { write_out(s.as_bytes(), buf, cap) }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_players_rev() -> u32 {
    PLAYERS_REV.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_rules_rev() -> u32 {
    RULES_REV.load(Ordering::Relaxed)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_players(buf: *mut u8, cap: i32) -> i32 {
    guard(0, || {
        let g = g();
        let Some(s) = g.session.as_ref() else { return 0 };
        let mut out = Vec::with_capacity(256);
        let list: Vec<_> = s.players().collect();
        out.extend_from_slice(&(list.len() as u16).to_le_bytes());
        for p in list {
            let flags = (p.id == s.host_id()) as u8 | ((p.admin as u8) << 1) | (((p.id == s.local_id()) as u8) << 2);
            let name = &p.name.as_bytes()[..p.name.len().min(255)];
            out.extend_from_slice(&p.id.to_le_bytes());
            out.extend_from_slice(&p.ping_ms.to_le_bytes());
            out.push(flags);
            out.push(name.len() as u8);
            out.extend_from_slice(name);
        }
        unsafe { write_out(&out, buf, cap) }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_rule_index(name: *const u8, len: i32) -> i32 {
    let name = unsafe { text(name, len) };
    guard(-1, || rules::find(&name).map_or(-1, |i| i as i32))
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_rule_value(idx: i32) -> f32 {
    guard(f32::NAN, || {
        let i = idx as usize;
        if idx < 0 || i >= rules::SCHEMA.len() {
            return f32::NAN;
        }
        let g = g();
        g.session.as_ref().map_or(rules::SCHEMA[i].default, |s| s.rules().at(i)).as_f32()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_is_host() -> i32 {
    guard(0, || g().session.as_ref().is_some_and(|s| s.is_host()) as i32)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_set_local_state(state: *const state::PlayerState, limbs: *const state::LimbPose, limb_count: i32) {
    guard((), || {
        let snap = if state.is_null() {
            None
        } else {
            let mut s = state::Snapshot { state: unsafe { *state }, ..Default::default() };
            let n = (limb_count.max(0) as usize).min(state::MAX_LIMBS);
            if !limbs.is_null() && n > 0 {
                s.limbs[..n].copy_from_slice(unsafe { std::slice::from_raw_parts(limbs, n) });
            }
            s.state.limb_count = n as u32;
            Some(s)
        };
        if let Some(sess) = g().session.as_mut() {
            sess.set_local_state(snap);
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_remote_state(id: u16, out: *mut state::PlayerState, limbs: *mut state::LimbPose, cap: i32) -> i32 {
    guard(0, || {
        if out.is_null() {
            return 0;
        }
        let Some(s) = g().session.as_ref().and_then(|s| s.remote_state(id)) else { return 0 };
        unsafe { *out = s.state };
        let n = (s.state.limb_count as usize).min(cap.max(0) as usize);
        if !limbs.is_null() && n > 0 {
            unsafe { std::ptr::copy_nonoverlapping(s.limbs.as_ptr(), limbs, n) };
        }
        1
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_world_begin(params: *const u8, len: i32) -> u32 {
    let p = if params.is_null() || len <= 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(params, len as usize) }.to_vec() };
    guard(0, || {
        let mut g = g();
        let Some(s) = g.session.as_mut() else { return 0 };
        let generation = s.world_begin(p);
        g.pump();
        generation
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_world_params(buf: *mut u8, cap: i32, generation: *mut u32) -> i32 {
    guard(0, || {
        let g = g();
        let Some(s) = g.session.as_ref() else { return 0 };
        let (id, params) = s.world_params();
        if !generation.is_null() {
            unsafe { *generation = id };
        }
        if (cap as usize) < params.len() {
            return -(params.len() as i32);
        }
        unsafe { write_out(params, buf, cap) }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_world_rev() -> u32 {
    guard(0, || g().session.as_ref().map_or(0, |s| s.world_rev()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_world_ready(generation: u32) {
    guard((), || {
        if let Some(s) = g().session.as_mut() {
            s.world_ready(generation);
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_tile_edit(x: u16, y: u16, block: u16) {
    guard((), || {
        if let Some(s) = g().session.as_mut() {
            s.tile_edit(world::TileEdit { x, y, block });
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_tile_poll(out: *mut world::TileEdit, cap: i32) -> i32 {
    guard(0, || {
        if out.is_null() || cap <= 0 {
            return 0;
        }
        let slice = unsafe { std::slice::from_raw_parts_mut(out, cap as usize) };
        g().session.as_mut().map_or(0, |s| s.drain_tiles(slice) as i32)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_entity_call(entity: u32, method: u16, flags: u8, seed: u32, args: *const u8, len: i32) {
    let args = if args.is_null() || len <= 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(args, len as usize) }.to_vec() };
    guard((), || {
        if let Some(s) = g().session.as_mut() {
            s.entity_call(world::EntityCall { entity, method, flags, seed, args });
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_entity_poll(buf: *mut u8, cap: i32) -> i32 {
    guard(0, || {
        let mut g = g();
        let Some(s) = g.session.as_mut() else { return 0 };
        let Some(c) = s.next_entity_call() else { return 0 };
        let need = 4 + 2 + 1 + 4 + 2 + c.args.len();
        if buf.is_null() || (cap as usize) < need || c.args.len() > u16::MAX as usize {
            return 0;
        }
        let mut out = Vec::with_capacity(need);
        out.extend_from_slice(&c.entity.to_le_bytes());
        out.extend_from_slice(&c.method.to_le_bytes());
        out.push(c.flags);
        out.extend_from_slice(&c.seed.to_le_bytes());
        out.extend_from_slice(&(c.args.len() as u16).to_le_bytes());
        out.extend_from_slice(&c.args);
        unsafe { write_out(&out, buf, cap) }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_local_id() -> i32 {
    guard(-1, || g().session.as_ref().map_or(-1, |s| s.local_id() as i32))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_motion_send(payload: *const u8, len: i32) {
    if payload.is_null() || len <= 0 {
        return;
    }
    let p = unsafe { std::slice::from_raw_parts(payload, len as usize) };
    guard((), || {
        if let Some(s) = g().session.as_mut() {
            s.send_motion(p);
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_motion_poll(buf: *mut u8, cap: i32) -> i32 {
    guard(0, || {
        let mut g = g();
        let Some(s) = g.session.as_mut() else { return 0 };
        let Some(m) = s.next_motion() else { return 0 };
        unsafe { write_out(&m[5..], buf, cap) }
    })
}

pub(crate) struct UiSnapshot {
    pub online: bool,
    pub is_host: bool,
    pub is_admin: bool,
    pub local_id: u16,
    pub host_id: u16,
    pub status: String,
    pub players: Vec<proto::PlayerInfo>,
    pub rules: Option<rules::Rules>,
    pub history: Vec<(f32, String, bool)>,
}

pub(crate) fn ui_snapshot() -> UiSnapshot {
    let g = g();
    let now = std::time::Instant::now();
    let history = g.history.iter().map(|(t, s, l)| (now.duration_since(*t).as_secs_f32(), s.clone(), *l == Level::Debug)).collect();
    match g.session.as_ref() {
        Some(s) => {
            let local = s.local_id();
            let players: Vec<proto::PlayerInfo> = s.players().cloned().collect();
            let is_admin = s.is_host() || players.iter().any(|p| p.id == local && p.admin);
            UiSnapshot { online: true, is_host: s.is_host(), is_admin, local_id: local, host_id: s.host_id(), status: s.status(), players, rules: Some(s.rules().clone()), history }
        }
        None => UiSnapshot { online: false, is_host: false, is_admin: false, local_id: 0, host_id: 0, status: "offline".into(), players: Vec::new(), rules: None, history },
    }
}

pub(crate) fn ui_action(action: String) {
    g().actions.push_back(action);
}

pub(crate) fn ui_chat(text: &str) {
    let mut g = g();
    if let Some(s) = g.session.as_mut() {
        s.chat(text);
    }
    g.pump();
}

pub(crate) fn ui_command(line: &str) {
    let mut g = g();
    if let Some(s) = g.session.as_mut() {
        s.command(line);
    }
    g.pump();
}

pub(crate) fn ui_default(key: &str) -> Option<String> {
    g().defaults.get(key).cloned()
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_overlay_init() -> i32 {
    let dir = g().data_dir.clone();
    guard(-1, || match overlay::init(dir) {
        Ok(()) => {
            g().note(Level::Info, "overlay ready");
            0
        }
        Err(e) => {
            g().note(Level::Error, format!("overlay failed: {e}"));
            -1
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_ui_set_default(key: *const u8, key_len: i32, value: *const u8, value_len: i32) {
    let (k, v) = unsafe { (text(key, key_len), text(value, value_len)) };
    guard((), || {
        g().defaults.insert(k, v);
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_ui_next_action(buf: *mut u8, cap: i32) -> i32 {
    guard(0, || {
        let Some(a) = g().actions.pop_front() else { return 0 };
        unsafe { write_out(a.as_bytes(), buf, cap) }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_overlay_show(visible: i32) {
    overlay::VISIBLE.store(visible != 0, std::sync::atomic::Ordering::SeqCst);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_bind_name(idx: i32, buf: *mut u8, cap: i32) -> i32 {
    guard(-1, || {
        let Some((b, ..)) = overlay::binds::ALL.get(idx as usize) else { return -1 };
        let name = overlay::binds::name(overlay::binds::key(*b));
        unsafe { write_out(name.as_bytes(), buf, cap) }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_local_typing() -> i32 {
    use std::sync::atomic::Ordering::Relaxed;
    (overlay::CHAT_OPEN.load(Relaxed) || overlay::WANTS_KEYBOARD.load(Relaxed) || overlay::INPUT_BLOCKED.load(Relaxed)) as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_set_input_blocked(blocked: i32) {
    overlay::INPUT_BLOCKED.store(blocked != 0, std::sync::atomic::Ordering::Relaxed);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_needs_sync(h: *mut f32, t: *mut f32, e: *mut f32) -> i32 {
    guard(0, || {
        if h.is_null() || t.is_null() || e.is_null() {
            return 0;
        }
        let mut g = g();
        let Some(s) = g.session.as_mut() else { return 0 };
        let cur = unsafe { [*h, *t, *e] };
        match s.needs_sync(cur) {
            Some(p) => {
                unsafe {
                    *h = p[0];
                    *t = p[1];
                    *e = p[2];
                }
                1
            }
            None => 0,
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_stash_deposit(prefab: *const u8, len: i32, condition: f32) {
    let p = unsafe { text(prefab, len) };
    guard((), || {
        let mut g = g();
        if let Some(s) = g.session.as_mut() {
            s.stash_deposit(&p, condition);
        }
        g.pump();
    })
}

pub(crate) fn ui_stash() -> Vec<proto::StashItem> {
    g().session.as_ref().map(|s| s.stash().to_vec()).unwrap_or_default()
}

pub(crate) fn ui_stash_take(id: u32) {
    let mut g = g();
    if let Some(s) = g.session.as_mut() {
        s.stash_take(id);
    }
    g.pump();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_file_send(name: *const u8, name_len: i32, data: *const u8, len: i32) -> i32 {
    let n = unsafe { text(name, name_len) };
    let bytes = if data.is_null() || len <= 0 { Vec::new() } else { unsafe { std::slice::from_raw_parts(data, len as usize) }.to_vec() };
    guard(-1, || {
        let mut g = g();
        let Some(s) = g.session.as_mut() else { return -1 };
        let r = s.send_file(&n, bytes);
        if let Err(e) = r {
            g.note(Level::Warning, e);
            return -1;
        }
        g.note(Level::Info, format!("sending {n}..."));
        0
    })
}

pub(crate) struct UiFiles {
    pub progress: Vec<files::Progress>,
    pub received: Vec<PathBuf>,
}

pub(crate) fn ui_files() -> UiFiles {
    let g = g();
    match g.session.as_ref() {
        Some(s) => UiFiles { progress: s.file_progress(), received: s.received_files().to_vec() },
        None => UiFiles { progress: Vec::new(), received: Vec::new() },
    }
}
