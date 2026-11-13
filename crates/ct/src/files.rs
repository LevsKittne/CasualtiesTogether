use crate::session::{Level, Session};
use ct_net::wire::{Reader, Writer};
use ct_net::{PeerId, channel};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

pub const FILE_TAG: u8 = 40;
const CHUNK: usize = 256 * 1024;
const MAX_FILE: usize = 64 << 20;
const MAX_BACKLOG: usize = 1024;

struct Outgoing {
    xfer: u32,
    from: u16,
    name: String,
    data: Vec<u8>,
    offset: usize,
    targets: Vec<PeerId>,
}

struct Incoming {
    name: String,
    from: u16,
    total: usize,
    data: Vec<u8>,
}

#[derive(Default)]
pub struct Files {
    out: VecDeque<Outgoing>,
    incoming: HashMap<(u16, u32), Incoming>,
    next_xfer: u32,
    pub received: Vec<PathBuf>,
}

pub struct Progress {
    pub name: String,
    pub done: usize,
    pub total: usize,
    pub sending: bool,
}

fn encode(xfer: u32, from: u16, name: &str, total: usize, offset: usize, chunk: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(chunk.len() + name.len() + 24);
    Writer::new(&mut out).u8(FILE_TAG).u32(xfer).u16(from).str(name).u32(total as u32).u32(offset as u32).blob(chunk);
    out
}

fn sanitize(name: &str) -> String {
    let s: String = name.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' }).take(64).collect();
    if s.is_empty() || s.starts_with('.') { "file".into() } else { s }
}

impl Session {
    pub fn send_file(&mut self, name: &str, data: Vec<u8>) -> Result<(), String> {
        if data.len() > MAX_FILE {
            return Err("file is too large".into());
        }
        let targets: Vec<PeerId> = if self.is_host() { self.player_peer.values().copied().collect() } else { self.host_peer().into_iter().collect() };
        if targets.is_empty() {
            return Err("nobody to send to".into());
        }
        self.files.next_xfer += 1;
        let from = self.local_id;
        self.files.out.push_back(Outgoing { xfer: self.files.next_xfer, from, name: sanitize(name), data, offset: 0, targets });
        Ok(())
    }

    pub(crate) fn tick_files(&mut self) {
        let Some(o) = self.files.out.front_mut() else { return };
        if o.targets.iter().any(|p| self.ep.backlog(*p) > MAX_BACKLOG) {
            return;
        }
        let end = (o.offset + CHUNK).min(o.data.len());
        let msg = encode(o.xfer, o.from, &o.name, o.data.len(), o.offset, &o.data[o.offset..end]);
        for p in &o.targets {
            let _ = self.ep.send(*p, channel::BULK, &msg);
        }
        o.offset = end;
        if o.offset >= o.data.len() {
            let name = o.name.clone();
            self.files.out.pop_front();
            self.note(Level::Info, format!("sent {name}"));
        }
    }

    pub(crate) fn on_file(&mut self, sender: Option<PeerId>, data: &[u8]) {
        let mut r = Reader::new(data);
        let parsed = (|| {
            r.u8()?;
            Some((r.u32()?, r.u16()?, r.str()?.to_owned(), r.u32()? as usize, r.u32()? as usize, r.blob()?.to_vec()))
        })();
        let Some((xfer, from, name, total, offset, chunk)) = parsed else { return };
        if total > MAX_FILE {
            return;
        }
        if self.is_host() {
            for (&peer, _) in self.peer_player.iter() {
                if Some(peer) != sender {
                    let _ = self.ep.send(peer, channel::BULK, data);
                }
            }
        }
        let key = (from, xfer);
        let entry = self.files.incoming.entry(key).or_insert_with(|| Incoming { name: sanitize(&name), from, total, data: Vec::with_capacity(total) });
        if offset != entry.data.len() {
            return;
        }
        entry.data.extend_from_slice(&chunk);
        if entry.data.len() < entry.total {
            return;
        }
        let done = self.files.incoming.remove(&key).unwrap();
        let who = sanitize(&self.name_of(done.from));
        let Some(dir) = self.data_path("mp_received") else { return };
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{who}_{}", done.name));
        match std::fs::write(&path, &done.data) {
            Ok(()) => {
                self.note(Level::Info, format!("received {} from {} ({} bytes)", done.name, self.name_of(done.from), done.data.len()));
                self.files.received.retain(|p| p != &path);
                self.files.received.push(path);
            }
            Err(e) => self.note(Level::Error, format!("cannot save {}: {e}", done.name)),
        }
    }

    pub fn file_progress(&self) -> Vec<Progress> {
        let mut v: Vec<Progress> = self.files.out.iter().map(|o| Progress { name: o.name.clone(), done: o.offset, total: o.data.len(), sending: true }).collect();
        v.extend(self.files.incoming.values().map(|i| Progress { name: i.name.clone(), done: i.data.len(), total: i.total, sending: false }));
        v
    }

    pub fn received_files(&self) -> &[PathBuf] {
        &self.files.received
    }
}
