use crate::bans::BanList;
use crate::proto::{self, Hello, Msg, PlayerInfo};
use crate::rules::{Id, Rules};
use crate::state::{Snapshot, Track};
use crate::world::{self, EntityCall, TileEdit, World};
use ct_net::wire::{Reader, Writer};
use ct_net::{Addr, Config, Endpoint, Event, PeerId, SteamTransport, Transport, UdpTransport, channel};
use std::collections::{BTreeMap, HashMap};
use std::net::{SocketAddr, ToSocketAddrs};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Level {
    Debug = 0,
    Info = 1,
    Warning = 2,
    Error = 3,
}

impl Level {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Level::Debug,
            2 => Level::Warning,
            3 => Level::Error,
            _ => Level::Info,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum NoteKind {
    Log = 0,
    Chat = 1,
    Players = 2,
    Rules = 3,
    World = 4,
}

pub struct Note {
    pub kind: NoteKind,
    pub level: Level,
    pub text: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Host,
    Client,
}

pub struct HostOptions {
    pub port: u16,
    pub steam: Option<Arc<SteamTransport>>,
    pub name: String,
    pub password: String,
    pub admin_password: String,
    pub max_players: Option<u8>,
    pub data_dir: Option<PathBuf>,
}

pub struct Session {
    pub(crate) ep: Endpoint,
    pub(crate) role: Role,
    pub(crate) local_id: u16,
    pub(crate) password: String,
    pub(crate) admin_password: String,
    pub(crate) rules: Rules,
    pub(crate) players: BTreeMap<u16, PlayerInfo>,
    pub(crate) player_peer: HashMap<u16, PeerId>,
    pub(crate) peer_player: HashMap<PeerId, u16>,
    pub(crate) pending: HashMap<PeerId, PlayerInfo>,
    pub(crate) host_peer: Option<PeerId>,
    pub(crate) next_player: u16,
    last_pings: Instant,
    events: Vec<Event>,
    pub(crate) welcomed: bool,
    pub(crate) started: Instant,
    pub(crate) bans: BanList,
    pub(crate) data_dir: Option<PathBuf>,
    chat_budget: HashMap<u16, (f32, Instant)>,
    notes: Vec<Note>,
    clock: Instant,
    pub(crate) local_state: Option<Snapshot>,
    last_state_send: Instant,
    pub(crate) tracks: HashMap<u16, Track>,
    pose_buf: Vec<u8>,
    pub(crate) world: World,
    motion_in: std::collections::VecDeque<Vec<u8>>,
    pub(crate) mods: crate::modifiers::Modifiers,
    pub(crate) files: crate::files::Files,
    pub(crate) host_id: u16,
    pub(crate) my_name: String,
    pub(crate) successors: Vec<proto::Successor>,
    pub(crate) migration: Option<Vec<proto::Successor>>,
    pub(crate) last_succession: Instant,
    pub(crate) resuming: std::collections::HashSet<PeerId>,
}

const POSE_TAG: u8 = 20;
const MOTION_TAG: u8 = 21;
const POSE_INTERVAL: Duration = Duration::from_millis(33);

pub fn parse_addr(s: &str) -> Result<Addr, String> {
    let s = s.trim();
    if let Some(id) = s.strip_prefix("steam:") {
        return id.trim().parse::<u64>().map(Addr::Steam).map_err(|_| format!("bad steam id: {id}"));
    }
    let with_port = if s.parse::<SocketAddr>().is_ok() || (s.contains(':') && !s.contains("::")) {
        s.to_owned()
    } else {
        format!("{s}:{}", proto::DEFAULT_PORT)
    };
    with_port
        .to_socket_addrs()
        .map_err(|e| format!("cannot resolve {s}: {e}"))?
        .find(|a| a.is_ipv4())
        .map(Addr::Udp)
        .ok_or_else(|| format!("no IPv4 address for {s}"))
}

impl Session {
    pub fn host(o: HostOptions) -> Result<Self, String> {
        let udp = UdpTransport::bind(SocketAddr::from(([0, 0, 0, 0], o.port))).map_err(|e| format!("cannot bind UDP {}: {e}", o.port))?;
        let mut transports: Vec<Arc<dyn Transport>> = vec![Arc::new(udp)];
        if let Some(s) = o.steam {
            transports.push(s);
        }
        let cfg = Config { accept_incoming: true, max_peers: 64, ..Default::default() };
        let mut s = Self::new(Endpoint::new(cfg, transports), Role::Host, &o.password);
        s.admin_password = o.admin_password;
        s.data_dir = o.data_dir;
        s.bans = BanList::load(s.data_path("mp_banlist.txt").as_deref());
        if let Some(p) = s.data_path("mp_rules.ini") {
            if let Ok(text) = std::fs::read_to_string(p) {
                s.rules.apply_ini(text.lines());
            }
        }
        if let Some(m) = o.max_players {
            let _ = s.rules.set_str("PLAYER_COUNT_LIMIT", &m.clamp(2, 64).to_string());
        }
        let name = proto::sanitize_name(&o.name);
        s.my_name = name.clone();
        s.players.insert(proto::HOST_ID, PlayerInfo { id: proto::HOST_ID, name, ping_ms: 0, admin: true });
        s.welcomed = true;
        Ok(s)
    }

