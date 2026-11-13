use crate::bans::BanList;
use crate::proto::{self, Hello, Msg, Successor};
use crate::session::{Level, NoteKind, Role, Session};
use ct_net::{Addr, channel};
use std::time::{Duration, Instant};

const SUCCESSION_INTERVAL: Duration = Duration::from_secs(3);
pub const TRANSFER_REASON: &str = "host transfer";

impl Session {
    pub(crate) fn tick_succession(&mut self) {
        if self.role != Role::Host || self.last_succession.elapsed() < SUCCESSION_INTERVAL {
            return;
        }
        self.last_succession = Instant::now();
        let list = self.succession_list();
        self.ep.broadcast(channel::CONTROL, &Msg::Succession(list).encode(), None);
    }

    fn succession_list(&self) -> Vec<Successor> {
        let mut list: Vec<Successor> = self
            .player_peer
            .iter()
            .filter_map(|(&id, &peer)| {
                let addr = self.ep.addr(peer)?;
                let name = self.players.get(&id)?.name.clone();
                Some(Successor { id, name, addr })
            })
            .collect();
        list.sort_by_key(|s| s.id);
        list
    }

    pub(crate) fn on_host_lost(&mut self, reason: &str) -> bool {
        if let Some(to) = reason.strip_prefix("host transfer:").and_then(|t| t.parse::<u16>().ok()) {
            self.on_host_transfer(to);
            return self.role == Role::Host || self.host_peer.is_some();
        }
        if self.migration.is_some() {
            self.note(Level::Warning, format!("successor unreachable: {reason}"));
            return self.try_next_candidate();
        }
        let crashed = reason == "timed out" || reason == "shutdown";
        if crashed && self.welcomed && !self.successors.is_empty() {
            self.note(Level::Warning, format!("host lost ({reason}), migrating"));
            self.players.remove(&self.host_id);
            self.tracks.remove(&self.host_id);
            self.signal_kind(NoteKind::Players);
            self.migration = Some(self.successors.clone());
            return self.try_next_candidate();
        }
        self.end_client(reason)
    }

    fn end_client(&mut self, reason: &str) -> bool {
        self.note(Level::Warning, format!("disconnected: {reason}"));
        self.players.clear();
        self.host_peer = None;
        self.signal_kind(NoteKind::Players);
        false
    }

    fn try_next_candidate(&mut self) -> bool {
        let next = match self.migration.as_mut() {
            Some(list) if !list.is_empty() => list.remove(0),
            _ => {
                self.migration = None;
                return self.end_client("no successor available");
            }
        };
        if next.id == self.local_id {
            self.migration = None;
            self.become_host();
            return true;
        }
        let me = self.local_id;
        self.players.retain(|&id, _| id == me || id >= next.id);
        self.host_id = next.id;
        self.signal_kind(NoteKind::Players);
        self.note(Level::Info, format!("connecting to new host {}", next.name));
        self.become_client(next.addr);
        true
    }

    pub(crate) fn become_host(&mut self) {
        let me = self.local_id;
        self.role = Role::Host;
        self.host_id = me;
        self.host_peer = None;
        self.welcomed = true;
        self.players.retain(|&id, _| id == me);
        if let Some(p) = self.players.get_mut(&me) {
            p.admin = true;
            p.ping_ms = 0;
        }
        self.player_peer.clear();
        self.peer_player.clear();
        self.pending.clear();
        self.resuming.clear();
        self.tracks.clear();
        let top = self.successors.iter().map(|s| s.id).chain(self.players.keys().copied()).max().unwrap_or(0);
        self.successors.clear();
        self.next_player = top.max(me).wrapping_add(1).max(1);
        self.bans = BanList::load(self.data_path("mp_banlist.txt").as_deref());
        self.mods.mark_loaded();
        self.ep.set_max_peers(64);
        self.ep.set_accept_incoming(true);
        self.last_succession = Instant::now() - Duration::from_secs(60);
        self.note(Level::Info, "you are now the host");
        self.signal_kind(NoteKind::Players);
    }

    pub(crate) fn become_client(&mut self, addr: Addr) {
        self.role = Role::Client;
        self.ep.set_accept_incoming(false);
        self.player_peer.clear();
        self.peer_player.clear();
        self.pending.clear();
        self.resuming.clear();
        let mut hello = Vec::new();
        Hello { version: proto::VERSION, name: &self.my_name, password: &self.password, prev_id: self.local_id, resume: true }.write(&mut hello);
        self.started = Instant::now();
        self.host_peer = Some(self.ep.connect(addr, &hello));
    }

    pub fn transfer_host(&mut self, to: u16) -> Result<(), String> {
        if self.role != Role::Host {
            return Err("only the host can transfer the host role".into());
        }
        if to == self.local_id {
            return Err("you are already the host".into());
        }
        let peer = *self.player_peer.get(&to).ok_or("no such player")?;
        let addr = self.ep.addr(peer).ok_or("player address unknown")?;
        let name = self.name_of(to);
        let list = self.succession_list();
        self.ep.broadcast(channel::CONTROL, &Msg::Succession(list).encode(), None);
        self.ep.broadcast(channel::CONTROL, &Msg::HostTransfer { to }.encode(), None);
        self.ep.flush();
        let reason = format!("host transfer:{to}");
        for p in self.peer_player.keys().copied().collect::<Vec<_>>() {
            self.ep.disconnect(p, &reason);
        }
        self.host_id = to;
        self.note(Level::Info, format!("host role handed to {name}"));
        self.become_client(addr);
        Ok(())
    }

    #[cfg(test)]
    pub fn kill(&self) {
        self.ep.kill();
    }

    pub fn transfer_target(&self) -> Option<u16> {
        self.player_peer.keys().copied().min()
    }

    pub(crate) fn on_host_transfer(&mut self, to: u16) {
        if self.role == Role::Host {
            return;
        }
        let old = self.host_peer.take();
        let old_host = self.host_id;
        self.migration = None;
        self.players.remove(&old_host);
        self.tracks.remove(&old_host);
        if to == self.local_id {
            self.become_host();
        } else if let Some(s) = self.successors.iter().find(|s| s.id == to).cloned() {
            self.host_id = to;
            self.note(Level::Info, format!("host role moved to {}", s.name));
            self.become_client(s.addr);
        } else {
            self.migration = Some(self.successors.clone());
            self.try_next_candidate();
        }
        if let Some(p) = old {
            self.ep.disconnect(p, TRANSFER_REASON);
        }
        self.signal_kind(NoteKind::Players);
    }
}
