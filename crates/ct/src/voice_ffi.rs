use crate::overlay::binds::{self, Bind};
use crate::rules::Id;
use crate::voice::{self, Capture, FLAG_DEAD, STREAM_MUSIC, STREAM_VOICE, Tx};
use crate::{g, guard};
use std::sync::atomic::Ordering;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

pub const FLAG_SLEEPING: u8 = 4;
pub const FLAG_FORCE: u8 = 0x80;
static LAST_SENT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn now_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64 + 1
}

pub fn transmitting() -> bool {
    let t = LAST_SENT.load(Ordering::Relaxed);
    t != 0 && now_ms().saturating_sub(t) < 250
}
const MUSIC_BITRATE: i32 = 64000;

fn ptt_held() -> bool {
    let vk = binds::key(Bind::Ptt);
    if vk == 0 || crate::overlay::INPUT_BLOCKED.load(Ordering::Relaxed) || crate::overlay::WANTS_KEYBOARD.load(Ordering::Relaxed) {
        return false;
    }
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(GetForegroundWindow(), Some(&mut pid));
        pid == GetCurrentProcessId() && GetAsyncKeyState(vk as i32) as u16 & 0x8000 != 0
    }
}

unsafe fn slice<'a>(p: *const f32, n: i32) -> &'a [f32] {
    if p.is_null() || n <= 0 { &[] } else { unsafe { std::slice::from_raw_parts(p, n as usize) } }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_voice_capture(samples: *const f32, n: i32, rate: i32, flags: u8) -> i32 {
    let input = unsafe { slice(samples, n) };
    guard(0, || {
        let cfg = voice::settings();
        let mut g = g();
        let g = &mut *g;
        let Some(s) = g.session.as_mut() else { return 0 };
        let rules = s.rules();
        let blocked = !rules.flag(Id::VoicechatEnabled)
            || (flags & FLAG_DEAD != 0 && !rules.flag(Id::DeadVoicechat))
            || (flags & FLAG_SLEEPING != 0 && rules.flag(Id::SleepingMute));
        let bitrate = voice::bitrate_for_quality(rules.get(Id::VoicechatQuality).as_f32() as u8);
        let open = cfg.mode == 2;
        let talking = !blocked && (flags & FLAG_FORCE != 0 || open || (cfg.mode == 1 && ptt_held()));
        if g.voice_tx.is_none() {
            g.voice_tx = Tx::new(false, bitrate);
        }
        let Some(tx) = g.voice_tx.as_mut() else { return 0 };
        if !talking {
            tx.reset();
            tx.level = input.iter().fold(0f32, |m, s| m.max((s * cfg.gain).abs()));
            return 0;
        }
        tx.set_bitrate(bitrate);
        let mut out = Vec::new();
        let flags = flags & (voice::FLAG_IMPAIRED | FLAG_DEAD);
        tx.feed(Capture { rate: rate as u32, samples: input, gain: cfg.gain, open: open && flags & FLAG_FORCE == 0, threshold: cfg.threshold, flags }, s.local_id(), STREAM_VOICE, &mut out);
        let sent = !out.is_empty();
        if sent {
            LAST_SENT.store(now_ms(), Ordering::Relaxed);
        }
        s.send_voice(&out);
        sent as i32
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_music_capture(samples: *const f32, frames: i32, channels: i32, rate: i32) -> i32 {
    let ch = channels.max(1) as usize;
    let input = unsafe { slice(samples, frames.max(0) * ch as i32) };
    guard(0, || {
        let mut g = g();
        let g = &mut *g;
        let Some(s) = g.session.as_mut() else { return 0 };
        if !s.rules().flag(Id::EnableMP3Sync) {
            return 0;
        }
        if g.music_tx.is_none() {
            g.music_tx = Tx::new(true, MUSIC_BITRATE);
        }
        let Some(tx) = g.music_tx.as_mut() else { return 0 };
        let mono: Vec<f32> = input.chunks_exact(ch).map(|c| c.iter().sum::<f32>() / ch as f32).collect();
        let mut out = Vec::new();
        tx.feed(Capture { rate: rate as u32, samples: &mono, gain: 1.0, open: false, threshold: 0.0, flags: 0 }, s.local_id(), STREAM_MUSIC, &mut out);
        s.send_voice(&out);
        out.len() as i32
    })
}

static META_SEQ: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_music_meta_send(name: *const u8, len: i32, seconds: f64) {
    let name = unsafe { crate::text(name, len) };
    guard((), || {
        let mut g = g();
        let Some(s) = g.session.as_mut() else { return };
        let seq = META_SEQ.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let pkt = voice::encode_meta(s.local_id(), seq, &name, seconds);
        s.send_voice(&[pkt]);
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_music_meta(from: u16, buf: *mut u8, cap: i32, seconds: *mut f64, volume: *mut f32) -> i32 {
    guard(-1, || {
        let Some((name, secs)) = voice::with_mixer(|m| m.meta(from)) else { return -1 };
        unsafe {
            if !seconds.is_null() {
                *seconds = secs;
            }
            if !volume.is_null() {
                *volume = voice::settings().music_volume;
            }
            crate::write_out(name.as_bytes(), buf, cap)
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_music_stop() {
    guard((), || {
        if let Some(tx) = g().music_tx.as_mut() {
            tx.reset();
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_voice_read(from: u16, stream: u8, out: *mut f32, n: i32) -> f32 {
    if out.is_null() || n <= 0 {
        return 0.0;
    }
    let buf = unsafe { std::slice::from_raw_parts_mut(out, n as usize) };
    guard(0.0, || voice::with_mixer(|m| m.read(from, stream, buf)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_voice_level(from: u16, stream: u8) -> f32 {
    guard(0.0, || voice::with_mixer(|m| m.level(from, stream)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_voice_listener(hearing_loss: f32, dead: i32) -> f32 {
    guard(0.0, || {
        let cfg = voice::settings();
        let g = g();
        let Some(s) = g.session.as_ref() else { return 0.0 };
        let r = s.rules();
        let l = voice::Listener {
            hearing_loss: if hearing_loss.is_finite() { hearing_loss } else { 0.0 },
            dead: dead != 0,
            volume: cfg.volume,
            music_volume: cfg.music_volume,
            hearing_rule: r.flag(Id::HearingLossVC),
            impaired_rule: r.flag(Id::SpeechImpairedVC),
            dead_rule: r.flag(Id::DeadVoicechat),
        };
        let range = s.hear_range();
        drop(g);
        voice::with_mixer(|m| {
            m.listener = l;
            m.prune();
        });
        range
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_voice_devices(text: *const u8, len: i32) {
    let list = unsafe { crate::text(text, len) };
    guard((), || {
        *voice::DEVICES.lock().unwrap_or_else(|e| e.into_inner()) = list.split('\t').filter(|s| !s.is_empty()).map(str::to_owned).collect();
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ct_voice_mic(buf: *mut u8, cap: i32) -> i32 {
    guard(-1, || {
        let s = voice::settings();
        if s.mode == 0 {
            return -1;
        }
        unsafe { crate::write_out(s.mic.as_bytes(), buf, cap) }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_voice_mic_level() -> f32 {
    guard(0.0, || g().voice_tx.as_ref().map_or(0.0, |t| t.level))
}

#[unsafe(no_mangle)]
pub extern "C" fn ct_voice_toggle_mute(id: u16) {
    guard((), || {
        voice::with_mixer(|m| {
            if !m.muted.remove(&id) {
                m.muted.insert(id);
            }
        })
    })
}