    pub fn connect(addr: Addr, steam: Option<Arc<SteamTransport>>, name: &str, password: &str) -> Result<Self, String> {
        let transport: Arc<dyn Transport> = match addr {
            Addr::Udp(_) => Arc::new(UdpTransport::bind(SocketAddr::from(([0, 0, 0, 0], 0))).map_err(|e| e.to_string())?),
            Addr::Steam(_) => steam.ok_or("Steam is not available")?,
        };
        let name = proto::sanitize_name(name);
        let ep = Endpoint::new(Config::default(), vec![transport]);
        let mut hello = Vec::new();
        Hello { version: proto::VERSION, name: &name, password, prev_id: proto::NO_ID, resume: false }.write(&mut hello);
        let peer = ep.connect(addr, &hello);
        let mut s = Self::new(ep, Role::Client, password);
        s.my_name = name;
        s.host_peer = Some(peer);
        Ok(s)
    }

    fn new(ep: Endpoint, role: Role, password: &str) -> Self {
        let now = Instant::now();
        Self {
            ep,
            role,
            local_id: proto::HOST_ID,
            password: password.to_owned(),
            admin_password: String::new(),
            rules: Rules::default(),
            players: BTreeMap::new(),
            player_peer: HashMap::new(),
            peer_player: HashMap::new(),
            pending: HashMap::new(),
            host_peer: None,
            next_player: 1,
            last_pings: now,
            events: Vec::new(),
            welcomed: false,
            started: now,
            bans: BanList::load(None),
            data_dir: None,
            chat_budget: HashMap::new(),
            notes: Vec::new(),
            clock: now,
            local_state: None,
            last_state_send: now,
            tracks: HashMap::new(),
            pose_buf: Vec::with_capacity(160),
            world: World::default(),
            motion_in: std::collections::VecDeque::new(),
            mods: Default::default(),
            files: Default::default(),
            host_id: proto::HOST_ID,
            my_name: String::new(),
            successors: Vec::new(),
            migration: None,
            last_succession: now,
            resuming: Default::default(),
        }
    }

