use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use unsafe_libopus as opus;

pub const VOICE_TAG: u8 = 50;
pub const RATE: u32 = 48000;
pub const FRAME: usize = 960;
pub const STREAM_VOICE: u8 = 0;
pub const STREAM_MUSIC: u8 = 1;
pub const STREAM_META: u8 = 2;
const META_TIMEOUT: Duration = Duration::from_millis(2500);
pub const FLAG_IMPAIRED: u8 = 1;
pub const FLAG_DEAD: u8 = 2;
const MAX_PACKET: usize = 1275;
const PREBUFFER: usize = 3;
const MAX_QUEUE: usize = 15;
const VAD_HANGOVER: Duration = Duration::from_millis(350);
const STALE: Duration = Duration::from_secs(10);

pub struct Encoder(*mut opus::OpusEncoder);

unsafe impl Send for Encoder {}

impl Encoder {
    pub fn new(music: bool, bitrate: i32) -> Option<Self> {
        let app = if music { opus::OPUS_APPLICATION_AUDIO } else { opus::OPUS_APPLICATION_VOIP };
        let mut err = 0;
        let st = unsafe { opus::opus_encoder_create(RATE as i32, 1, app, &mut err) };
        if st.is_null() || err != opus::OPUS_OK {
            return None;
        }
        let e = Self(st);
        e.set_bitrate(bitrate);
        unsafe {
            opus::opus_encoder_ctl!(e.0, opus::OPUS_SET_SIGNAL_REQUEST, if music { opus::OPUS_SIGNAL_MUSIC } else { opus::OPUS_SIGNAL_VOICE });
            opus::opus_encoder_ctl!(e.0, opus::OPUS_SET_INBAND_FEC_REQUEST, 1);
            opus::opus_encoder_ctl!(e.0, opus::OPUS_SET_PACKET_LOSS_PERC_REQUEST, 5);
        }
        Some(e)
    }

    pub fn set_bitrate(&self, bitrate: i32) {
        unsafe {
            opus::opus_encoder_ctl!(self.0, opus::OPUS_SET_BITRATE_REQUEST, bitrate);
        }
    }

    pub fn encode(&mut self, pcm: &[f32; FRAME], out: &mut Vec<u8>) -> bool {
        let start = out.len();
        out.resize(start + MAX_PACKET, 0);
        let n = unsafe { opus::opus_encode_float(self.0, pcm.as_ptr(), FRAME as i32, out[start..].as_mut_ptr(), MAX_PACKET as i32) };
        if n <= 0 {
            out.truncate(start);
            return false;
        }
        out.truncate(start + n as usize);
        true
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        unsafe { opus::opus_encoder_destroy(self.0) }
    }
}

pub struct Decoder(*mut opus::OpusDecoder);

unsafe impl Send for Decoder {}

impl Decoder {
    pub fn new() -> Option<Self> {
        let mut err = 0;
        let st = unsafe { opus::opus_decoder_create(RATE as i32, 1, &mut err) };
        if st.is_null() || err != opus::OPUS_OK { None } else { Some(Self(st)) }
    }

    pub fn decode(&mut self, packet: Option<&[u8]>, out: &mut [f32; FRAME]) -> usize {
        let (ptr, len) = match packet {
            Some(p) => (p.as_ptr(), p.len() as i32),
            None => (std::ptr::null(), 0),
        };
        let n = unsafe { opus::opus_decode_float(self.0, ptr, len, out.as_mut_ptr(), FRAME as i32, 0) };
        n.max(0) as usize
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe { opus::opus_decoder_destroy(self.0) }
    }
}

pub fn encode_packet(from: u16, stream: u8, seq: u16, flags: u8, opus: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(opus.len() + 7);
    v.push(VOICE_TAG);
    v.extend_from_slice(&from.to_le_bytes());
    v.push(stream);
    v.extend_from_slice(&seq.to_le_bytes());
    v.push(flags);
    v.extend_from_slice(opus);
    v
}

pub struct Packet<'a> {
    pub from: u16,
    pub stream: u8,
    pub seq: u16,
    pub flags: u8,
    pub opus: &'a [u8],
}

