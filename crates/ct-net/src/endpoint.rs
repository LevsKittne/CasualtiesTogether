use crate::conn::{Connection, SendError, Stats};
use crate::packet::*;
use crate::transport::{Addr, Transport};
use crate::wire::{Reader, Writer};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub type PeerId = u32;

const CONNECT_RETRY: Duration = Duration::from_millis(250);
const MAX_PAYLOAD: usize = 1024;

#[derive(Debug)]
pub enum Event {
    Request { peer: PeerId, addr: Addr, payload: Vec<u8> },
    Connected { peer: PeerId, addr: Addr },
    Message { peer: PeerId, channel: u8, data: Vec<u8> },
    Disconnected { peer: PeerId, reason: String },
}

#[derive(Clone, Debug)]
pub struct Config {
    pub accept_incoming: bool,
    pub max_peers: usize,
    pub timeout: Duration,
    pub rate_bytes_per_sec: u32,
    pub compress: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            accept_incoming: false,
            max_peers: 32,
            timeout: Duration::from_secs(10),
            rate_bytes_per_sec: 2 << 20,
            compress: true,
        }
    }
}

enum Status {
    Connecting { payload: Vec<u8>, next_try: Instant, started: Instant },
    Pending { since: Instant },
    Connected,
}

struct Peer {
    addr: Addr,
    client_salt: u32,
    status: Status,
    conn: Connection,
}

struct State {
    config: Config,
    peers: HashMap<PeerId, Peer>,
    by_addr: HashMap<Addr, PeerId>,
    next_id: PeerId,
    events: Vec<Event>,
    delivered: Vec<(u8, Vec<u8>)>,
    scratch: Vec<u8>,
}

struct Shared {
    state: Mutex<State>,
    transports: Vec<Arc<dyn Transport>>,
    stop: AtomicBool,
}

