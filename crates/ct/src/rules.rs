use ct_net::wire::{Reader, Writer};
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    U8(u8),
    U16(u16),
    F32(f32),
}

impl Value {
    pub fn as_bool(self) -> bool {
        match self {
            Value::Bool(b) => b,
            Value::U8(v) => v != 0,
            Value::U16(v) => v != 0,
            Value::F32(v) => v != 0.0,
        }
    }

    pub fn as_f32(self) -> f32 {
        match self {
            Value::Bool(b) => b as u8 as f32,
            Value::U8(v) => v as f32,
            Value::U16(v) => v as f32,
            Value::F32(v) => v,
        }
    }

    fn tag(self) -> u8 {
        match self {
            Value::Bool(_) => 0,
            Value::U8(_) => 1,
            Value::U16(_) => 2,
            Value::F32(_) => 3,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{}", *b as u8),
            Value::U8(v) => write!(f, "{v}"),
            Value::U16(v) => write!(f, "{v}"),
            Value::F32(v) => write!(f, "{v}"),
        }
    }
}

pub struct Rule {
    pub name: &'static str,
    pub default: Value,
    pub min: f32,
    pub max: f32,
    pub unit: &'static str,
}

macro_rules! schema {
    ($($name:ident : $kind:ident = $def:expr $(, $min:expr => $max:expr $(, $unit:literal)?)? ;)*) => {
        pub const SCHEMA: &[Rule] = &[$(
            Rule {
                name: stringify!($name),
                default: schema!(@v $kind $def),
                min: schema!(@min $kind $($min)?),
                max: schema!(@max $kind $($max)?),
                unit: schema!(@unit $($($unit)?)?),
            },
        )*];

        #[allow(non_camel_case_types, dead_code, clippy::upper_case_acronyms)]
        #[repr(usize)]
        pub enum Id { $($name,)* }
    };
    (@v Bool $d:expr) => { Value::Bool($d) };
    (@v U8 $d:expr) => { Value::U8($d) };
    (@v U16 $d:expr) => { Value::U16($d) };
    (@v F32 $d:expr) => { Value::F32($d) };
    (@min Bool) => { 0.0 };
    (@min U8) => { 0.0 };
    (@min U16) => { 0.0 };
    (@min F32) => { 0.0 };
    (@min $k:ident $m:expr) => { $m as f32 };
    (@max Bool) => { 1.0 };
    (@max U8) => { 255.0 };
    (@max U16) => { 65535.0 };
    (@max F32) => { 1000.0 };
    (@max $k:ident $m:expr) => { $m as f32 };
    (@unit) => { "" };
    (@unit $u:literal) => { $u };
}