pub fn decode_packet(data: &[u8]) -> Option<Packet<'_>> {
    if data.len() < 8 || data[0] != VOICE_TAG || data[3] > STREAM_META {
        return None;
    }
    Some(Packet { from: u16::from_le_bytes([data[1], data[2]]), stream: data[3], seq: u16::from_le_bytes([data[4], data[5]]), flags: data[6], opus: &data[7..] })
}

pub fn set_from(data: &mut [u8], from: u16) {
    data[1..3].copy_from_slice(&from.to_le_bytes());
}

struct Rx {
    dec: Decoder,
    queue: BTreeMap<u32, (u8, Vec<u8>)>,
    last_ext: Option<u32>,
    next: u32,
    primed: bool,
    pcm: VecDeque<f32>,
    flags: u8,
    level: f32,
    seen: Instant,
    lp: f32,
    phase: f32,
}

impl Rx {
    fn ext(&mut self, seq: u16) -> u32 {
        let e = match self.last_ext {
            None => seq as u32 + 0x10000,
            Some(last) => {
                let diff = seq.wrapping_sub(last as u16) as i16 as i32;
                (last as i64 + diff as i64).max(0) as u32
            }
        };
        if self.last_ext.is_none_or(|l| e > l) {
            self.last_ext = Some(e);
        }
        e
    }

    fn push(&mut self, seq: u16, flags: u8, data: &[u8]) {
        let e = self.ext(seq);
        if self.primed && e < self.next {
            return;
        }
        self.queue.insert(e, (flags, data.to_vec()));
        while self.queue.len() > MAX_QUEUE {
            self.queue.pop_first();
            if let Some((&k, _)) = self.queue.first_key_value() {
                self.next = self.next.max(k);
            }
        }
        self.seen = Instant::now();
    }

    fn step(&mut self, frame: &mut [f32; FRAME]) -> bool {
        if !self.primed {
            if self.queue.len() < PREBUFFER {
                return false;
            }
            self.primed = true;
            self.next = *self.queue.keys().next().unwrap();
        }
        let n = if let Some((flags, data)) = self.queue.remove(&self.next) {
            self.flags = flags;
            self.dec.decode(Some(&data), frame)
        } else if self.queue.keys().next().is_some_and(|&k| k > self.next) {
            self.dec.decode(None, frame)
        } else {
            self.primed = false;
            return false;
        };
        self.next += 1;
        let mut peak = 0f32;
        for s in &frame[..n] {
            peak = peak.max(s.abs());
        }
        self.level = self.level * 0.6 + peak * 0.4;
        self.pcm.extend(&frame[..n]);
        true
    }
}

#[derive(Clone, Copy)]
pub struct Listener {
    pub hearing_loss: f32,
    pub dead: bool,
    pub volume: f32,
    pub music_volume: f32,
    pub hearing_rule: bool,
    pub impaired_rule: bool,
    pub dead_rule: bool,
}

impl Default for Listener {
    fn default() -> Self {
        Self { hearing_loss: 0.0, dead: false, volume: 1.0, music_volume: 0.8, hearing_rule: true, impaired_rule: true, dead_rule: true }
    }
}

#[derive(Default)]
pub struct Mixer {
    streams: HashMap<(u16, u8), Rx>,
    pub listener: Listener,
    pub muted: std::collections::HashSet<u16>,
    meta: HashMap<u16, (String, f64, Instant)>,
}

pub fn encode_meta(from: u16, seq: u16, name: &str, seconds: f64) -> Vec<u8> {
    let mut body = Vec::with_capacity(name.len() + 8);
    body.extend_from_slice(&seconds.to_le_bytes());
    body.extend_from_slice(name.as_bytes());
    encode_packet(from, STREAM_META, seq, 0, &body)
}

pub static MIXER: Mutex<Option<Mixer>> = Mutex::new(None);

pub fn with_mixer<R>(f: impl FnOnce(&mut Mixer) -> R) -> R {
    let mut g = MIXER.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Mixer::default))
}

