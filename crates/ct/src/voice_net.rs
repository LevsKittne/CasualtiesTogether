use crate::rules::Id;
use crate::session::Session;
use crate::voice::{self, STREAM_META, STREAM_MUSIC, STREAM_VOICE};
use ct_net::channel;

const TILE_METERS: f32 = 0.3;

impl Session {
    fn voice_allowed(&self, stream: u8) -> bool {
        match stream {
            STREAM_VOICE => self.rules.flag(Id::VoicechatEnabled),
            STREAM_MUSIC | STREAM_META => self.rules.flag(Id::EnableMP3Sync),
            _ => false,
        }
    }

    pub(crate) fn hear_range(&self) -> f32 {
        self.rules.get(Id::ProximityHearDistance).as_f32() / TILE_METERS
    }

    fn pos_of(&self, id: u16) -> Option<(f32, f32)> {
        let s = if id == self.local_id { self.local_state.as_ref() } else { self.tracks.get(&id).and_then(|t| t.latest()) }?;
        Some((s.state.x, s.state.y))
    }

    fn in_range(&self, a: u16, b: u16, range: f32) -> bool {
        match (self.pos_of(a), self.pos_of(b)) {
            (Some(p), Some(q)) => {
                let (dx, dy) = (p.0 - q.0, p.1 - q.1);
                dx * dx + dy * dy <= range * range
            }
            _ => true,
        }
    }

    fn relay_voice(&self, from: u16, data: &[u8]) {
        let range = self.hear_range() * 1.25;
        for (&id, &peer) in self.player_peer.iter() {
            if id != from && self.in_range(from, id, range) {
                let _ = self.ep.send(peer, channel::VOICE, data);
            }
        }
    }

    pub(crate) fn on_voice_from(&mut self, id: u16, data: &mut [u8]) {
        let Some(stream) = voice::decode_packet(data).map(|p| p.stream) else { return };
        if !self.voice_allowed(stream) {
            return;
        }
        voice::set_from(data, id);
        self.relay_voice(id, data);
        if self.in_range(id, self.local_id, self.hear_range() * 1.25) {
            if let Some(p) = voice::decode_packet(data) {
                voice::with_mixer(|m| m.push(&p));
            }
        }
    }

    pub(crate) fn on_voice_client(&mut self, data: &[u8]) {
        if let Some(p) = voice::decode_packet(data) {
            if p.from != self.local_id && self.voice_allowed(p.stream) {
                voice::with_mixer(|m| m.push(&p));
            }
        }
    }

    pub fn send_voice(&mut self, packets: &[Vec<u8>]) {
        for data in packets {
            let Some(stream) = voice::decode_packet(data).map(|p| p.stream) else { continue };
            if !self.voice_allowed(stream) {
                continue;
            }
            if self.is_host() {
                self.relay_voice(self.local_id, data);
            } else if let Some(peer) = self.host_peer() {
                let _ = self.ep.send(peer, channel::VOICE, data);
            }
        }
    }
}