schema! {
    sv_cheats: Bool = false;
    AllowClientCheatCommands: Bool = false;
    PLAYER_COUNT_LIMIT: U8 = 6, 2 => 64;
    ShowPlayerDirections: Bool = true;
    EnableNametags: Bool = true;
    EnableStatusIcons: Bool = true;
    UnchippedHideNametags: Bool = true;
    EnableChatbox: Bool = true;
    OnlyProximityChat: Bool = false;
    UnchippedProximityChat: Bool = true;
    UnchippedIsIndividual: Bool = true;
    ConsentForFeeding: Bool = true;
    ScatterMinGroupSize: U8 = 51, 0 => 255;
    ScatterPunishDistance: F32 = 80.0, 0 => 1500, "m";
    LayerFinishPlrPercent: U8 = 60, 0 => 101, "%";
    LayerFinishKeepXOffset: Bool = true;
    StragglerRadlinePercent: U8 = 30, 0 => 255, "%";
    RadlineExtraDamage: Bool = true;
    NoInventoryLock: Bool = false;
    EnableSleep: Bool = false;
    EnableTimeManipulation: Bool = false;
    SpeechImpairedChat: Bool = true;
    HearingLossChat: Bool = true;
    SpeechImpairedVC: Bool = true;
    HearingLossVC: Bool = true;
    MindwipeDisablesChat: Bool = true;
    DeadTextchat: Bool = true;
    DeadVoicechat: Bool = true;
    SleepingMute: Bool = false;
    TrackPlayerDeaths: Bool = true;
    Permadeath: Bool = false;
    ReviveOnNextLevel: Bool = false;
    ReviveFromTrader: Bool = true;
    RespawnKeepInventory: Bool = false;
    RespawnKeepSkills: Bool = false;
    AllowSpectatorFreecam: Bool = true;
    AllowPush: Bool = true;
    AlwaysAllowCarry: Bool = false;
    PiggybackMaxStack: U8 = 1, 0 => 254;
    PiggybackWeightMultiplier: F32 = 0.8, 0 => 2;
    MinigameMaxPlayers: U8 = 50, 1 => 50;
    SpectateWhileUnconscious: Bool = false;
    EnableMP3Sync: Bool = true;
    VoicechatQuality: U8 = 4, 0 => 4;
    VoicechatEnabled: Bool = true;
    ProximityHearDistance: F32 = 55.0, 0 => 1500, "m";
    CharacterYapPublic: Bool = true;
    Teams: Bool = false;
    PVP: Bool = false;
    PVPCombatDismember: Bool = true;
    PVPMoodDebuff: F32 = 0.5, 0 => 10;
    PVPDamageMultiplier: F32 = 1.0, 0 => 10, "x";
    LateJoinAllowed: Bool = true;
    LateJoinSpectate: Bool = false;
    SpawnOnPlayer: Bool = true;
    AmputateHealthyPlayers: Bool = true;
    AdditionalBrainRegen: F32 = 1.0, 0 => 10, "x";
    AdditionalHealthRegen: F32 = 1.0, 0 => 10, "x";
    AdditionalHealthDecay: F32 = 1.0, 0 => 10, "x";
    LastStandAllowed: Bool = true;
    SelfharmWitnessMoodDebuff: F32 = 3.0, 0 => 10;
    SavePlayerState: Bool = true;
    SavePlayerInventory: Bool = true;
    SavePlayerPosition: Bool = true;
    AutoContinue: Bool = false;
    AutoMinPlrsToStart: U16 = 2, 1 => 64;
    AutoExitWhenAllDied: Bool = true;
    AutoExitWhenAllLeft: Bool = true;
    SharedStats: Bool = false;
    SharedStash: Bool = false;
    FriendlyFire: Bool = false;
    OneHit: Bool = false;
    Collision: Bool = true;
    SoulLink: Bool = false;
    GravityMultiplier: F32 = 1.0, 0.1 => 3, "x";
}

pub fn find(name: &str) -> Option<usize> {
    SCHEMA.iter().position(|r| r.name.eq_ignore_ascii_case(name))
}

pub fn parse(idx: usize, s: &str) -> Result<Value, String> {
    let rule = &SCHEMA[idx];
    let s = s.trim();
    if let Value::Bool(_) = rule.default {
        return match s.to_ascii_lowercase().as_str() {
            "1" | "true" | "on" | "yes" => Ok(Value::Bool(true)),
            "0" | "false" | "off" | "no" => Ok(Value::Bool(false)),
            _ => Err(format!("{} expects 0/1", rule.name)),
        };
    }
    let v: f32 = s.parse().map_err(|_| format!("{} expects a number", rule.name))?;
    if !v.is_finite() || v < rule.min || v > rule.max {
        return Err(format!("{} must be in {}..{}{}", rule.name, rule.min, rule.max, rule.unit));
    }
    Ok(match rule.default {
        Value::U8(_) => Value::U8(v.round() as u8),
        Value::U16(_) => Value::U16(v.round() as u16),
        _ => Value::F32(v),
    })
}

#[derive(Clone, PartialEq)]
pub struct Rules {
    values: Vec<Value>,
}

impl Default for Rules {
    fn default() -> Self {
        Self { values: SCHEMA.iter().map(|r| r.default).collect() }
    }
}

