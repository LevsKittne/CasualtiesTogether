mod conn;
mod endpoint;
pub mod packet;
mod steam;
mod transport;
pub mod wire;

pub use conn::{SendError, Stats};
pub use endpoint::{Config, Endpoint, Event, PeerId, random_u32};
pub use packet::{CHANNELS, Delivery, MAX_UNRELIABLE, delivery};
pub use steam::SteamTransport;
pub use transport::{Addr, Transport, UdpTransport};

pub mod channel {
    pub const CONTROL: u8 = 0;
    pub const STATE: u8 = 1;
    pub const BULK: u8 = 2;
    pub const CHAT: u8 = 3;
    pub const SNAPSHOT: u8 = 8;
    pub const INPUT: u8 = 9;
    pub const VOICE: u8 = 12;
    pub const POSE: u8 = 13;
}
