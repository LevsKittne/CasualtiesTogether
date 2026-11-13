use crate::packet::*;
use crate::wire::{Reader, Writer};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const FRAG_SIZE: usize = 1024;
const SENT_SLOTS: usize = 1024;
const RELIABLE_WINDOW: u16 = 4096;
const COMPRESS_MIN: usize = 96;
const MAX_MESSAGE: usize = 64 << 20;
const KEEPALIVE: Duration = Duration::from_millis(100);
const ACK_DELAY: Duration = Duration::from_millis(4);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    TooLarge,
    WindowFull,
    BadChannel,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Stats {
    pub rtt_ms: f32,
    pub packets_sent: u64,
    pub packets_recv: u64,
    pub bytes_sent: u64,
    pub bytes_recv: u64,
    pub packets_lost: u64,
    pub resends: u64,
}

struct OutMsg {
    id: u16,
    flags: u8,
    data: Box<[u8]>,
    sent_at: Option<Instant>,
    acked: bool,
}

#[derive(Default)]
struct ReliableOut {
    next_id: u16,
    base_id: u16,
    queue: VecDeque<OutMsg>,
}

#[derive(Default)]
struct ReliableIn {
    expected: u16,
    pending: HashMap<u16, (u8, Box<[u8]>)>,
    frag: Vec<u8>,
}

struct Unsent {
    ch: u8,
    flags: u8,
    seq: u16,
    data: Box<[u8]>,
}

struct SentPacket {
    seq: u16,
    time: Instant,
    acked: bool,
    msgs: Vec<(u8, u16)>,
}

pub struct Connection {
    pub(crate) salt: u32,
    send_seq: u16,
    remote_seq: u16,
    remote_bits: u32,
    has_remote: bool,
    ack_pending: bool,
    recv_since_ack: u32,
    sent: Vec<SentPacket>,
    rel_out: [ReliableOut; RELIABLE_CHANNELS],
    rel_in: [ReliableIn; RELIABLE_CHANNELS],
    seq_out: [u16; SEQUENCED_CHANNELS],
    seq_in: [Option<u16>; SEQUENCED_CHANNELS],
    unsent: Vec<Unsent>,
    srtt: f32,
    rttvar: f32,
    has_rtt: bool,
    tokens: f64,
    rate: f64,
    last_refill: Instant,
    pub(crate) last_recv: Instant,
    last_send: Instant,
    compress: bool,
    pkt: Vec<u8>,
    pkt_msgs: Vec<(u8, u16)>,
    pub stats: Stats,
}

impl Connection {
    pub fn new(salt: u32, now: Instant, rate_bytes_per_sec: u32, compress: bool) -> Self {
        Self {
            salt,
            send_seq: 0,
            remote_seq: 0,
            remote_bits: 0,
            has_remote: false,
            ack_pending: false,
            recv_since_ack: 0,
            sent: (0..SENT_SLOTS)
                .map(|_| SentPacket { seq: 0, time: now, acked: true, msgs: Vec::new() })
                .collect(),
            rel_out: Default::default(),
            rel_in: Default::default(),
            seq_out: [0; SEQUENCED_CHANNELS],
            seq_in: [None; SEQUENCED_CHANNELS],
            unsent: Vec::new(),
            srtt: 0.2,
            rttvar: 0.1,
            has_rtt: false,
            tokens: rate_bytes_per_sec as f64 * 0.02,
            rate: rate_bytes_per_sec as f64,
            last_refill: now,
            last_recv: now,
            last_send: now,
            compress,
            pkt: Vec::with_capacity(MAX_PACKET),
            pkt_msgs: Vec::new(),
            stats: Stats::default(),
        }
    }

    pub(crate) fn owes_ack(&self) -> bool {
        self.recv_since_ack >= 16
    }

    pub fn reliable_backlog(&self) -> usize {
        self.rel_out.iter().map(|r| r.queue.len()).sum()
    }