    pub(crate) fn data_path(&self, file: &str) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join(file))
    }

    pub(crate) fn note(&mut self, level: Level, text: impl Into<String>) {
        self.notes.push(Note { kind: NoteKind::Log, level, text: text.into() });
    }

    pub(crate) fn signal_kind(&mut self, kind: NoteKind) {
        self.signal(kind);
    }

    pub(crate) fn host_peer(&self) -> Option<PeerId> {
        self.host_peer
    }

    fn signal(&mut self, kind: NoteKind) {
        if !self.notes.iter().any(|n| n.kind == kind && n.text.is_empty()) {
            self.notes.push(Note { kind, level: Level::Debug, text: String::new() });
        }
    }

    pub fn drain_notes(&mut self, out: &mut impl Extend<Note>) {
        out.extend(self.notes.drain(..));
    }

    pub(crate) fn apply_rename(&mut self, id: u16, name: String) {
        let Some(p) = self.players.get_mut(&id) else { return };
        let old = std::mem::replace(&mut p.name, name.clone());
        if id == self.local_id {
            self.my_name = name.clone();
        }
        self.note(Level::Info, format!("{old} is now {name}"));
        self.signal(NoteKind::Players);
    }

    pub fn host_id(&self) -> u16 {
        self.host_id
    }

    pub fn is_host(&self) -> bool {
        self.role == Role::Host
    }

    pub fn local_id(&self) -> u16 {
        self.local_id
    }

    pub fn players(&self) -> impl Iterator<Item = &PlayerInfo> {
        self.players.values()
    }

    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    pub(crate) fn name_of(&self, id: u16) -> String {
        self.players.get(&id).map_or_else(|| format!("#{id}"), |p| p.name.clone())
    }

    pub fn tick(&mut self) -> bool {
        let mut events = std::mem::take(&mut self.events);
        self.ep.poll(&mut events);
        let mut alive = true;
        for e in events.drain(..) {
            alive &= match self.role {
                Role::Host => self.host_event(e),
                Role::Client => self.client_event(e),
            };
        }
        self.events = events;
        self.send_pose();
        self.send_edits();
        self.tick_succession();
        self.tick_modifiers();
        alive &= self.role == Role::Host || self.host_peer.is_some();
        self.tick_files();
        if self.role == Role::Host && self.last_pings.elapsed() >= Duration::from_secs(1) {
            self.last_pings = Instant::now();
            self.broadcast_pings();
        }
        self.ep.flush();
        alive
    }

    fn host_event(&mut self, e: Event) -> bool {
        match e {
            Event::Request { peer, addr, payload } => {
                self.on_request(peer, addr, &payload);
            }
            Event::Connected { peer, addr } => {
                let Some(info) = self.pending.remove(&peer) else { return true };
                self.peer_player.insert(peer, info.id);
                self.player_peer.insert(info.id, peer);
                self.players.insert(info.id, info.clone());
                let welcome = Msg::Welcome { you: info.id, host: self.host_id, players: self.players.values().cloned().collect(), rules: self.rules.clone() };
                let _ = self.ep.send(peer, channel::CONTROL, &welcome.encode());
                self.ep.broadcast(channel::CONTROL, &Msg::Joined(info.clone()).encode(), Some(peer));
                let resumed = self.resuming.remove(&peer);
                self.last_succession = Instant::now() - Duration::from_secs(60);
                if resumed && self.world.generation != 0 {
                    let _ = self.ep.send(peer, channel::STATE, &world::encode_params(self.world.generation, &self.world.params));
                    self.stash_snapshot_for(peer);
                } else if self.world.generation != 0 {
                    let _ = self.ep.send(peer, channel::STATE, &world::encode_params(self.world.generation, &self.world.params));
                    for m in world::encode_edits(self.world.generation, &self.world.snapshot()) {
                        let _ = self.ep.send(peer, channel::STATE, &m);
                    }
                    self.stash_snapshot_for(peer);
                    for c in self.world.journal() {
                        let _ = self.ep.send(peer, channel::STATE, &c.encode(world::CALL_APPLY_TAG, self.world.generation));
                    }
                }
                self.note(Level::Info, format!("{} joined ({addr}), id {}", info.name, info.id));
                self.signal(NoteKind::Players);
            }
            Event::Message { peer, channel: channel::POSE, data } => {
                let Some(&from) = self.peer_player.get(&peer) else { return true };
                let mut data = data;
                if data.first() == Some(&MOTION_TAG) {
                    if data.len() > 5 && self.world.is_ready() && u32::from_le_bytes([data[1], data[2], data[3], data[4]]) == self.world.generation {
                        self.ep.broadcast(channel::POSE, &data, Some(peer));
                        self.push_motion(data);
                    }
                } else if self.ingest_pose(from, &data) {
                    data[1..3].copy_from_slice(&from.to_le_bytes());
                    self.ep.broadcast(channel::POSE, &data, Some(peer));
                }
            }
            Event::Message { peer, channel: channel::VOICE, mut data } => {
                if let Some(&id) = self.peer_player.get(&peer) {
                    self.on_voice_from(id, &mut data);
                }
            }
            Event::Message { peer, channel: channel::BULK, data } => {
                if self.peer_player.contains_key(&peer) {
                    self.on_file(Some(peer), &data);
                }
            }
            Event::Message { peer, channel: channel::STATE, data } => {
                if !self.peer_player.contains_key(&peer) {
                    return true;
                }
                if let Some((generation, list)) = world::decode_edits(&data) {
                    if generation == self.world.generation {
                        self.world.remote_edits(generation, &list);
                        self.ep.broadcast(channel::STATE, &data, Some(peer));
                    }
                } else if let Some((generation, call)) = EntityCall::decode(&data, world::CALL_REQ_TAG) {
                    if generation == self.world.generation {
                        self.world.record_call(&call);
                        self.ep.broadcast(channel::STATE, &call.encode(world::CALL_APPLY_TAG, generation), None);
                        self.world.incoming_call(generation, call);
                    }
                }
            }
            Event::Message { peer, channel: ch, data } => {
                let Some(&from) = self.peer_player.get(&peer) else { return true };
                if ch != channel::CONTROL && ch != channel::CHAT {
                    return true;
                }
                match Msg::decode(&data) {
                    Some(Msg::Chat { text, .. }) => self.handle_chat(from, &text),
                    Some(Msg::Command(line)) => {
                        let replies = self.exec(from, &line);
                        self.reply(from, replies);
                    }
                    Some(Msg::AdminLogin(pass)) => self.admin_login(from, &pass),
                    Some(Msg::NeedsDelta(d)) => self.on_needs_delta(d),
                    Some(Msg::StashDeposit { prefab, condition }) => self.on_stash_deposit(from, prefab, condition),
                    Some(Msg::StashTake(id)) => self.on_stash_take(from, id),
                    _ => {}
                }
            }
            Event::Disconnected { peer, reason } => {
                self.pending.remove(&peer);
                self.resuming.remove(&peer);
                if let Some(id) = self.peer_player.remove(&peer) {
                    self.player_peer.remove(&id);
                    self.chat_budget.remove(&id);
                    self.tracks.remove(&id);
                    if let Some(p) = self.players.remove(&id) {
                        self.ep.broadcast(channel::CONTROL, &Msg::Left { id, reason: reason.clone() }.encode(), None);
                        self.last_succession = Instant::now() - Duration::from_secs(60);
                        self.note(Level::Info, format!("{} left: {reason}", p.name));
                        self.signal(NoteKind::Players);
                    }
                }
            }
        }
        true
    }

    fn on_request(&mut self, peer: PeerId, addr: Addr, payload: &[u8]) {
        let Some(hello) = Hello::read(payload) else {
            self.ep.deny(peer, "malformed hello");
            return;
        };
        if hello.version != proto::VERSION {
            self.ep.deny(peer, &format!("version mismatch (server v{}, you v{})", proto::VERSION, hello.version));
            return;
        }
        if let Some(b) = self.bans.find(&addr) {
            let reason = if b.reason.is_empty() { "banned".to_owned() } else { format!("banned: {}", b.reason) };
            self.ep.deny(peer, &reason);
            self.note(Level::Info, format!("{addr} ({}) denied: banned", hello.name));
            return;
        }
        if !self.password.is_empty() && hello.password != self.password {
            self.ep.deny(peer, "wrong password");
            self.note(Level::Warning, format!("{addr} denied: wrong password"));
            return;
        }
        let limit = self.rules.get(Id::PLAYER_COUNT_LIMIT).as_f32() as usize;
        if self.players.len() + self.pending.len() >= limit {
            self.ep.deny(peer, "server is full");
            return;
        }
        let name = self.unique_name(&proto::sanitize_name(hello.name));
        let reclaim = hello.prev_id != proto::NO_ID
            && hello.prev_id != self.local_id
            && !self.players.contains_key(&hello.prev_id)
            && !self.pending.values().any(|p| p.id == hello.prev_id);
        let id = if reclaim { hello.prev_id } else { self.alloc_player() };
        if hello.resume {
            self.resuming.insert(peer);
        }
        self.pending.insert(peer, PlayerInfo { id, name, ping_ms: 0, admin: false });
        self.ep.accept(peer);
    }

    fn client_event(&mut self, e: Event) -> bool {
        match e {
            Event::Connected { addr, .. } => self.note(Level::Debug, format!("connected to {addr}, waiting for welcome")),
            Event::Message { channel: channel::POSE, data, .. } if data.first() == Some(&MOTION_TAG) => {
                if data.len() > 5 && self.world.is_ready() && u32::from_le_bytes([data[1], data[2], data[3], data[4]]) == self.world.generation {
                    self.push_motion(data);
                }
            }
            Event::Message { channel: channel::POSE, data, .. } => {
                if let Some(id) = data.get(1..3).map(|b| u16::from_le_bytes([b[0], b[1]])) {
                    if id != self.local_id {
                        self.ingest_pose(id, &data);
                    }
                }
            }
            Event::Message { channel: channel::VOICE, data, .. } => self.on_voice_client(&data),
            Event::Message { channel: channel::BULK, data, .. } => self.on_file(None, &data),
            Event::Message { channel: channel::STATE, data, .. } => {
                if let Some((generation, params)) = world::decode_params(&data) {
                    if self.world.adopt(generation, params) {
                        self.note(Level::Info, format!("host world #{generation}"));
                        self.signal(NoteKind::World);
                    }
                } else if let Some((generation, list)) = world::decode_edits(&data) {
                    self.world.remote_edits(generation, &list);
                } else if let Some((generation, call)) = EntityCall::decode(&data, world::CALL_APPLY_TAG) {
                    if generation == self.world.generation {
                        self.world.record_call(&call);
                    }
                    self.world.incoming_call(generation, call);
                }
            }
            Event::Message { channel: ch, data, .. } if ch == channel::CONTROL || ch == channel::CHAT => match Msg::decode(&data) {
                Some(Msg::Welcome { you, host, players, rules }) => {
                    self.local_id = you;
                    self.host_id = host;
                    self.migration = None;
                    self.welcomed = true;
                    self.players = players.into_iter().map(|p| (p.id, p)).collect();
                    self.rules = rules;
                    let names: Vec<&str> = self.players.values().map(|p| p.name.as_str()).collect();
                    let text = format!("joined as id {you} in {} ms, players: {}", self.started.elapsed().as_millis(), names.join(", "));
                    self.note(Level::Info, text);
                    self.signal(NoteKind::Players);
                    self.signal(NoteKind::Rules);
                }
                Some(Msg::Joined(p)) => {
                    self.note(Level::Info, format!("{} joined", p.name));
                    self.players.insert(p.id, p);
                    self.signal(NoteKind::Players);
                }
                Some(Msg::Left { id, reason }) => {
                    self.tracks.remove(&id);
                    if let Some(p) = self.players.remove(&id) {
                        self.note(Level::Info, format!("{} left: {reason}", p.name));
                        self.signal(NoteKind::Players);
                    }
                }
                Some(Msg::Pings(list)) => {
                    for (id, ms) in list {
                        if let Some(p) = self.players.get_mut(&id) {
                            p.ping_ms = ms;
                        }
                    }
                    self.signal(NoteKind::Players);
                }
                Some(Msg::Rules(r)) => {
                    let changed: Vec<String> = crate::rules::SCHEMA
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| r.at(*i) != self.rules.at(*i))
                        .map(|(i, s)| format!("{} = {}", s.name, r.at(i)))
                        .collect();
                    self.rules = r;
                    if !changed.is_empty() {
                        self.note(Level::Info, format!("rules changed: {}", changed.join(", ")));
                    }
                    self.signal(NoteKind::Rules);
                }
                Some(Msg::Chat { from, flags, text }) => self.chat_note(from, flags, &text),
                Some(Msg::NeedsPool(p)) => self.on_needs_pool(p),
                Some(Msg::Succession(list)) => self.successors = list,
                Some(Msg::HostTransfer { to }) => self.on_host_transfer(to),
                Some(Msg::Renamed { id, name }) => self.apply_rename(id, name),
                Some(Msg::Stash(items)) => {
                    self.mods.stash = items;
                    self.signal(NoteKind::Players);
                }
                Some(Msg::StashGrant { prefab, condition }) => self.mods.grants.push((prefab, condition)),
                Some(Msg::Reply { level, text }) => self.note(Level::from_u8(level), text),
                Some(Msg::Admin { id, admin }) => {
                    if let Some(p) = self.players.get_mut(&id) {
                        p.admin = admin;
                    }
                    if id == self.local_id {
                        self.note(Level::Info, if admin { "you are now an admin" } else { "your admin rights were removed" });
                    }
                    self.signal(NoteKind::Players);
                }
                _ => self.note(Level::Warning, "bad control message"),
            },
            Event::Message { .. } | Event::Request { .. } => {}
            Event::Disconnected { peer, reason } => {
                if Some(peer) == self.host_peer {
                    return self.on_host_lost(&reason);
                }
            }
        }
        true
    }

    fn chat_note(&mut self, from: u16, flags: u8, text: &str) {
        let line = if flags & proto::CHAT_SYSTEM != 0 { format!("* {text}") } else { format!("{}: {text}", self.name_of(from)) };
        self.notes.push(Note { kind: NoteKind::Chat, level: Level::Info, text: line });
    }

    pub(crate) fn system_chat(&mut self, text: &str) {
        let msg = Msg::Chat { from: self.host_id, flags: proto::CHAT_SYSTEM, text: text.to_owned() };
        self.ep.broadcast(channel::CONTROL, &msg.encode(), None);
        self.chat_note(self.host_id, proto::CHAT_SYSTEM, text);
    }

    fn handle_chat(&mut self, from: u16, raw: &str) {
        if let Some(cmd) = raw.strip_prefix('/') {
            let replies = self.exec(from, cmd);
            self.reply(from, replies);
            return;
        }
        if !self.rules.flag(Id::EnableChatbox) && from != self.host_id {
            self.reply(from, vec![(Level::Warning, "chat is disabled on this server".into())]);
            return;
        }
        let now = Instant::now();
        let (tokens, last) = self.chat_budget.entry(from).or_insert((5.0, now));
        *tokens = (*tokens + now.saturating_duration_since(*last).as_secs_f32() * 2.0).min(5.0);
        *last = now;
        if *tokens < 1.0 {
            self.reply(from, vec![(Level::Warning, "you are sending messages too fast".into())]);
            return;
        }
        *tokens -= 1.0;
        let text = proto::sanitize_chat(raw);
        if text.is_empty() {
            return;
        }
        let msg = Msg::Chat { from, flags: 0, text: text.clone() };
        self.ep.broadcast(channel::CONTROL, &msg.encode(), None);
        self.chat_note(from, 0, &text);
    }

    pub fn chat(&mut self, text: &str) {
        match self.role {
            Role::Host => self.handle_chat(self.local_id, text),
            Role::Client => {
                if let Some(cmd) = text.strip_prefix('/') {
                    self.command(cmd);
                } else if let Some(peer) = self.host_peer {
                    let _ = self.ep.send(peer, channel::CONTROL, &Msg::Chat { from: 0, flags: 0, text: text.to_owned() }.encode());
                }
            }
        }
    }

    pub fn command(&mut self, line: &str) {
        match self.role {
            Role::Host => {
                let replies = self.exec(self.local_id, line);
                self.reply(self.local_id, replies);
            }
            Role::Client => {
                let mut parts = line.trim().splitn(2, char::is_whitespace);
                let verb = parts.next().unwrap_or("").to_ascii_lowercase();
                let rest = parts.next().unwrap_or("").trim();
                let Some(peer) = self.host_peer else { return };
                let msg = if verb == "login" { Msg::AdminLogin(rest.to_owned()) } else { Msg::Command(line.trim().to_owned()) };
                let _ = self.ep.send(peer, channel::CONTROL, &msg.encode());
            }
        }
    }

    pub(crate) fn reply(&mut self, to: u16, lines: Vec<(Level, String)>) {
        if to == self.local_id {
            for (level, text) in lines {
                self.note(level, text);
            }
            return;
        }
        let Some(&peer) = self.player_peer.get(&to) else { return };
        for (level, text) in lines {
            let _ = self.ep.send(peer, channel::CONTROL, &Msg::Reply { level: level as u8, text }.encode());
        }
    }

    fn admin_login(&mut self, from: u16, pass: &str) {
        if self.admin_password.is_empty() || pass != self.admin_password {
            let name = self.name_of(from);
            self.note(Level::Warning, format!("{name} failed admin login"));
            self.reply(from, vec![(Level::Warning, "admin login failed".into())]);
            return;
        }
        self.set_admin(from, true);
    }

    pub(crate) fn set_admin(&mut self, id: u16, admin: bool) -> bool {
        let Some(p) = self.players.get_mut(&id) else { return false };
        if p.admin == admin || id == self.host_id {
            return false;
        }
        p.admin = admin;
        let name = p.name.clone();
        self.ep.broadcast(channel::CONTROL, &Msg::Admin { id, admin }.encode(), None);
        self.system_chat(&if admin { format!("{name} is now an admin") } else { format!("{name} is no longer an admin") });
        self.signal(NoteKind::Players);
        true
    }

    pub(crate) fn kick(&mut self, id: u16, reason: &str) -> bool {
        let Some(&peer) = self.player_peer.get(&id) else { return false };
        self.ep.disconnect(peer, reason);
        true
    }

    pub(crate) fn peer_addr(&self, id: u16) -> Option<Addr> {
        self.player_peer.get(&id).and_then(|p| self.ep.addr(*p))
    }

    pub(crate) fn rules_changed(&mut self) {
        self.ep.broadcast(channel::CONTROL, &Msg::Rules(self.rules.clone()).encode(), None);
        if let Some(p) = self.data_path("mp_rules.ini") {
            let _ = std::fs::write(p, self.rules.to_ini());
        }
        self.signal(NoteKind::Rules);
    }

    fn broadcast_pings(&mut self) {
        let mut list = Vec::with_capacity(self.peer_player.len() + 1);
        list.push((self.local_id, 0));
        for (&peer, &id) in &self.peer_player {
            let ms = self.ep.stats(peer).map_or(0, |s| s.rtt_ms.round() as u16);
            if let Some(p) = self.players.get_mut(&id) {
                p.ping_ms = ms;
            }
            list.push((id, ms));
        }
        self.ep.broadcast(channel::CONTROL, &Msg::Pings(list).encode(), None);
        self.signal(NoteKind::Players);
    }

    fn alloc_player(&mut self) -> u16 {
        loop {
            let id = self.next_player;
            self.next_player = self.next_player.wrapping_add(1).max(1);
            if !self.players.contains_key(&id) && !self.pending.values().any(|p| p.id == id) {
                return id;
            }
        }
    }

    pub(crate) fn unique_name(&self, name: &str) -> String {
        let taken = |n: &str| self.players.values().chain(self.pending.values()).any(|p| p.name.eq_ignore_ascii_case(n));
        if !taken(name) {
            return name.to_owned();
        }
        (2..).map(|i| format!("{name} ({i})")).find(|n| !taken(n)).unwrap()
    }

    fn now_ms(&self) -> f64 {
        self.clock.elapsed().as_secs_f64() * 1000.0
    }

    pub fn set_local_state(&mut self, snap: Option<Snapshot>) {
        self.local_state = snap;
    }

    pub fn remote_state(&self, id: u16) -> Option<Snapshot> {
        self.tracks.get(&id)?.sample(self.now_ms())
    }

    fn ingest_pose(&mut self, id: u16, data: &[u8]) -> bool {
        let mut r = Reader::new(data);
        if r.u8() != Some(POSE_TAG) || r.u16().is_none() {
            return false;
        }
        let Some(sent) = r.u32() else { return false };
        let Some(snap) = Snapshot::read(&mut r) else { return false };
        let now = self.now_ms();
        self.tracks.entry(id).or_default().push(now, sent, snap);
        true
    }

    fn send_pose(&mut self) {
        let Some(snap) = self.local_state else { return };
        if !self.welcomed || self.last_state_send.elapsed() < POSE_INTERVAL {
            return;
        }
        self.last_state_send = Instant::now();
        let ms = self.now_ms() as u32;
        self.pose_buf.clear();
        let mut w = Writer::new(&mut self.pose_buf);
        w.u8(POSE_TAG).u16(self.local_id).u32(ms);
        snap.write(&mut w);
        match self.role {
            Role::Host => self.ep.broadcast(channel::POSE, &self.pose_buf, None),
            Role::Client => {
                if let Some(peer) = self.host_peer {
                    let _ = self.ep.send(peer, channel::POSE, &self.pose_buf);
                }
            }
        }
    }

    pub fn world_begin(&mut self, params: Vec<u8>) -> u32 {
        let generation = self.world.begin(params);
        if self.role == Role::Host {
            self.ep.broadcast(channel::STATE, &world::encode_params(generation, &self.world.params), None);
        }
        self.signal(NoteKind::World);
        generation
    }

    pub fn world_ready(&mut self, generation: u32) {
        self.world.mark_ready(generation);
    }

    pub fn tile_edit(&mut self, e: TileEdit) {
        self.world.local_edit(e);
    }

    pub fn world_params(&self) -> (u32, &[u8]) {
        (self.world.generation, &self.world.params)
    }

    pub fn world_rev(&self) -> u32 {
        self.world.rev
    }

    pub fn drain_tiles(&mut self, out: &mut [TileEdit]) -> usize {
        self.world.drain_inbox(out)
    }

    pub fn entity_call(&mut self, call: EntityCall) {
        if self.world.generation == 0 || !self.welcomed {
            return;
        }
        self.send_edits();
        let generation = self.world.generation;
        match self.role {
            Role::Host => {
                self.world.record_call(&call);
                self.ep.broadcast(channel::STATE, &call.encode(world::CALL_APPLY_TAG, generation), None);
            }
            Role::Client => {
                if let Some(peer) = self.host_peer {
                    let _ = self.ep.send(peer, channel::STATE, &call.encode(world::CALL_REQ_TAG, generation));
                }
            }
        }
    }

    fn push_motion(&mut self, data: Vec<u8>) {
        if self.motion_in.len() >= 4096 {
            self.motion_in.pop_front();
        }
        self.motion_in.push_back(data);
    }

    pub fn send_motion(&mut self, payload: &[u8]) {
        if self.world.generation == 0 || !self.welcomed || payload.len() > ct_net::MAX_UNRELIABLE - 8 {
            return;
        }
        let mut msg = Vec::with_capacity(payload.len() + 5);
        msg.push(MOTION_TAG);
        msg.extend_from_slice(&self.world.generation.to_le_bytes());
        msg.extend_from_slice(payload);
        match self.role {
            Role::Host => self.ep.broadcast(channel::POSE, &msg, None),
            Role::Client => {
                if let Some(peer) = self.host_peer {
                    let _ = self.ep.send(peer, channel::POSE, &msg);
                }
            }
        }
    }

    pub fn next_motion(&mut self) -> Option<Vec<u8>> {
        self.motion_in.pop_front()
    }

    pub fn next_entity_call(&mut self) -> Option<EntityCall> {
        self.world.next_call()
    }

    fn send_edits(&mut self) {
        if !self.welcomed {
            return;
        }
        let out = self.world.take_outbox();
        if out.is_empty() {
            return;
        }
        for m in world::encode_edits(self.world.generation, &out) {
            match self.role {
                Role::Host => self.ep.broadcast(channel::STATE, &m, None),
                Role::Client => {
                    if let Some(peer) = self.host_peer {
                        let _ = self.ep.send(peer, channel::STATE, &m);
                    }
                }
            }
        }
    }

    pub fn shutdown(&self, reason: &str) {
        self.ep.disconnect_all(reason);
    }

    pub fn status(&self) -> String {
        let limit = self.rules.get(Id::PLAYER_COUNT_LIMIT);
        let mut s = match self.role {
            Role::Host => format!("hosting, {}/{limit} players", self.players.len()),
            Role::Client if self.welcomed => format!("client id {}, {}/{limit} players", self.local_id, self.players.len()),
            Role::Client => "connecting...".to_owned(),
        };
        for p in self.players.values() {
            let me = if p.id == self.local_id { " (you)" } else { "" };
            let adm = if p.id == self.host_id { " [host]" } else if p.admin { " [admin]" } else { "" };
            s.push_str(&format!("\n  [{}] {}{adm}{me} - {} ms", p.id, p.name, p.ping_ms));
        }
        let peers: Vec<PeerId> = match self.role {
            Role::Host => self.peer_player.keys().copied().collect(),
            Role::Client => self.host_peer.into_iter().collect(),
        };
        let (mut tx, mut rx, mut lost, mut sent) = (0u64, 0u64, 0u64, 0u64);
        for p in peers {
            if let Some(st) = self.ep.stats(p) {
                tx += st.bytes_sent;
                rx += st.bytes_recv;
                lost += st.packets_lost;
                sent += st.packets_sent;
            }
        }
        let loss = if sent > 0 { lost as f64 * 100.0 / sent as f64 } else { 0.0 };
        if self.world.generation != 0 {
            s.push_str(&format!("
  world #{}, {} tile edits{}", self.world.generation, self.world.edit_count(), if self.world.is_ready() { "" } else { ", generating" }));
        }
        s.push_str(&format!("\n  traffic: {} KB out, {} KB in, loss {loss:.1}%", tx / 1024, rx / 1024));
        s
    }
}
