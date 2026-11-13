use crate::rules::Rules;
use ct_net::wire::{Reader, Writer};

pub const VERSION: u16 = 6;
pub const NO_ID: u16 = u16::MAX;
pub const DEFAULT_PORT: u16 = 7790;
pub const HOST_ID: u16 = 0;
pub const MAX_CHAT: usize = 256;

pub struct Hello<'a> {
    pub version: u16,
    pub name: &'a str,
    pub password: &'a str,
    pub prev_id: u16,
    pub resume: bool,
}

impl<'a> Hello<'a> {
    pub fn write(&self, out: &mut Vec<u8>) {
        Writer::new(out).u16(self.version).str(self.name).str(self.password).u16(self.prev_id).u8(self.resume as u8);
    }

    pub fn read(data: &'a [u8]) -> Option<Self> {
        let mut r = Reader::new(data);
        let version = r.u16()?;
        let name = r.str()?;
        let password = r.str()?;
        let prev_id = r.u16().unwrap_or(NO_ID);
        let resume = r.u8().unwrap_or(0) != 0;
        Some(Self { version, name, password, prev_id, resume })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Successor {
    pub id: u16,
    pub name: String,
    pub addr: ct_net::Addr,
}

fn write_addr(w: &mut Writer, a: &ct_net::Addr) {
    match a {
        ct_net::Addr::Udp(std::net::SocketAddr::V4(v4)) => {
            w.u8(0).bytes(&v4.ip().octets()).u16(v4.port());
        }
        ct_net::Addr::Udp(std::net::SocketAddr::V6(v6)) => {
            w.u8(2).bytes(&v6.ip().octets()).u16(v6.port());
        }
        ct_net::Addr::Steam(id) => {
            w.u8(1).u64(*id);
        }
    }
}

fn read_addr(r: &mut Reader) -> Option<ct_net::Addr> {
    Some(match r.u8()? {
        0 => {
            let o = r.bytes(4)?;
            ct_net::Addr::Udp(std::net::SocketAddr::from(([o[0], o[1], o[2], o[3]], r.u16()?)))
        }
        2 => {
            let o: [u8; 16] = r.bytes(16)?.try_into().ok()?;
            ct_net::Addr::Udp(std::net::SocketAddr::from((o, r.u16()?)))
        }
        1 => ct_net::Addr::Steam(r.u64()?),
        _ => return None,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerInfo {
    pub id: u16,
    pub name: String,
    pub ping_ms: u16,
    pub admin: bool,
}

impl PlayerInfo {
    fn write(&self, w: &mut Writer) {
        w.u16(self.id).str(&self.name).u16(self.ping_ms).u8(self.admin as u8);
    }

    fn read(r: &mut Reader) -> Option<Self> {
        Some(Self { id: r.u16()?, name: r.str()?.to_owned(), ping_ms: r.u16()?, admin: r.u8()? != 0 })
    }
}

pub const CHAT_SYSTEM: u8 = 1;

pub enum Msg {
    Welcome { you: u16, host: u16, players: Vec<PlayerInfo>, rules: Rules },
    Succession(Vec<Successor>),
    HostTransfer { to: u16 },
    Renamed { id: u16, name: String },
    Joined(PlayerInfo),
    Left { id: u16, reason: String },
    Pings(Vec<(u16, u16)>),
    Rules(Rules),
    Chat { from: u16, flags: u8, text: String },
    Command(String),
    Reply { level: u8, text: String },
    Admin { id: u16, admin: bool },
    AdminLogin(String),
    NeedsDelta([f32; 3]),
    NeedsPool([f32; 3]),
    StashDeposit { prefab: String, condition: f32 },
    StashTake(u32),
    Stash(Vec<StashItem>),
    StashGrant { prefab: String, condition: f32 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct StashItem {
    pub id: u32,
    pub prefab: String,
    pub condition: f32,
    pub by: String,
}

mod tag {
    pub const WELCOME: u8 = 1;
    pub const JOINED: u8 = 2;
    pub const LEFT: u8 = 3;
    pub const PINGS: u8 = 4;
    pub const RULES: u8 = 5;
    pub const CHAT: u8 = 6;
    pub const COMMAND: u8 = 7;
    pub const REPLY: u8 = 8;
    pub const ADMIN: u8 = 9;
    pub const ADMIN_LOGIN: u8 = 10;
    pub const NEEDS_DELTA: u8 = 11;
    pub const NEEDS_POOL: u8 = 12;
    pub const STASH_DEPOSIT: u8 = 13;
    pub const STASH_TAKE: u8 = 14;
    pub const STASH: u8 = 15;
    pub const STASH_GRANT: u8 = 16;
    pub const SUCCESSION: u8 = 17;
    pub const HOST_TRANSFER: u8 = 18;
    pub const RENAMED: u8 = 19;
}

impl Msg {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64);
        let mut w = Writer::new(&mut out);
        match self {
            Msg::Welcome { you, host, players, rules } => {
                w.u8(tag::WELCOME).u16(*you).u16(*host).varint(players.len() as u64);
                for p in players {
                    p.write(&mut w);
                }
                rules.write(&mut w);
            }
            Msg::Joined(p) => {
                w.u8(tag::JOINED);
                p.write(&mut w);
            }
            Msg::Left { id, reason } => {
                w.u8(tag::LEFT).u16(*id).str(reason);
            }
            Msg::Pings(list) => {
                w.u8(tag::PINGS).varint(list.len() as u64);
                for (id, ms) in list {
                    w.u16(*id).u16(*ms);
                }
            }
            Msg::Rules(r) => {
                w.u8(tag::RULES);
                r.write(&mut w);
            }
            Msg::Chat { from, flags, text } => {
                w.u8(tag::CHAT).u16(*from).u8(*flags).str(text);
            }
            Msg::Command(t) => {
                w.u8(tag::COMMAND).str(t);
            }
            Msg::Reply { level, text } => {
                w.u8(tag::REPLY).u8(*level).str(text);
            }
            Msg::Admin { id, admin } => {
                w.u8(tag::ADMIN).u16(*id).u8(*admin as u8);
            }
            Msg::AdminLogin(p) => {
                w.u8(tag::ADMIN_LOGIN).str(p);
            }
            Msg::NeedsDelta(v) | Msg::NeedsPool(v) => {
                w.u8(if matches!(self, Msg::NeedsDelta(_)) { tag::NEEDS_DELTA } else { tag::NEEDS_POOL });
                for x in v {
                    w.f32(*x);
                }
            }
            Msg::StashDeposit { prefab, condition } => {
                w.u8(tag::STASH_DEPOSIT).str(prefab).f32(*condition);
            }
            Msg::StashTake(id) => {
                w.u8(tag::STASH_TAKE).u32(*id);
            }
            Msg::Stash(items) => {
                w.u8(tag::STASH).varint(items.len() as u64);
                for it in items {
                    w.u32(it.id).str(&it.prefab).f32(it.condition).str(&it.by);
                }
            }
            Msg::StashGrant { prefab, condition } => {
                w.u8(tag::STASH_GRANT).str(prefab).f32(*condition);
            }
            Msg::Succession(list) => {
                w.u8(tag::SUCCESSION).varint(list.len() as u64);
                for s in list {
                    w.u16(s.id).str(&s.name);
                    write_addr(&mut w, &s.addr);
                }
            }
            Msg::HostTransfer { to } => {
                w.u8(tag::HOST_TRANSFER).u16(*to);
            }
            Msg::Renamed { id, name } => {
                w.u8(tag::RENAMED).u16(*id).str(name);
            }
        }
        out
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        let mut r = Reader::new(data);
        Some(match r.u8()? {
            tag::WELCOME => {
                let you = r.u16()?;
                let host = r.u16()?;
                let n = r.varint()? as usize;
                let mut players = Vec::with_capacity(n.min(256));
                for _ in 0..n {
                    players.push(PlayerInfo::read(&mut r)?);
                }
                Msg::Welcome { you, host, players, rules: Rules::read(&mut r)? }
            }
            tag::JOINED => Msg::Joined(PlayerInfo::read(&mut r)?),
            tag::LEFT => Msg::Left { id: r.u16()?, reason: r.str()?.to_owned() },
            tag::PINGS => {
                let n = r.varint()? as usize;
                let mut list = Vec::with_capacity(n.min(256));
                for _ in 0..n {
                    list.push((r.u16()?, r.u16()?));
                }
                Msg::Pings(list)
            }
            tag::RULES => Msg::Rules(Rules::read(&mut r)?),
            tag::CHAT => Msg::Chat { from: r.u16()?, flags: r.u8()?, text: r.str()?.to_owned() },
            tag::COMMAND => Msg::Command(r.str()?.to_owned()),
            tag::REPLY => Msg::Reply { level: r.u8()?, text: r.str()?.to_owned() },
            tag::ADMIN => Msg::Admin { id: r.u16()?, admin: r.u8()? != 0 },
            tag::ADMIN_LOGIN => Msg::AdminLogin(r.str()?.to_owned()),
            tag::NEEDS_DELTA => Msg::NeedsDelta([r.f32()?, r.f32()?, r.f32()?]),
            tag::NEEDS_POOL => Msg::NeedsPool([r.f32()?, r.f32()?, r.f32()?]),
            tag::STASH_DEPOSIT => Msg::StashDeposit { prefab: r.str()?.to_owned(), condition: r.f32()? },
            tag::STASH_TAKE => Msg::StashTake(r.u32()?),
            tag::STASH => {
                let n = r.varint()? as usize;
                let mut items = Vec::with_capacity(n.min(1024));
                for _ in 0..n {
                    items.push(StashItem { id: r.u32()?, prefab: r.str()?.to_owned(), condition: r.f32()?, by: r.str()?.to_owned() });
                }
                Msg::Stash(items)
            }
            tag::STASH_GRANT => Msg::StashGrant { prefab: r.str()?.to_owned(), condition: r.f32()? },
            tag::SUCCESSION => {
                let n = r.varint()? as usize;
                let mut list = Vec::with_capacity(n.min(64));
                for _ in 0..n {
                    list.push(Successor { id: r.u16()?, name: r.str()?.to_owned(), addr: read_addr(&mut r)? });
                }
                Msg::Succession(list)
            }
            tag::HOST_TRANSFER => Msg::HostTransfer { to: r.u16()? },
            tag::RENAMED => Msg::Renamed { id: r.u16()?, name: r.str()?.to_owned() },
            _ => return None,
        })
    }
}

pub fn sanitize_name(name: &str) -> String {
    let s: String = name.chars().filter(|c| !c.is_control() && *c != '<' && *c != '>').take(24).collect();
    let s = s.trim();
    if s.is_empty() { "Player".to_owned() } else { s.to_owned() }
}

pub fn sanitize_chat(text: &str) -> String {
    let s: String = text.chars().filter(|c| !c.is_control()).take(MAX_CHAT).collect();
    s.replace('<', "\u{2039}").replace('>', "\u{203A}").trim().to_owned()
}