impl Rules {
    pub fn get(&self, id: Id) -> Value {
        self.values[id as usize]
    }

    pub fn flag(&self, id: Id) -> bool {
        self.get(id).as_bool()
    }

    pub fn at(&self, idx: usize) -> Value {
        self.values[idx]
    }

    pub fn set(&mut self, idx: usize, v: Value) -> bool {
        if self.values[idx] == v {
            return false;
        }
        self.values[idx] = v;
        true
    }

    pub fn set_str(&mut self, name: &str, value: &str) -> Result<bool, String> {
        let idx = find(name).ok_or_else(|| format!("unknown rule {name}"))?;
        let v = parse(idx, value)?;
        Ok(self.set(idx, v))
    }

    pub fn write(&self, w: &mut Writer) {
        w.varint(self.values.len() as u64);
        for v in &self.values {
            w.u8(v.tag());
            match *v {
                Value::Bool(b) => w.u8(b as u8),
                Value::U8(x) => w.u8(x),
                Value::U16(x) => w.u16(x),
                Value::F32(x) => w.f32(x),
            };
        }
    }

    pub fn read(r: &mut Reader) -> Option<Self> {
        let n = r.varint()? as usize;
        let mut out = Self::default();
        for i in 0..n {
            let v = match r.u8()? {
                0 => Value::Bool(r.u8()? != 0),
                1 => Value::U8(r.u8()?),
                2 => Value::U16(r.u16()?),
                3 => Value::F32(r.f32()?),
                _ => return None,
            };
            if i < out.values.len() && v.tag() == out.values[i].tag() {
                out.values[i] = v;
            }
        }
        Some(out)
    }

    pub fn diff(&self) -> Vec<(usize, Value)> {
        self.values.iter().enumerate().filter(|(i, v)| **v != SCHEMA[*i].default).map(|(i, v)| (i, *v)).collect()
    }

    pub fn to_ini(&self) -> String {
        let mut s = String::new();
        for (i, r) in SCHEMA.iter().enumerate() {
            s.push_str(&format!("{} = {}\r\n", r.name, self.values[i]));
        }
        s
    }

    pub fn apply_ini<'a>(&mut self, lines: impl Iterator<Item = &'a str>) {
        for line in lines {
            if let Some((k, v)) = line.split_once('=') {
                let _ = self.set_str(k.trim(), v.trim());
            }
        }
    }
}

pub struct Presets {
    pub map: BTreeMap<String, String>,
}

impl Presets {
    pub fn load(path: &Path) -> Self {
        let mut map = BTreeMap::new();
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let mut cur: Option<(String, String)> = None;
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('[') && t.ends_with(']') {
                if let Some((k, v)) = cur.take() {
                    map.insert(k, v);
                }
                cur = Some((t[1..t.len() - 1].to_owned(), String::new()));
            } else if let Some((_, body)) = cur.as_mut() {
                if !t.is_empty() {
                    body.push_str(t);
                    body.push('\n');
                }
            }
        }
        if let Some((k, v)) = cur {
            map.insert(k, v);
        }
        Self { map }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut s = String::new();
        for (k, v) in &self.map {
            s.push_str(&format!("[{k}]\r\n{}\r\n", v.replace('\n', "\r\n")));
        }
        std::fs::write(path, s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_parse() {
        let mut r = Rules::default();
        assert!(r.set_str("pvp", "1").unwrap());
        assert!(r.set_str("ProximityHearDistance", "120").unwrap());
        assert!(r.set_str("PLAYER_COUNT_LIMIT", "100").is_err());
        assert!(r.set_str("nope", "1").is_err());
        let mut buf = Vec::new();
        r.write(&mut Writer::new(&mut buf));
        let back = Rules::read(&mut Reader::new(&buf)).unwrap();
        assert!(back == r);
        assert!(back.flag(Id::PVP));
        let mut c = Rules::default();
        c.apply_ini(r.to_ini().lines());
        assert!(c == r);
        assert_eq!(r.diff().len(), 2);
    }
}
