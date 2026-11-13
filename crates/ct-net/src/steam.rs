use crate::transport::{Addr, Transport};
use core::ffi::{c_char, c_void};

const CHANNEL: i32 = 3;
const SEND_FLAGS: i32 = 1 | 32;
const IDENTITY_STEAMID: i32 = 16;

#[repr(C)]
#[derive(Clone, Copy)]
struct Identity {
    kind: i32,
    size: i32,
    data: [u64; 16],
}

impl Identity {
    fn steam(id: u64) -> Self {
        let mut data = [0u64; 16];
        data[0] = id;
        Self { kind: IDENTITY_STEAMID, size: 8, data }
    }
}

#[repr(C)]
struct Message {
    data: *const u8,
    size: i32,
    conn: u32,
    peer: Identity,
}

type Ptr = *mut c_void;

struct Api {
    messages: Ptr,
    user: Ptr,
    friends: Ptr,
    send: unsafe extern "C" fn(Ptr, *const Identity, *const c_void, u32, i32, i32) -> i32,
    receive: unsafe extern "C" fn(Ptr, i32, *mut *mut Message, i32) -> i32,
    accept: unsafe extern "C" fn(Ptr, *const Identity) -> bool,
    close: unsafe extern "C" fn(Ptr, *const Identity) -> bool,
    release: unsafe extern "C" fn(*mut Message),
    steam_id: unsafe extern "C" fn(Ptr) -> u64,
    persona: unsafe extern "C" fn(Ptr) -> *const c_char,
}

pub struct SteamTransport {
    api: Api,
}

unsafe impl Send for SteamTransport {}
unsafe impl Sync for SteamTransport {}

#[cfg(windows)]
mod sys {
    use core::ffi::c_void;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        pub fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    }
}

impl SteamTransport {
    #[cfg(windows)]
    pub fn attach() -> Result<Self, String> {
        let name: Vec<u16> = "steam_api64.dll\0".encode_utf16().collect();
        let m = unsafe { sys::GetModuleHandleW(name.as_ptr()) };
        if m.is_null() {
            return Err("steam_api64.dll not loaded".into());
        }
        macro_rules! f {
            ($n:literal) => {{
                let p = unsafe { sys::GetProcAddress(m, concat!($n, "\0").as_ptr()) };
                if p.is_null() {
                    return Err(concat!("missing ", $n).into());
                }
                unsafe { core::mem::transmute::<*mut c_void, _>(p) }
            }};
        }
        let messages_v: unsafe extern "C" fn() -> Ptr = f!("SteamAPI_SteamNetworkingMessages_SteamAPI_v002");
        let user_v: unsafe extern "C" fn() -> Ptr = f!("SteamAPI_SteamUser_v023");
        let friends_v: unsafe extern "C" fn() -> Ptr = f!("SteamAPI_SteamFriends_v018");
        let api = Api {
            messages: unsafe { messages_v() },
            user: unsafe { user_v() },
            friends: unsafe { friends_v() },
            send: f!("SteamAPI_ISteamNetworkingMessages_SendMessageToUser"),
            receive: f!("SteamAPI_ISteamNetworkingMessages_ReceiveMessagesOnChannel"),
            accept: f!("SteamAPI_ISteamNetworkingMessages_AcceptSessionWithUser"),
            close: f!("SteamAPI_ISteamNetworkingMessages_CloseSessionWithUser"),
            release: f!("SteamAPI_SteamNetworkingMessage_t_Release"),
            steam_id: f!("SteamAPI_ISteamUser_GetSteamID"),
            persona: f!("SteamAPI_ISteamFriends_GetPersonaName"),
        };
        if api.messages.is_null() || api.user.is_null() {
            return Err("Steam is not initialized".into());
        }
        Ok(Self { api })
    }

    #[cfg(not(windows))]
    pub fn attach() -> Result<Self, String> {
        Err("steam transport is windows-only".into())
    }

    pub fn local_id(&self) -> u64 {
        unsafe { (self.api.steam_id)(self.api.user) }
    }

    pub fn persona_name(&self) -> String {
        if self.api.friends.is_null() {
            return String::new();
        }
        let p = unsafe { (self.api.persona)(self.api.friends) };
        if p.is_null() {
            return String::new();
        }
        unsafe { std::ffi::CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }

    pub fn accept_session(&self, id: u64) -> bool {
        let ident = Identity::steam(id);
        unsafe { (self.api.accept)(self.api.messages, &ident) }
    }

    pub fn close_session(&self, id: u64) {
        let ident = Identity::steam(id);
        unsafe { (self.api.close)(self.api.messages, &ident) };
    }
}

impl Transport for SteamTransport {
    fn handles(&self, addr: &Addr) -> bool {
        matches!(addr, Addr::Steam(_))
    }

    fn send(&self, to: &Addr, data: &[u8]) {
        if let Addr::Steam(id) = to {
            let ident = Identity::steam(*id);
            unsafe {
                (self.api.send)(self.api.messages, &ident, data.as_ptr().cast(), data.len() as u32, SEND_FLAGS, CHANNEL);
            }
        }
    }

    fn recv(&self, buf: &mut [u8]) -> Option<(usize, Addr)> {
        let mut msgs = [core::ptr::null_mut::<Message>(); 1];
        let n = unsafe { (self.api.receive)(self.api.messages, CHANNEL, msgs.as_mut_ptr(), 1) };
        if n <= 0 {
            return None;
        }
        let m = msgs[0];
        let out = unsafe {
            let msg = &*m;
            let len = (msg.size.max(0) as usize).min(buf.len());
            core::ptr::copy_nonoverlapping(msg.data, buf.as_mut_ptr(), len);
            let from = if msg.peer.kind == IDENTITY_STEAMID { msg.peer.data[0] } else { 0 };
            (len, Addr::Steam(from))
        };
        unsafe { (self.api.release)(m) };
        Some(out)
    }
}
