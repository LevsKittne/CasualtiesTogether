pub const PROTOCOL: u32 = 0x4354_0001;
pub const MAX_PACKET: usize = 1200;
pub const DATA_HEADER: usize = 1 + 4 + 2 + 2 + 4;
pub const MAX_UNRELIABLE: usize = MAX_PACKET - DATA_HEADER - 1 - 2 - 2;

pub const KIND_CONNECT: u8 = 1;
pub const KIND_ACCEPT: u8 = 2;
pub const KIND_DENY: u8 = 3;
pub const KIND_DATA: u8 = 4;
pub const KIND_DISCONNECT: u8 = 5;

pub const CHANNELS: u8 = 16;
pub const RELIABLE_CHANNELS: usize = 8;
pub const SEQUENCED_CHANNELS: usize = 4;

pub(crate) const F_FRAG: u8 = 0x10;
pub(crate) const F_LZ4: u8 = 0x20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Reliable,
    Sequenced,
    Unreliable,
}

#[inline]
pub fn delivery(ch: u8) -> Delivery {
    match ch {
        0..=7 => Delivery::Reliable,
        8..=11 => Delivery::Sequenced,
        _ => Delivery::Unreliable,
    }
}

#[inline]
pub fn newer(a: u16, b: u16) -> bool {
    a != b && a.wrapping_sub(b) < 0x8000
}
