use ct_net::wire::{Reader, Writer};
use std::collections::{HashMap, VecDeque};

pub const PARAMS_TAG: u8 = 30;
pub const EDITS_TAG: u8 = 31;
pub const CALL_REQ_TAG: u8 = 32;
pub const CALL_APPLY_TAG: u8 = 33;
pub const METHOD_DESTROY: u16 = 0xFFFF;
pub const CALL_PERSIST: u8 = 1;
pub const CALL_REPLACE: u8 = 2;
const MAX_EDITS_PER_MSG: usize = 8192;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TileEdit {
    pub x: u16,
    pub y: u16,
    pub block: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityCall {
    pub entity: u32,
    pub method: u16,
    pub flags: u8,
    pub seed: u32,
    pub args: Vec<u8>,
}

impl EntityCall {
    pub fn encode(&self, tag: u8, generation: u32) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.args.len() + 20);
        Writer::new(&mut out).u8(tag).u32(generation).u32(self.entity).u16(self.method).u8(self.flags).u32(self.seed).blob(&self.args);
        out
    }

    pub fn decode(data: &[u8], tag: u8) -> Option<(u32, Self)> {
        let mut r = Reader::new(data);
        if r.u8()? != tag {
            return None;
        }
        let generation = r.u32()?;
        let call = Self { entity: r.u32()?, method: r.u16()?, flags: r.u8()?, seed: r.u32()?, args: r.blob()?.to_vec() };
        Some((generation, call))
    }

    pub fn persist(&self) -> bool {
        self.flags & CALL_PERSIST != 0 || self.method == METHOD_DESTROY
    }
}

#[inline]
fn key(x: u16, y: u16) -> u32 {
    ((x as u32) << 16) | y as u32
}

#[derive(Default)]
pub struct World {
    pub generation: u32,
    pub params: Vec<u8>,
    edits: HashMap<u32, u16>,
    early: HashMap<u32, HashMap<u32, u16>>,
    ready: u32,
    outbox: Vec<TileEdit>,
    inbox: Vec<TileEdit>,
    pub rev: u32,
    journal: Vec<EntityCall>,
    call_pending: Vec<EntityCall>,
    call_inbox: VecDeque<EntityCall>,
}

impl World {
    pub fn begin(&mut self, params: Vec<u8>) -> u32 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.params = params;
        self.edits.clear();
        self.outbox.clear();
        self.inbox.clear();
        self.early.retain(|g, _| *g > self.generation);
        self.ready = 0;
        self.reset_calls();
        self.rev = self.rev.wrapping_add(1);
        self.generation
    }

    fn reset_calls(&mut self) {
        self.journal.clear();
        self.call_pending.clear();
        self.call_inbox.clear();
    }

    pub fn record_call(&mut self, call: &EntityCall) {
        if !call.persist() {
            return;
        }
        if call.method == METHOD_DESTROY {
            self.journal.retain(|c| c.entity != call.entity);
        } else if call.flags & CALL_REPLACE != 0 {
            self.journal.retain(|c| c.entity != call.entity || c.method != call.method);
        }
        self.journal.push(call.clone());
    }

    pub fn incoming_call(&mut self, generation: u32, call: EntityCall) {
        if generation != self.generation {
            return;
        }
        if self.is_ready() {
            self.call_inbox.push_back(call);
        } else if call.persist() {
            self.call_pending.push(call);
        }
    }

    pub fn next_call(&mut self) -> Option<EntityCall> {
        self.call_inbox.pop_front()
    }

    pub fn journal(&self) -> &[EntityCall] {
        &self.journal
    }

    pub fn adopt(&mut self, generation: u32, params: Vec<u8>) -> bool {
        if generation == self.generation && params == self.params {
            return false;
        }
        self.generation = generation;
        self.params = params;
        self.edits = self.early.remove(&generation).unwrap_or_default();
        self.early.retain(|g, _| *g > generation);
        self.inbox.clear();
        self.outbox.clear();
        self.ready = 0;
        self.reset_calls();
        self.rev = self.rev.wrapping_add(1);
        true
    }

    pub fn mark_ready(&mut self, generation: u32) {
        if generation != self.generation || self.ready == generation {
            return;
        }
        self.ready = generation;
        let pending = std::mem::take(&mut self.call_pending);
        self.call_inbox.extend(pending);
        self.inbox.extend(self.edits.iter().map(|(&k, &b)| TileEdit { x: (k >> 16) as u16, y: k as u16, block: b }));
    }

    pub fn is_ready(&self) -> bool {
        self.ready != 0 && self.ready == self.generation
    }

    pub fn local_edit(&mut self, e: TileEdit) {
        if self.generation == 0 {
            return;
        }
        self.edits.insert(key(e.x, e.y), e.block);
        self.outbox.push(e);
    }

    pub fn remote_edits(&mut self, generation: u32, list: &[TileEdit]) {
        if generation == self.generation {
            for e in list {
                self.edits.insert(key(e.x, e.y), e.block);
            }
            if self.is_ready() {
                self.inbox.extend_from_slice(list);
            }
        } else if generation > self.generation {
            let m = self.early.entry(generation).or_default();
            for e in list {
                m.insert(key(e.x, e.y), e.block);
            }
        }
    }

    pub fn take_outbox(&mut self) -> Vec<TileEdit> {
        std::mem::take(&mut self.outbox)
    }

    pub fn drain_inbox(&mut self, out: &mut [TileEdit]) -> usize {
        let n = self.inbox.len().min(out.len());
        for (dst, src) in out.iter_mut().zip(self.inbox.drain(..n)) {
            *dst = src;
        }
        n
    }

    pub fn snapshot(&self) -> Vec<TileEdit> {
        self.edits.iter().map(|(&k, &b)| TileEdit { x: (k >> 16) as u16, y: k as u16, block: b }).collect()
    }

    pub fn edit_count(&self) -> usize {
        self.edits.len()
    }
}