    pub fn send(&mut self, ch: u8, data: &[u8]) -> Result<(), SendError> {
        if ch >= CHANNELS {
            return Err(SendError::BadChannel);
        }
        if data.len() > MAX_MESSAGE {
            return Err(SendError::TooLarge);
        }
        let (flags, payload) = self.encode(data);
        match delivery(ch) {
            Delivery::Reliable => {
                let r = &mut self.rel_out[ch as usize];
                let parts = payload.len().div_ceil(FRAG_SIZE).max(1);
                if r.queue.len() + parts > RELIABLE_WINDOW as usize {
                    return Err(SendError::WindowFull);
                }
                if parts == 1 {
                    let id = r.next_id;
                    r.next_id = id.wrapping_add(1);
                    r.queue.push_back(OutMsg { id, flags, data: payload, sent_at: None, acked: false });
                } else {
                    for (i, chunk) in payload.chunks(FRAG_SIZE).enumerate() {
                        let id = r.next_id;
                        r.next_id = id.wrapping_add(1);
                        let f = if i + 1 < parts { flags | F_FRAG } else { flags };
                        r.queue.push_back(OutMsg { id, flags: f, data: chunk.into(), sent_at: None, acked: false });
                    }
                }
            }
            kind => {
                if payload.len() > MAX_UNRELIABLE {
                    return Err(SendError::TooLarge);
                }
                let seq = if kind == Delivery::Sequenced {
                    let s = &mut self.seq_out[(ch as usize) - RELIABLE_CHANNELS];
                    *s = s.wrapping_add(1);
                    *s
                } else {
                    0
                };
                self.unsent.push(Unsent { ch, flags, seq, data: payload });
            }
        }
        Ok(())
    }

    fn encode(&self, data: &[u8]) -> (u8, Box<[u8]>) {
        if self.compress && data.len() >= COMPRESS_MIN {
            let c = lz4_flex::block::compress_prepend_size(data);
            if c.len() + 8 < data.len() {
                return (F_LZ4, c.into_boxed_slice());
            }
        }
        (0, data.into())
    }

    fn resend_delay(&self) -> Duration {
        let base = if self.has_rtt { self.srtt + 4.0 * self.rttvar } else { 0.25 };
        Duration::from_secs_f32(base.clamp(0.03, 1.0) + 0.005)
    }