impl Mixer {
    pub fn push(&mut self, p: &Packet) {
        if p.stream == STREAM_META {
            if p.opus.len() >= 8 {
                let secs = f64::from_le_bytes(p.opus[..8].try_into().unwrap());
                let name = String::from_utf8_lossy(&p.opus[8..]).chars().take(128).collect();
                if secs.is_finite() {
                    self.meta.insert(p.from, (name, secs, Instant::now()));
                }
            }
            return;
        }
        let key = (p.from, p.stream);
        if !self.streams.contains_key(&key) {
            let Some(dec) = Decoder::new() else { return };
            self.streams.insert(key, Rx { dec, queue: BTreeMap::new(), last_ext: None, next: 0, primed: false, pcm: VecDeque::new(), flags: 0, level: 0.0, seen: Instant::now(), lp: 0.0, phase: 0.0 });
        }
        self.streams.get_mut(&key).unwrap().push(p.seq, p.flags, p.opus);
    }

    pub fn read(&mut self, from: u16, stream: u8, out: &mut [f32]) -> f32 {
        let l = self.listener;
        let muted = self.muted.contains(&from);
        let Some(rx) = self.streams.get_mut(&(from, stream)) else {
            out.fill(0.0);
            return 0.0;
        };
        let mut frame = [0f32; FRAME];
        while rx.pcm.len() < out.len() {
            if !rx.step(&mut frame) {
                break;
            }
        }
        let have = rx.pcm.len().min(out.len());
        for (o, s) in out.iter_mut().zip(rx.pcm.drain(..have)) {
            *o = s;
        }
        out[have..].fill(0.0);
        if rx.pcm.len() > FRAME * 6 {
            let extra = rx.pcm.len() - FRAME * 3;
            rx.pcm.drain(..extra);
        }
        let speaker_dead = rx.flags & FLAG_DEAD != 0;
        let silent = muted || (stream == STREAM_VOICE && speaker_dead && !l.dead && !l.dead_rule);
        let gain = if stream == STREAM_MUSIC { l.music_volume } else { l.volume };
        if silent || gain <= 0.0 {
            out.fill(0.0);
            return 0.0;
        }
        if stream == STREAM_VOICE && rx.flags & FLAG_IMPAIRED != 0 && l.impaired_rule {
            for s in out[..have].iter_mut() {
                rx.phase += 2.0 * std::f32::consts::PI * 7.0 / RATE as f32;
                if rx.phase > 2.0 * std::f32::consts::PI {
                    rx.phase -= 2.0 * std::f32::consts::PI;
                }
                let wobble = 0.55 + 0.45 * rx.phase.sin();
                *s = ((*s * wobble) * 10.0).round() / 10.0;
            }
        }
        let loss = if l.hearing_rule { l.hearing_loss.clamp(0.0, 1.0) } else { 0.0 };
        let alpha = 1.0 - loss * 0.93;
        let g = gain * (1.0 - loss * 0.6);
        let mut peak = 0f32;
        for s in out[..have].iter_mut() {
            rx.lp += alpha * (*s - rx.lp);
            *s = (rx.lp * g).clamp(-1.0, 1.0);
            peak = peak.max(s.abs());
        }
        peak
    }

    pub fn meta(&self, from: u16) -> Option<(String, f64)> {
        let (name, secs, at) = self.meta.get(&from)?;
        if self.muted.contains(&from) || at.elapsed() > META_TIMEOUT {
            return None;
        }
        Some((name.clone(), secs + at.elapsed().as_secs_f64()))
    }

    pub fn level(&self, from: u16, stream: u8) -> f32 {
        self.streams.get(&(from, stream)).map_or(0.0, |r| if r.seen.elapsed() < Duration::from_millis(300) { r.level } else { 0.0 })
    }

    pub fn active(&self) -> Vec<(u16, u8)> {
        self.streams.iter().filter(|(_, r)| r.seen.elapsed() < Duration::from_secs(1)).map(|(k, _)| *k).collect()
    }

    pub fn prune(&mut self) {
        self.streams.retain(|_, r| r.seen.elapsed() < STALE);
        self.meta.retain(|_, m| m.2.elapsed() < STALE);
    }

    pub fn clear(&mut self) {
        self.streams.clear();
        self.meta.clear();
    }
}

pub struct Resampler {
    from: u32,
    pos: f64,
    last: f32,
}

impl Resampler {
    pub fn new() -> Self {
        Self { from: RATE, pos: 0.0, last: 0.0 }
    }

