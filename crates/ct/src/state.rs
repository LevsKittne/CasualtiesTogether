use ct_net::wire::{Reader, Writer};
use std::collections::VecDeque;

pub const MAX_LIMBS: usize = 24;
const LIMB_SCALE: f32 = 512.0;
const LOOK_SCALE: f32 = 32.0;
const VEL_SCALE: f32 = 100.0;
const HISTORY: usize = 32;

#[allow(dead_code)]
pub mod flag {
    pub const STANDING: u32 = 1;
    pub const CROUCHING: u32 = 2;
    pub const ALIVE: u32 = 4;
    pub const CONSCIOUS: u32 = 8;
    pub const FACING_RIGHT: u32 = 16;
    pub const GROUNDED: u32 = 32;
    pub const IN_WORLD: u32 = 64;
    pub const CLIMBING: u32 = 128;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LimbPose {
    pub x: f32,
    pub y: f32,
    pub rot: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PlayerState {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub look_x: f32,
    pub look_y: f32,
    pub move_x: f32,
    pub move_y: f32,
    pub flags: u32,
    pub character: u32,
    pub limb_count: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Snapshot {
    pub state: PlayerState,
    pub limbs: [LimbPose; MAX_LIMBS],
}

impl Snapshot {
    pub fn has(&self, f: u32) -> bool {
        self.state.flags & f != 0
    }

    pub fn write(&self, w: &mut Writer) {
        let s = &self.state;
        w.f32(s.x).f32(s.y);
        w.u16(q16(s.vx, VEL_SCALE)).u16(q16(s.vy, VEL_SCALE));
        w.u16(q16(s.look_x - s.x, LOOK_SCALE)).u16(q16(s.look_y - s.y, LOOK_SCALE));
        w.u8(q8(s.move_x)).u8(q8(s.move_y));
        w.u8(s.flags as u8).u8(s.character as u8).u8((s.character >> 8) as u8);
        let n = if self.has(flag::STANDING) { 0 } else { (s.limb_count as usize).min(MAX_LIMBS) };
        w.u8(n as u8);
        for l in &self.limbs[..n] {
            w.u16(q16(l.x - s.x, LIMB_SCALE)).u16(q16(l.y - s.y, LIMB_SCALE)).u16(qangle(l.rot));
        }
    }

    pub fn read(r: &mut Reader) -> Option<Self> {
        let x = r.f32()?;
        let y = r.f32()?;
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        let vx = dq16(r.u16()?, VEL_SCALE);
        let vy = dq16(r.u16()?, VEL_SCALE);
        let look_x = x + dq16(r.u16()?, LOOK_SCALE);
        let look_y = y + dq16(r.u16()?, LOOK_SCALE);
        let move_x = dq8(r.u8()?);
        let move_y = dq8(r.u8()?);
        let flags = r.u8()? as u32;
        let character = r.u8()? as u32 | (r.u8()? as u32) << 8;
        let n = (r.u8()? as usize).min(MAX_LIMBS);
        let mut limbs = [LimbPose::default(); MAX_LIMBS];
        for l in &mut limbs[..n] {
            l.x = x + dq16(r.u16()?, LIMB_SCALE);
            l.y = y + dq16(r.u16()?, LIMB_SCALE);
            l.rot = dqangle(r.u16()?);
        }
        let state = PlayerState { x, y, vx, vy, look_x, look_y, move_x, move_y, flags, character, limb_count: n as u32 };
        Some(Self { state, limbs })
    }

    fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        let (sa, sb) = (&a.state, &b.state);
        let pick = if t < 0.5 { sa } else { sb };
        let mut out = Snapshot {
            state: PlayerState {
                x: lerp(sa.x, sb.x, t),
                y: lerp(sa.y, sb.y, t),
                vx: lerp(sa.vx, sb.vx, t),
                vy: lerp(sa.vy, sb.vy, t),
                look_x: lerp(sa.look_x, sb.look_x, t),
                look_y: lerp(sa.look_y, sb.look_y, t),
                move_x: pick.move_x,
                move_y: pick.move_y,
                flags: pick.flags,
                character: sb.character,
                limb_count: pick.limb_count,
            },
            limbs: [LimbPose::default(); MAX_LIMBS],
        };
        let n = out.state.limb_count as usize;
        let both = sa.limb_count == sb.limb_count;
        let src = if t < 0.5 { a } else { b };
        for i in 0..n {
            out.limbs[i] = if both {
                LimbPose {
                    x: lerp(a.limbs[i].x, b.limbs[i].x, t),
                    y: lerp(a.limbs[i].y, b.limbs[i].y, t),
                    rot: lerp_angle(a.limbs[i].rot, b.limbs[i].rot, t),
                }
            } else {
                src.limbs[i]
            };
        }
        out
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut d = (b - a) % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    a + d * t
}

#[inline]
fn q16(v: f32, scale: f32) -> u16 {
    ((v * scale).round().clamp(-32768.0, 32767.0) as i16) as u16
}

#[inline]
fn dq16(v: u16, scale: f32) -> f32 {
    (v as i16) as f32 / scale
}

#[inline]
fn q8(v: f32) -> u8 {
    ((v.clamp(-1.0, 1.0) * 127.0).round() as i8) as u8
}

#[inline]
fn dq8(v: u8) -> f32 {
    (v as i8) as f32 / 127.0
}

#[inline]
fn qangle(deg: f32) -> u16 {
    (deg.rem_euclid(360.0) / 360.0 * 65536.0).round() as u32 as u16
}

#[inline]
fn dqangle(v: u16) -> f32 {
    v as f32 / 65536.0 * 360.0
}

pub struct Track {
    history: VecDeque<(u32, Snapshot)>,
    offset: Option<f64>,
    last_sample: f64,
}

pub const INTERP_DELAY_MS: f64 = 100.0;
const MAX_EXTRAPOLATE_MS: f64 = 150.0;
const OFFSET_DRIFT_PER_SEC: f64 = 2.0;

impl Default for Track {
    fn default() -> Self {
        Self { history: VecDeque::with_capacity(HISTORY), offset: None, last_sample: 0.0 }
    }
}

impl Track {
    pub fn push(&mut self, now_ms: f64, sender_ms: u32, snap: Snapshot) {
        let sample = now_ms - sender_ms as f64;
        self.offset = Some(match self.offset {
            None => sample,
            Some(o) => {
                let relaxed = o + (now_ms - self.last_sample).max(0.0) / 1000.0 * OFFSET_DRIFT_PER_SEC;
                if (sample - o).abs() > 5000.0 { sample } else { relaxed.min(sample) }
            }
        });
        self.last_sample = now_ms;
        if let Some((last, _)) = self.history.back() {
            let d = sender_ms.wrapping_sub(*last) as i32;
            if d <= 0 && d > -5000 {
                return;
            }
            if d <= -5000 {
                self.history.clear();
            }
        }
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back((sender_ms, snap));
    }

    pub fn latest(&self) -> Option<&Snapshot> {
        self.history.back().map(|(_, s)| s)
    }

    pub fn sample(&self, now_ms: f64) -> Option<Snapshot> {
        let offset = self.offset?;
        let (t_last, last) = self.history.back()?;
        let target = now_ms - offset - INTERP_DELAY_MS;
        let base = *t_last as f64;
        let rel = |t: u32| base + (t.wrapping_sub(*t_last) as i32) as f64;
        if target >= base {
            let ahead = (target - base).min(MAX_EXTRAPOLATE_MS) / 1000.0;
            let mut s = *last;
            s.state.x += s.state.vx * ahead as f32;
            s.state.y += s.state.vy * ahead as f32;
            return Some(s);
        }
        let mut prev: Option<&(u32, Snapshot)> = None;
        for entry in self.history.iter().rev() {
            if rel(entry.0) <= target {
                prev = Some(entry);
                break;
            }
        }
        let Some((t0, s0)) = prev else {
            return self.history.front().map(|(_, s)| *s);
        };
        let idx = self.history.iter().position(|e| e.0 == *t0).unwrap();
        let (t1, s1) = &self.history[idx + 1];
        let (a, b) = (rel(*t0), rel(*t1));
        let t = if b > a { ((target - a) / (b - a)).clamp(0.0, 1.0) as f32 } else { 1.0 };
        Some(Snapshot::lerp(s0, s1, t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(x: f32, standing: bool) -> Snapshot {
        let mut s = Snapshot::default();
        s.state.x = x;
        s.state.y = 10.0;
        s.state.vx = 10.0;
        s.state.look_x = x + 5.0;
        s.state.look_y = 12.0;
        s.state.move_x = 1.0;
        s.state.flags = flag::ALIVE | if standing { flag::STANDING } else { 0 };
        s.state.character = 2;
        if !standing {
            s.state.limb_count = 15;
            for i in 0..15 {
                s.limbs[i] = LimbPose { x: x + i as f32 * 0.1, y: 10.5, rot: 350.0 + i as f32 };
            }
        }
        s
    }

    #[test]
    fn quantization_roundtrip() {
        let s = snap(1234.567, false);
        let mut buf = Vec::new();
        s.write(&mut Writer::new(&mut buf));
        assert!(buf.len() < 120, "{}", buf.len());
        let back = Snapshot::read(&mut Reader::new(&buf)).unwrap();
        assert_eq!(back.state.x, s.state.x);
        assert!((back.state.look_x - s.state.look_x).abs() < 0.05);
        assert!((back.state.vx - 10.0).abs() < 0.01);
        assert_eq!(back.state.limb_count, 15);
        assert!((back.limbs[14].x - s.limbs[14].x).abs() < 0.002);
        let d = (back.limbs[14].rot - 4.0).rem_euclid(360.0);
        assert!(d < 0.01 || d > 359.99, "{}", back.limbs[14].rot);
        let mut b2 = Vec::new();
        snap(0.0, true).write(&mut Writer::new(&mut b2));
        assert!(b2.len() < 25, "{}", b2.len());
    }

    #[test]
    fn interpolates_behind_and_extrapolates() {
        let mut t = Track::default();
        let latency = 40.0;
        for i in 0..20u32 {
            let send = 1000 + i * 33;
            t.push(send as f64 + latency + 500_000.0, send, snap(i as f32, true));
        }
        let now = (1000 + 19 * 33) as f64 + latency + 500_000.0;
        let s = t.sample(now).unwrap();
        let expected = 19.0 - INTERP_DELAY_MS as f32 / 33.0;
        assert!((s.state.x - expected).abs() < 0.05, "{} vs {expected}", s.state.x);
        let later = t.sample(now + 400.0).unwrap();
        assert!((later.state.x - (19.0 + 10.0 * 0.15)).abs() < 0.05, "{}", later.state.x);
    }

    #[test]
    fn angle_wraps() {
        assert!((lerp_angle(350.0, 10.0, 0.5) - 360.0).abs() < 1e-3);
        assert!((lerp_angle(10.0, 350.0, 0.5) - 0.0).abs() < 1e-3);
    }

    #[test]
    fn ignores_old_and_duplicate() {
        let mut t = Track::default();
        t.push(100.0, 100, snap(1.0, true));
        t.push(110.0, 100, snap(2.0, true));
        t.push(120.0, 90, snap(3.0, true));
        assert_eq!(t.history.len(), 1);
        assert_eq!(t.latest().unwrap().state.x, 1.0);
    }
}
