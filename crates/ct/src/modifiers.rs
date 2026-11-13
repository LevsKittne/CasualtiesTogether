use crate::proto::{Msg, StashItem};
use crate::rules::Id;
use crate::session::{Level, NoteKind, Session};
use ct_net::channel;
use std::time::{Duration, Instant};

const POOL_INTERVAL: Duration = Duration::from_millis(500);
const POOL_MAX: f32 = 200.0;

#[derive(Default)]
pub struct Modifiers {
    pub pool: Option<[f32; 3]>,
    last_applied: Option<[f32; 3]>,
    last_broadcast: Option<Instant>,
    pub stash: Vec<StashItem>,
    next_stash: u32,
    pub grants: Vec<(String, f32)>,
    loaded: bool,
    loaded_file: bool,
}

impl Modifiers {
    pub(crate) fn mark_loaded(&mut self) {
        self.loaded = true;
        self.loaded_file = true;
        self.next_stash = self.stash.iter().map(|s| s.id).max().unwrap_or(0).max(self.next_stash);
    }
}

impl Session {
    fn combine(&mut self, delta: [f32; 3]) {
        let n = self.players.len().max(1) as f32;
        let pool = self.mods.pool.get_or_insert([100.0; 3]);
        for i in 0..3 {
            let d = delta[i];
            if !d.is_finite() {
                continue;
            }
            pool[i] = (pool[i] + if d > 0.0 { d } else { d / n }).clamp(0.0, POOL_MAX);
        }
    }

    pub fn needs_sync(&mut self, current: [f32; 3]) -> Option<[f32; 3]> {
        if !self.rules.flag(Id::SharedStats) {
            self.mods.pool = None;
            self.mods.last_applied = None;
            return None;
        }
        let Some(last) = self.mods.last_applied else {
            if self.is_host() && self.mods.pool.is_none() {
                self.mods.pool = Some(current);
            }
            self.mods.last_applied = Some(self.mods.pool.unwrap_or(current));
            return self.mods.pool;
        };
        let delta = [current[0] - last[0], current[1] - last[1], current[2] - last[2]];
        if self.is_host() {
            self.combine(delta);
        } else if let Some(peer) = self.host_peer() {
            let _ = self.ep.send(peer, channel::CONTROL, &Msg::NeedsDelta(delta).encode());
            if let Some(p) = self.mods.pool.as_mut() {
                for i in 0..3 {
                    if delta[i] > 0.0 {
                        p[i] = (p[i] + delta[i]).min(POOL_MAX);
                    }
                }
            }
        }
        let out = self.mods.pool.unwrap_or(current);
        self.mods.last_applied = Some(out);
        Some(out)
    }

    pub(crate) fn on_needs_delta(&mut self, delta: [f32; 3]) {
        if self.rules.flag(Id::SharedStats) {
            self.combine(delta);
        }
    }

    pub(crate) fn on_needs_pool(&mut self, pool: [f32; 3]) {
        self.mods.pool = Some(pool);
    }

    pub(crate) fn tick_modifiers(&mut self) {
        if !self.is_host() {
            return;
        }
        if !self.mods.loaded {
            self.mods.loaded = true;
            self.load_stash();
        }
        if !self.rules.flag(Id::SharedStats) {
            return;
        }
        let Some(pool) = self.mods.pool else { return };
        if self.mods.last_broadcast.is_some_and(|t| t.elapsed() < POOL_INTERVAL) {
            return;
        }
        self.mods.last_broadcast = Some(Instant::now());
        self.ep.broadcast(channel::CONTROL, &Msg::NeedsPool(pool).encode(), None);
    }

    fn load_stash(&mut self) {
        if self.mods.loaded_file {
            return;
        }
        self.mods.loaded_file = true;
        let Some(path) = self.data_path("mp_stash.txt") else { return };
        let Ok(text) = std::fs::read_to_string(path) else { return };
        for line in text.lines() {
            let mut it = line.split('\t');
            let (Some(prefab), Some(cond)) = (it.next(), it.next()) else { continue };
            let Ok(condition) = cond.parse::<f32>() else { continue };
            self.mods.next_stash += 1;
            self.mods.stash.push(StashItem { id: self.mods.next_stash, prefab: prefab.to_owned(), condition, by: it.next().unwrap_or("").to_owned() });
        }
    }

    fn save_stash(&self) {
        if let Some(path) = self.data_path("mp_stash.txt") {
            let text: String = self.mods.stash.iter().map(|s| format!("{}\t{}\t{}\r\n", s.prefab, s.condition, s.by)).collect();
            let _ = std::fs::write(path, text);
        }
    }

    pub(crate) fn broadcast_stash(&mut self) {
        self.ep.broadcast(channel::CONTROL, &Msg::Stash(self.mods.stash.clone()).encode(), None);
        self.signal_kind(NoteKind::Players);
    }

    pub(crate) fn stash_snapshot_for(&self, peer: ct_net::PeerId) {
        if !self.mods.stash.is_empty() {
            let _ = self.ep.send(peer, channel::CONTROL, &Msg::Stash(self.mods.stash.clone()).encode());
        }
    }

    pub(crate) fn on_stash_deposit(&mut self, from: u16, prefab: String, condition: f32) {
        if !self.rules.flag(Id::SharedStash) || prefab.is_empty() || prefab.len() > 64 {
            self.reply(from, vec![(Level::Warning, "shared stash is disabled".into())]);
            return;
        }
        self.mods.next_stash += 1;
        let by = self.name_of(from);
        self.mods.stash.push(StashItem { id: self.mods.next_stash, prefab: prefab.clone(), condition: condition.clamp(0.0, 1.0), by: by.clone() });
        self.save_stash();
        self.broadcast_stash();
        self.system_chat(&format!("{by} put {prefab} into the shared stash"));
    }

    pub(crate) fn on_stash_take(&mut self, from: u16, id: u32) {
        if !self.rules.flag(Id::SharedStash) {
            return;
        }
        let Some(pos) = self.mods.stash.iter().position(|s| s.id == id) else {
            self.reply(from, vec![(Level::Warning, "that item is already gone".into())]);
            return;
        };
        let item = self.mods.stash.remove(pos);
        self.save_stash();
        self.broadcast_stash();
        if from == self.local_id {
            self.mods.grants.push((item.prefab.clone(), item.condition));
        } else if let Some(&peer) = self.player_peer.get(&from) {
            let _ = self.ep.send(peer, channel::CONTROL, &Msg::StashGrant { prefab: item.prefab.clone(), condition: item.condition }.encode());
        }
        let who = self.name_of(from);
        self.system_chat(&format!("{who} took {} from the shared stash", item.prefab));
    }

    pub fn stash_deposit(&mut self, prefab: &str, condition: f32) {
        if self.is_host() {
            let me = self.local_id;
            self.on_stash_deposit(me, prefab.to_owned(), condition);
        } else if let Some(peer) = self.host_peer() {
            let _ = self.ep.send(peer, channel::CONTROL, &Msg::StashDeposit { prefab: prefab.to_owned(), condition }.encode());
        }
    }

    pub fn stash_take(&mut self, id: u32) {
        if self.is_host() {
            let me = self.local_id;
            self.on_stash_take(me, id);
        } else if let Some(peer) = self.host_peer() {
            let _ = self.ep.send(peer, channel::CONTROL, &Msg::StashTake(id).encode());
        }
    }

    pub fn stash(&self) -> &[StashItem] {
        &self.mods.stash
    }

    pub fn take_grants(&mut self) -> Vec<(String, f32)> {
        std::mem::take(&mut self.mods.grants)
    }
}