    pub(crate) fn flush(&mut self, now: Instant, out: &mut dyn FnMut(&[u8])) {
        let dt = now.saturating_duration_since(self.last_refill).as_secs_f64();
        self.last_refill = now;
        self.tokens = (self.tokens + dt * self.rate).min(self.rate * 0.05);
        let resend_after = self.resend_delay();
        let mut sent_any = false;

        let unsent = std::mem::take(&mut self.unsent);
        for m in &unsent {
            let seq = (delivery(m.ch) == Delivery::Sequenced).then_some(m.seq);
            self.put(now, m.ch, m.flags, seq, &m.data, None, out, &mut sent_any);
        }
        let mut unsent = unsent;
        unsent.clear();
        self.unsent = unsent;

        for ch in 0..RELIABLE_CHANNELS {
            let mut i = 0;
            while i < self.rel_out[ch].queue.len() {
                if self.tokens <= 0.0 {
                    break;
                }
                let m = &self.rel_out[ch].queue[i];
                let due = !m.acked && m.sent_at.is_none_or(|t| now.saturating_duration_since(t) >= resend_after);
                if due {
                    let (id, flags, resend) = (m.id, m.flags, m.sent_at.is_some());
                    let data = std::mem::take(&mut self.rel_out[ch].queue[i].data);
                    self.put(now, ch as u8, flags, Some(id), &data, Some(id), out, &mut sent_any);
                    let m = &mut self.rel_out[ch].queue[i];
                    m.data = data;
                    m.sent_at = Some(now);
                    if resend {
                        self.stats.resends += 1;
                    }
                }
                i += 1;
            }
        }
        if !self.pkt.is_empty() {
            self.finish(now, out);
            sent_any = true;
        }
        if !sent_any {
            let idle = now.saturating_duration_since(self.last_send);
            if (self.ack_pending && idle >= ACK_DELAY) || idle >= KEEPALIVE {
                self.begin();
                self.finish(now, out);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn put(
        &mut self,
        now: Instant,
        ch: u8,
        flags: u8,
        num: Option<u16>,
        data: &[u8],
        reliable_id: Option<u16>,
        out: &mut dyn FnMut(&[u8]),
        sent_any: &mut bool,
    ) {
        let size = 1 + if num.is_some() { 2 } else { 0 } + varint_len(data.len()) + data.len();
        if !self.pkt.is_empty() && self.pkt.len() + size > MAX_PACKET {
            self.finish(now, out);
            *sent_any = true;
        }
        if self.pkt.is_empty() {
            self.begin();
        }
        let mut w = Writer::new(&mut self.pkt);
        w.u8(ch | flags);
        if let Some(n) = num {
            w.u16(n);
        }
        w.blob(data);
        if let Some(id) = reliable_id {
            self.pkt_msgs.push((ch, id));
            self.tokens -= size as f64;
        }
    }

    fn begin(&mut self) {
        self.pkt.clear();
        Writer::new(&mut self.pkt)
            .u8(KIND_DATA)
            .u32(self.salt)
            .u16(self.send_seq)
            .u16(self.remote_seq)
            .u32(self.remote_bits);
    }

    fn finish(&mut self, now: Instant, out: &mut dyn FnMut(&[u8])) {
        let slot = &mut self.sent[self.send_seq as usize % SENT_SLOTS];
        if !slot.acked {
            self.stats.packets_lost += 1;
        }
        slot.seq = self.send_seq;
        slot.time = now;
        slot.acked = false;
        slot.msgs.clear();
        slot.msgs.append(&mut self.pkt_msgs);
        out(&self.pkt);
        self.stats.packets_sent += 1;
        self.stats.bytes_sent += self.pkt.len() as u64;
        self.send_seq = self.send_seq.wrapping_add(1);
        self.last_send = now;
        self.ack_pending = false;
        self.recv_since_ack = 0;
        self.pkt.clear();
    }

    pub(crate) fn receive(&mut self, now: Instant, packet: &[u8], delivered: &mut Vec<(u8, Vec<u8>)>) -> Option<()> {
        let mut r = Reader::new(packet);
        let seq = r.u16()?;
        let ack = r.u16()?;
        let bits = r.u32()?;
        if !self.track_remote(seq) {
            return Some(());
        }
        self.stats.packets_recv += 1;
        self.stats.bytes_recv += (packet.len() + 5) as u64;
        self.last_recv = now;
        self.ack_pending = true;
        self.recv_since_ack += 1;
        self.process_acks(now, ack, bits);
        while r.remaining() > 0 {
            let hdr = r.u8()?;
            let ch = hdr & 0x0F;
            let flags = hdr & 0xF0;
            match delivery(ch) {
                Delivery::Reliable => {
                    let id = r.u16()?;
                    let data = r.blob()?;
                    self.recv_reliable(ch, id, flags, data, delivered);
                }
                Delivery::Sequenced => {
                    let s = r.u16()?;
                    let data = r.blob()?;
                    let last = &mut self.seq_in[(ch as usize) - RELIABLE_CHANNELS];
                    if last.is_none_or(|l| newer(s, l)) {
                        *last = Some(s);
                        deliver(ch, flags, data.to_vec(), delivered);
                    }
                }
                Delivery::Unreliable => {
                    let data = r.blob()?;
                    deliver(ch, flags, data.to_vec(), delivered);
                }
            }
        }
        Some(())
    }

    fn track_remote(&mut self, seq: u16) -> bool {
        if !self.has_remote {
            self.has_remote = true;
            self.remote_seq = seq;
            self.remote_bits = 0;
            return true;
        }
        if newer(seq, self.remote_seq) {
            let d = seq.wrapping_sub(self.remote_seq) as u32;
            self.remote_bits = match d {
                1..=31 => (self.remote_bits << d) | (1 << (d - 1)),
                32 => 1 << 31,
                _ => 0,
            };
            self.remote_seq = seq;
            return true;
        }
        let d = self.remote_seq.wrapping_sub(seq) as u32;
        if d == 0 || d > 32 {
            return false;
        }
        let bit = 1u32 << (d - 1);
        if self.remote_bits & bit != 0 {
            return false;
        }
        self.remote_bits |= bit;
        true
    }

    fn process_acks(&mut self, now: Instant, ack: u16, bits: u32) {
        for i in 0..=32u16 {
            if i > 0 && bits & (1 << (i - 1)) == 0 {
                continue;
            }
            let seq = ack.wrapping_sub(i);
            let slot = &mut self.sent[seq as usize % SENT_SLOTS];
            if slot.acked || slot.seq != seq {
                continue;
            }
            slot.acked = true;
            if i == 0 {
                let sample = now.saturating_duration_since(slot.time).as_secs_f32();
                if self.has_rtt {
                    self.rttvar = 0.75 * self.rttvar + 0.25 * (self.srtt - sample).abs();
                    self.srtt = 0.875 * self.srtt + 0.125 * sample;
                } else {
                    self.has_rtt = true;
                    self.srtt = sample;
                    self.rttvar = sample / 2.0;
                }
                self.stats.rtt_ms = self.srtt * 1000.0;
            }
            for &(ch, id) in &slot.msgs {
                let q = &mut self.rel_out[ch as usize];
                let off = id.wrapping_sub(q.base_id) as usize;
                if let Some(m) = q.queue.get_mut(off) {
                    m.acked = true;
                }
            }
        }
        for q in &mut self.rel_out {
            while q.queue.front().is_some_and(|m| m.acked) {
                q.queue.pop_front();
                q.base_id = q.base_id.wrapping_add(1);
            }
        }
    }

    fn recv_reliable(&mut self, ch: u8, id: u16, flags: u8, data: &[u8], delivered: &mut Vec<(u8, Vec<u8>)>) {
        let ri = &mut self.rel_in[ch as usize];
        let off = id.wrapping_sub(ri.expected);
        if off >= RELIABLE_WINDOW {
            return;
        }
        if off != 0 {
            ri.pending.entry(id).or_insert_with(|| (flags, data.into()));
            return;
        }
        accept_reliable(ri, ch, flags, data, delivered);
        ri.expected = ri.expected.wrapping_add(1);
        while let Some((f, d)) = ri.pending.remove(&ri.expected) {
            accept_reliable(ri, ch, f, &d, delivered);
            ri.expected = ri.expected.wrapping_add(1);
        }
    }
}

fn accept_reliable(ri: &mut ReliableIn, ch: u8, flags: u8, data: &[u8], delivered: &mut Vec<(u8, Vec<u8>)>) {
    if flags & F_FRAG != 0 {
        if ri.frag.len() + data.len() <= MAX_MESSAGE {
            ri.frag.extend_from_slice(data);
        }
        return;
    }
    if ri.frag.is_empty() {
        deliver(ch, flags, data.to_vec(), delivered);
    } else {
        ri.frag.extend_from_slice(data);
        let whole = std::mem::take(&mut ri.frag);
        deliver(ch, flags, whole, delivered);
    }
}

fn deliver(ch: u8, flags: u8, data: Vec<u8>, delivered: &mut Vec<(u8, Vec<u8>)>) {
    if flags & F_LZ4 == 0 {
        delivered.push((ch, data));
        return;
    }
    let size = data.get(..4).map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize);
    if size.is_some_and(|s| s <= MAX_MESSAGE) {
        if let Ok(d) = lz4_flex::block::decompress_size_prepended(&data) {
            delivered.push((ch, d));
        }
    }
}

#[inline]
fn varint_len(mut v: usize) -> usize {
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}