    pub fn push(&mut self, rate: u32, input: &[f32], out: &mut Vec<f32>) {
        if rate == RATE || rate == 0 {
            out.extend_from_slice(input);
            self.from = RATE;
            return;
        }
        if rate != self.from {
            self.from = rate;
            self.pos = 0.0;
        }
        let step = rate as f64 / RATE as f64;
        let n = input.len() as f64;
        let at = |i: isize| if i < 0 { self.last } else { input[i as usize] };
        while self.pos < n - 1.0 {
            let i = self.pos.floor() as isize;
            let t = (self.pos - i as f64) as f32;
            let (a, b) = (at(i), at(i + 1));
            out.push(a + (b - a) * t);
            self.pos += step;
        }
        self.pos -= n;
        if let Some(&l) = input.last() {
            self.last = l;
        }
    }
}

pub struct Tx {
    enc: Encoder,
    resampler: Resampler,
    acc: Vec<f32>,
    seq: u16,
    bitrate: i32,
    vad_until: Option<Instant>,
    pub level: f32,
}

pub struct Capture<'a> {
    pub rate: u32,
    pub samples: &'a [f32],
    pub gain: f32,
    pub open: bool,
    pub threshold: f32,
    pub flags: u8,
}

impl Tx {
    pub fn new(music: bool, bitrate: i32) -> Option<Self> {
        Some(Self { enc: Encoder::new(music, bitrate)?, resampler: Resampler::new(), acc: Vec::with_capacity(FRAME * 4), seq: 0, bitrate, vad_until: None, level: 0.0 })
    }

    pub fn set_bitrate(&mut self, bitrate: i32) {
        if bitrate != self.bitrate {
            self.bitrate = bitrate;
            self.enc.set_bitrate(bitrate);
        }
    }

    pub fn feed(&mut self, c: Capture, from: u16, stream: u8, out: &mut Vec<Vec<u8>>) {
        self.resampler.push(c.rate, c.samples, &mut self.acc);
        let mut frame = [0f32; FRAME];
        let mut used = 0;
        while self.acc.len() - used >= FRAME {
            let mut peak = 0f32;
            for (d, s) in frame.iter_mut().zip(&self.acc[used..used + FRAME]) {
                *d = (s * c.gain).clamp(-1.0, 1.0);
                peak = peak.max(d.abs());
            }
            used += FRAME;
            self.level = self.level * 0.5 + peak * 0.5;
            let now = Instant::now();
            if c.open {
                if peak >= c.threshold {
                    self.vad_until = Some(now + VAD_HANGOVER);
                }
                if self.vad_until.is_none_or(|t| now > t) {
                    continue;
                }
            }
            let mut buf = Vec::with_capacity(160);
            if self.enc.encode(&frame, &mut buf) {
                self.seq = self.seq.wrapping_add(1);
                out.push(encode_packet(from, stream, self.seq, c.flags, &buf));
            }
        }
        self.acc.drain(..used);
    }

    pub fn reset(&mut self) {
        self.acc.clear();
        self.vad_until = None;
        self.level = 0.0;
    }
}

#[derive(Clone)]
pub struct Settings {
    pub mode: u8,
    pub mic: String,
    pub gain: f32,
    pub threshold: f32,
    pub volume: f32,
    pub music_volume: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self { mode: 1, mic: String::new(), gain: 1.0, threshold: 0.04, volume: 1.0, music_volume: 0.8 }
    }
}

type Stored = (Settings, Option<std::path::PathBuf>);

static SETTINGS: Mutex<Option<Stored>> = Mutex::new(None);
pub static DEVICES: Mutex<Vec<String>> = Mutex::new(Vec::new());

pub fn load_settings(dir: Option<std::path::PathBuf>) {
    let path = dir.map(|d| d.join("ct_voice.ini"));
    let mut s = Settings::default();
    if let Some(text) = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let f = v.parse::<f32>().ok().filter(|f| f.is_finite());
            match k {
                "mode" => s.mode = v.parse().unwrap_or(1u8).min(2),
                "mic" => s.mic = v.to_owned(),
                "gain" => s.gain = f.unwrap_or(1.0).clamp(0.0, 4.0),
                "threshold" => s.threshold = f.unwrap_or(0.04).clamp(0.0, 1.0),
                "volume" => s.volume = f.unwrap_or(1.0).clamp(0.0, 2.0),
                "music_volume" => s.music_volume = f.unwrap_or(0.8).clamp(0.0, 2.0),
                _ => {}
            }
        }
    }
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Some((s, path));
}