impl Shared {
    fn emit(&self, to: &Addr, data: &[u8]) {
        if let Some(t) = self.transports.iter().find(|t| t.handles(to)) {
            t.send(to, data);
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

pub struct Endpoint {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Endpoint {
    pub fn new(config: Config, transports: Vec<Arc<dyn Transport>>) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                config,
                peers: HashMap::new(),
                by_addr: HashMap::new(),
                next_id: 1,
                events: Vec::new(),
                delivered: Vec::new(),
                scratch: Vec::with_capacity(MAX_PACKET),
            }),
            transports,
            stop: AtomicBool::new(false),
        });
        let s = shared.clone();
        let thread = std::thread::Builder::new().name("ct-net".into()).spawn(move || run(s)).ok();
        Self { shared, thread }
    }

    pub fn kill(&self) {
        let mut st = self.shared.lock();
        st.peers.clear();
        st.by_addr.clear();
        self.shared.stop.store(true, Ordering::SeqCst);
    }

    pub fn set_accept_incoming(&self, v: bool) {
        self.shared.lock().config.accept_incoming = v;
    }

    pub fn set_max_peers(&self, v: usize) {
        self.shared.lock().config.max_peers = v;
    }

    pub fn connect(&self, addr: Addr, payload: &[u8]) -> PeerId {
        let now = Instant::now();
        let mut st = self.shared.lock();
        let st = &mut *st;
        let id = st.alloc_id();
        let salt = random_u32();
        let rate = st.config.rate_bytes_per_sec;
        let compress = st.config.compress;
        let payload = payload[..payload.len().min(MAX_PAYLOAD)].to_vec();
        st.scratch.clear();
        Writer::new(&mut st.scratch).u8(KIND_CONNECT).u32(PROTOCOL).u32(salt).bytes(&payload);
        self.shared.emit(&addr, &st.scratch);
        st.peers.insert(
            id,
            Peer {
                addr,
                client_salt: salt,
                status: Status::Connecting { payload, next_try: now + CONNECT_RETRY, started: now },
                conn: Connection::new(0, now, rate, compress),
            },
        );
        st.by_addr.insert(addr, id);
        id
    }

    pub fn accept(&self, peer: PeerId) {
        let now = Instant::now();
        let mut st = self.shared.lock();
        let st = &mut *st;
        let (rate, compress) = (st.config.rate_bytes_per_sec, st.config.compress);
        let Some(p) = st.peers.get_mut(&peer) else { return };
        if !matches!(p.status, Status::Pending { .. }) {
            return;
        }
        let server_salt = random_u32();
        p.conn = Connection::new(p.client_salt ^ server_salt, now, rate, compress);
        p.status = Status::Connected;
        st.scratch.clear();
        Writer::new(&mut st.scratch).u8(KIND_ACCEPT).u32(p.client_salt).u32(server_salt);
        self.shared.emit(&p.addr, &st.scratch);
        st.events.push(Event::Connected { peer, addr: p.addr });
    }

    pub fn deny(&self, peer: PeerId, reason: &str) {
        let mut st = self.shared.lock();
        if let Some(p) = st.remove(peer) {
            let mut b = Vec::new();
            Writer::new(&mut b).u8(KIND_DENY).u32(p.client_salt).bytes(reason.as_bytes());
            self.shared.emit(&p.addr, &b);
        }
    }

    pub fn disconnect(&self, peer: PeerId, reason: &str) {
        let mut st = self.shared.lock();
        if let Some(p) = st.remove(peer) {
            if matches!(p.status, Status::Connected) {
                let mut b = Vec::new();
                Writer::new(&mut b).u8(KIND_DISCONNECT).u32(p.conn.salt).bytes(reason.as_bytes());
                for _ in 0..3 {
                    self.shared.emit(&p.addr, &b);
                }
            }
            st.events.push(Event::Disconnected { peer, reason: reason.to_owned() });
        }
    }

    pub fn disconnect_all(&self, reason: &str) {
        let ids: Vec<PeerId> = self.shared.lock().peers.keys().copied().collect();
        for id in ids {
            self.disconnect(id, reason);
        }
    }

    pub fn send(&self, peer: PeerId, channel: u8, data: &[u8]) -> Result<(), SendError> {
        let mut st = self.shared.lock();
        match st.peers.get_mut(&peer) {
            Some(p) if matches!(p.status, Status::Connected) => p.conn.send(channel, data),
            _ => Err(SendError::BadChannel),
        }
    }

    pub fn broadcast(&self, channel: u8, data: &[u8], except: Option<PeerId>) {
        let mut st = self.shared.lock();
        for (id, p) in st.peers.iter_mut() {
            if Some(*id) != except && matches!(p.status, Status::Connected) {
                let _ = p.conn.send(channel, data);
            }
        }
    }

    pub fn flush(&self) {
        let now = Instant::now();
        let mut st = self.shared.lock();
        for p in st.peers.values_mut() {
            if matches!(p.status, Status::Connected) {
                let addr = p.addr;
                p.conn.flush(now, &mut |b| self.shared.emit(&addr, b));
            }
        }
    }

    pub fn poll(&self, out: &mut Vec<Event>) {
        let mut st = self.shared.lock();
        out.append(&mut st.events);
    }

    pub fn stats(&self, peer: PeerId) -> Option<Stats> {
        self.shared.lock().peers.get(&peer).map(|p| p.conn.stats)
    }

    pub fn backlog(&self, peer: PeerId) -> usize {
        self.shared.lock().peers.get(&peer).map_or(0, |p| p.conn.reliable_backlog())
    }

    pub fn addr(&self, peer: PeerId) -> Option<Addr> {
        self.shared.lock().peers.get(&peer).map(|p| p.addr)
    }

    pub fn connected(&self) -> Vec<PeerId> {
        let st = self.shared.lock();
        st.peers.iter().filter(|(_, p)| matches!(p.status, Status::Connected)).map(|(id, _)| *id).collect()
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        self.disconnect_all("shutdown");
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl State {
    fn alloc_id(&mut self) -> PeerId {
        loop {
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            if !self.peers.contains_key(&id) {
                return id;
            }
        }
    }

    fn remove(&mut self, peer: PeerId) -> Option<Peer> {
        let p = self.peers.remove(&peer)?;
        if self.by_addr.get(&p.addr) == Some(&peer) {
            self.by_addr.remove(&p.addr);
        }
        Some(p)
    }

    fn handle(&mut self, sh: &Shared, now: Instant, addr: Addr, data: &[u8]) {
        let Some((&kind, body)) = data.split_first() else { return };
        let mut r = Reader::new(body);
        match kind {
            KIND_CONNECT => self.on_connect(sh, now, addr, &mut r),
            KIND_ACCEPT => {
                let (Some(cs), Some(ss)) = (r.u32(), r.u32()) else { return };
                let Some(&id) = self.by_addr.get(&addr) else { return };
                let (rate, compress) = (self.config.rate_bytes_per_sec, self.config.compress);
                let p = self.peers.get_mut(&id).unwrap();
                if matches!(p.status, Status::Connecting { .. }) && p.client_salt == cs {
                    p.conn = Connection::new(cs ^ ss, now, rate, compress);
                    p.status = Status::Connected;
                    self.events.push(Event::Connected { peer: id, addr });
                }
            }
            KIND_DENY => {
                let Some(cs) = r.u32() else { return };
                let Some(&id) = self.by_addr.get(&addr) else { return };
                let p = &self.peers[&id];
                if matches!(p.status, Status::Connecting { .. }) && p.client_salt == cs {
                    let reason = String::from_utf8_lossy(r.rest()).into_owned();
                    self.remove(id);
                    self.events.push(Event::Disconnected { peer: id, reason });
                }
            }
            KIND_DATA => {
                let Some(salt) = r.u32() else { return };
                let Some(&id) = self.by_addr.get(&addr) else { return };
                let p = self.peers.get_mut(&id).unwrap();
                if !matches!(p.status, Status::Connected) || p.conn.salt != salt {
                    return;
                }
                let mut delivered = std::mem::take(&mut self.delivered);
                let _ = p.conn.receive(now, r.rest(), &mut delivered);
                if p.conn.owes_ack() {
                    let addr = p.addr;
                    p.conn.flush(now, &mut |b| sh.emit(&addr, b));
                }
                for (channel, data) in delivered.drain(..) {
                    self.events.push(Event::Message { peer: id, channel, data });
                }
                self.delivered = delivered;
            }
            KIND_DISCONNECT => {
                let Some(salt) = r.u32() else { return };
                let Some(&id) = self.by_addr.get(&addr) else { return };
                let p = &self.peers[&id];
                if matches!(p.status, Status::Connected) && p.conn.salt == salt {
                    let reason = String::from_utf8_lossy(r.rest()).into_owned();
                    self.remove(id);
                    self.events.push(Event::Disconnected { peer: id, reason });
                }
            }
            _ => {}
        }
    }

    fn on_connect(&mut self, sh: &Shared, now: Instant, addr: Addr, r: &mut Reader) {
        if !self.config.accept_incoming {
            return;
        }
        let (Some(proto), Some(cs)) = (r.u32(), r.u32()) else { return };
        if let Some(&id) = self.by_addr.get(&addr) {
            let p = &self.peers[&id];
            if p.client_salt == cs {
                if matches!(p.status, Status::Connected) {
                    let ss = p.conn.salt ^ cs;
                    let mut b = Vec::new();
                    Writer::new(&mut b).u8(KIND_ACCEPT).u32(cs).u32(ss);
                    sh.emit(&addr, &b);
                }
                return;
            }
            self.remove(id);
            self.events.push(Event::Disconnected { peer: id, reason: "reconnected".into() });
        }
        let deny = |reason: &str| {
            let mut b = Vec::new();
            Writer::new(&mut b).u8(KIND_DENY).u32(cs).bytes(reason.as_bytes());
            sh.emit(&addr, &b);
        };
        if proto != PROTOCOL {
            deny("protocol mismatch");
            return;
        }
        if self.peers.len() >= self.config.max_peers {
            deny("server is full");
            return;
        }
        let id = self.alloc_id();
        let conn = Connection::new(0, now, self.config.rate_bytes_per_sec, self.config.compress);
        self.peers.insert(id, Peer { addr, client_salt: cs, status: Status::Pending { since: now }, conn });
        self.by_addr.insert(addr, id);
        self.events.push(Event::Request { peer: id, addr, payload: r.rest().to_vec() });
    }

    fn update(&mut self, sh: &Shared, now: Instant) {
        let timeout = self.config.timeout;
        let mut dead: Vec<(PeerId, Option<&'static str>)> = Vec::new();
        for (&id, p) in self.peers.iter_mut() {
            match &mut p.status {
                Status::Connecting { payload, next_try, started } => {
                    if now.saturating_duration_since(*started) > timeout {
                        dead.push((id, Some("connection timed out")));
                    } else if now >= *next_try {
                        *next_try = now + CONNECT_RETRY;
                        self.scratch.clear();
                        Writer::new(&mut self.scratch).u8(KIND_CONNECT).u32(PROTOCOL).u32(p.client_salt).bytes(payload);
                        sh.emit(&p.addr, &self.scratch);
                    }
                }
                Status::Pending { since } => {
                    if now.saturating_duration_since(*since) > timeout {
                        dead.push((id, None));
                    }
                }
                Status::Connected => {
                    if now.saturating_duration_since(p.conn.last_recv) > timeout {
                        dead.push((id, Some("timed out")));
                    } else {
                        let addr = p.addr;
                        p.conn.flush(now, &mut |b| sh.emit(&addr, b));
                    }
                }
            }
        }
        for (id, reason) in dead {
            self.remove(id);
            if let Some(reason) = reason {
                self.events.push(Event::Disconnected { peer: id, reason: reason.into() });
            }
        }
    }
}

fn run(sh: Arc<Shared>) {
    let mut buf = vec![0u8; 2048];
    let mut inbox: Vec<(Addr, Vec<u8>)> = Vec::with_capacity(256);
    let mut pool: Vec<Vec<u8>> = Vec::new();
    let waits = sh.transports.iter().any(|t| t.waits());
    let mut last_update = Instant::now();
    while !sh.stop.load(Ordering::Relaxed) {
        for t in &sh.transports {
            while inbox.len() < 512 {
                let Some((n, from)) = t.recv(&mut buf) else { break };
                let mut v = pool.pop().unwrap_or_default();
                v.clear();
                v.extend_from_slice(&buf[..n]);
                inbox.push((from, v));
            }
        }
        if !waits && inbox.is_empty() {
            std::thread::sleep(Duration::from_millis(1));
        }
        let now = Instant::now();
        if inbox.is_empty() && now.saturating_duration_since(last_update) < Duration::from_millis(2) {
            continue;
        }
        last_update = now;
        let mut st = sh.lock();
        for (from, data) in inbox.drain(..) {
            st.handle(&sh, now, from, &data);
            pool.push(data);
        }
        st.update(&sh, now);
    }
}

pub fn random_u32() -> u32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    (h.finish() as u32) | 1
}