pub fn encode_params(generation: u32, params: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(params.len() + 16);
    Writer::new(&mut out).u8(PARAMS_TAG).u32(generation).blob(params);
    out
}

pub fn decode_params(data: &[u8]) -> Option<(u32, Vec<u8>)> {
    let mut r = Reader::new(data);
    if r.u8()? != PARAMS_TAG {
        return None;
    }
    Some((r.u32()?, r.blob()?.to_vec()))
}

pub fn encode_edits(generation: u32, list: &[TileEdit]) -> Vec<Vec<u8>> {
    list.chunks(MAX_EDITS_PER_MSG)
        .map(|chunk| {
            let mut out = Vec::with_capacity(chunk.len() * 6 + 12);
            let mut w = Writer::new(&mut out);
            w.u8(EDITS_TAG).u32(generation).varint(chunk.len() as u64);
            for e in chunk {
                w.u16(e.x).u16(e.y).u16(e.block);
            }
            out
        })
        .collect()
}

pub fn decode_edits(data: &[u8]) -> Option<(u32, Vec<TileEdit>)> {
    let mut r = Reader::new(data);
    if r.u8()? != EDITS_TAG {
        return None;
    }
    let generation = r.u32()?;
    let n = r.varint()? as usize;
    if n > r.remaining() / 6 {
        return None;
    }
    let mut list = Vec::with_capacity(n);
    for _ in 0..n {
        list.push(TileEdit { x: r.u16()?, y: r.u16()?, block: r.u16()? });
    }
    Some((generation, list))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_buffer_until_ready_and_compact() {
        let mut w = World::default();
        w.adopt(3, vec![1]);
        w.remote_edits(3, &[TileEdit { x: 1, y: 2, block: 5 }, TileEdit { x: 1, y: 2, block: 0 }]);
        let mut out = [TileEdit::default(); 8];
        assert_eq!(w.drain_inbox(&mut out), 0);
        w.mark_ready(3);
        assert_eq!(w.drain_inbox(&mut out), 1);
        assert_eq!(out[0], TileEdit { x: 1, y: 2, block: 0 });
        w.remote_edits(3, &[TileEdit { x: 9, y: 9, block: 1 }]);
        assert_eq!(w.drain_inbox(&mut out), 1);
        w.remote_edits(4, &[TileEdit { x: 7, y: 7, block: 2 }]);
        w.remote_edits(2, &[TileEdit { x: 6, y: 6, block: 2 }]);
        assert_eq!(w.edit_count(), 2);
        w.adopt(4, vec![2]);
        assert_eq!(w.edit_count(), 1);
    }

    #[test]
    fn calls_gate_on_ready_and_compact_journal() {
        let mut w = World::default();
        w.adopt(2, vec![]);
        let c = |e: u32, m: u16, f: u8| EntityCall { entity: e, method: m, flags: f, seed: 7, args: vec![1, 2] };
        w.incoming_call(2, c(1, 5, 0));
        w.incoming_call(2, c(1, 6, CALL_PERSIST));
        w.incoming_call(1, c(9, 6, CALL_PERSIST));
        assert!(w.next_call().is_none());
        w.mark_ready(2);
        assert_eq!(w.next_call().unwrap().method, 6);
        assert!(w.next_call().is_none());
        w.incoming_call(2, c(3, 5, 0));
        assert_eq!(w.next_call().unwrap().entity, 3);
        w.record_call(&c(4, 1, CALL_PERSIST));
        w.record_call(&c(4, 2, 0));
        w.record_call(&c(5, 1, CALL_PERSIST));
        w.record_call(&c(4, METHOD_DESTROY, 0));
        let j: Vec<(u32, u16)> = w.journal().iter().map(|c| (c.entity, c.method)).collect();
        assert_eq!(j, vec![(5, 1), (4, METHOD_DESTROY)]);
        w.record_call(&c(6, 9, CALL_PERSIST | CALL_REPLACE));
        w.record_call(&c(6, 8, CALL_PERSIST));
        let mut last = c(6, 9, CALL_PERSIST | CALL_REPLACE);
        last.seed = 99;
        w.record_call(&last);
        let six: Vec<(u16, u32)> = w.journal().iter().filter(|c| c.entity == 6).map(|c| (c.method, c.seed)).collect();
        assert_eq!(six, vec![(8, 7), (9, 99)]);
        let enc = c(8, 3, 1).encode(CALL_APPLY_TAG, 2);
        assert_eq!(EntityCall::decode(&enc, CALL_APPLY_TAG), Some((2, c(8, 3, 1))));
        assert!(EntityCall::decode(&enc, CALL_REQ_TAG).is_none());
    }

    #[test]
    fn codec_roundtrip_and_split() {
        let list: Vec<TileEdit> = (0..20000).map(|i| TileEdit { x: i as u16, y: (i * 7) as u16, block: (i % 30) as u16 }).collect();
        let msgs = encode_edits(9, &list);
        assert_eq!(msgs.len(), 3);
        let back: Vec<TileEdit> = msgs.iter().flat_map(|m| decode_edits(m).unwrap().1).collect();
        assert_eq!(back, list);
        let (g, p) = decode_params(&encode_params(5, b"abc")).unwrap();
        assert_eq!((g, p.as_slice()), (5, &b"abc"[..]));
    }
}