pub fn settings() -> Settings {
    SETTINGS.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| s.0.clone()).unwrap_or_default()
}

pub fn update_settings(f: impl FnOnce(&mut Settings)) {
    let mut g = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    let (s, path) = g.get_or_insert_with(|| (Settings::default(), None));
    f(s);
    if let Some(p) = path {
        let text = format!(
            "mode = {}\r\nmic = {}\r\ngain = {}\r\nthreshold = {}\r\nvolume = {}\r\nmusic_volume = {}\r\n",
            s.mode, s.mic, s.gain, s.threshold, s.volume, s.music_volume
        );
        let _ = std::fs::write(p, text);
    }
}

pub fn bitrate_for_quality(q: u8) -> i32 {
    [8000, 12000, 16000, 24000, 32000][q.min(4) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(n: usize, start: usize) -> Vec<f32> {
        (start..start + n).map(|i| (i as f32 * 2.0 * std::f32::consts::PI * 440.0 / RATE as f32).sin() * 0.5).collect()
    }

    #[test]
    fn encode_decode_with_loss_and_reorder() {
        let mut tx = Tx::new(false, 24000).unwrap();
        let mut pkts = Vec::new();
        tx.feed(Capture { rate: RATE, samples: &tone(FRAME * 20, 0), gain: 1.0, open: false, threshold: 0.0, flags: FLAG_IMPAIRED }, 3, STREAM_VOICE, &mut pkts);
        assert_eq!(pkts.len(), 20);
        assert!(pkts.iter().all(|p| p.len() < 200));
        let mut m = Mixer::default();
        m.listener.impaired_rule = false;
        pkts.swap(4, 5);
        pkts.remove(9);
        for p in &pkts {
            let p = decode_packet(p).unwrap();
            assert_eq!((p.from, p.stream), (3, STREAM_VOICE));
            m.push(&p);
        }
        let mut out = vec![0f32; FRAME * 18];
        let peak = m.read(3, STREAM_VOICE, &mut out);
        assert!(peak > 0.3, "peak {peak}");
        let energy: f32 = out[FRAME * 2..].iter().map(|s| s * s).sum::<f32>() / (out.len() - FRAME * 2) as f32;
        assert!(energy > 0.05, "energy {energy}");
        assert!(m.level(3, STREAM_VOICE) > 0.1);
    }

    #[test]
    fn open_mic_gates_silence_and_resamples() {
        let mut tx = Tx::new(false, 16000).unwrap();
        let mut pkts = Vec::new();
        let silence = vec![0.001f32; 44100];
        tx.feed(Capture { rate: 44100, samples: &silence, gain: 1.0, open: true, threshold: 0.05, flags: 0 }, 1, STREAM_VOICE, &mut pkts);
        assert!(pkts.is_empty());
        let loud: Vec<f32> = (0..44100).map(|i| (i as f32 * 0.06).sin() * 0.4).collect();
        tx.feed(Capture { rate: 44100, samples: &loud, gain: 1.0, open: true, threshold: 0.05, flags: 0 }, 1, STREAM_VOICE, &mut pkts);
        assert!((48..=51).contains(&pkts.len()), "{}", pkts.len());
    }

    #[test]
    fn dead_speaker_and_mute() {
        let mut tx = Tx::new(false, 16000).unwrap();
        let mut pkts = Vec::new();
        tx.feed(Capture { rate: RATE, samples: &tone(FRAME * 6, 0), gain: 1.0, open: false, threshold: 0.0, flags: FLAG_DEAD }, 2, STREAM_VOICE, &mut pkts);
        let mut m = Mixer::default();
        m.listener.dead_rule = false;
        for p in &pkts {
            m.push(&decode_packet(p).unwrap());
        }
        let mut out = vec![0f32; FRAME * 4];
        assert_eq!(m.read(2, STREAM_VOICE, &mut out), 0.0);
        m.listener.dead = true;
        assert!(m.read(2, STREAM_VOICE, &mut out[..FRAME]) > 0.0);
        m.muted.insert(2);
        assert_eq!(m.read(2, STREAM_VOICE, &mut out[..FRAME]), 0.0);
    }
}
