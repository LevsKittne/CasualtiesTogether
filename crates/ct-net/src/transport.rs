use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Addr {
    Udp(SocketAddr),
    Steam(u64),
}

impl std::fmt::Display for Addr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Addr::Udp(a) => write!(f, "{a}"),
            Addr::Steam(id) => write!(f, "steam:{id}"),
        }
    }
}

pub trait Transport: Send + Sync {
    fn handles(&self, addr: &Addr) -> bool;
    fn send(&self, to: &Addr, data: &[u8]);
    fn recv(&self, buf: &mut [u8]) -> Option<(usize, Addr)>;
    fn waits(&self) -> bool {
        false
    }
}

pub struct UdpTransport {
    socket: UdpSocket,
}

impl UdpTransport {
    pub fn bind(addr: SocketAddr) -> std::io::Result<Self> {
        let socket = UdpSocket::bind(addr)?;
        socket.set_read_timeout(Some(Duration::from_millis(1)))?;
        let _ = disable_connreset(&socket);
        let _ = set_buffers(&socket, 4 << 20);
        let _ = socket.set_broadcast(false);
        Ok(Self { socket })
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.socket.local_addr()
    }

    fn map(r: std::io::Result<(usize, SocketAddr)>) -> Option<(usize, Addr)> {
        match r {
            Ok((n, a)) => Some((n, Addr::Udp(a))),
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::ConnectionReset) => None,
            Err(_) => None,
        }
    }
}

impl Transport for UdpTransport {
    fn handles(&self, addr: &Addr) -> bool {
        matches!(addr, Addr::Udp(_))
    }

    fn send(&self, to: &Addr, data: &[u8]) {
        if let Addr::Udp(a) = to {
            let _ = self.socket.send_to(data, a);
        }
    }

    fn recv(&self, buf: &mut [u8]) -> Option<(usize, Addr)> {
        Self::map(self.socket.recv_from(buf))
    }

    fn waits(&self) -> bool {
        true
    }
}

#[cfg(windows)]
fn disable_connreset(socket: &UdpSocket) -> std::io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    const SIO_UDP_CONNRESET: u32 = 0x9800_000C;
    #[link(name = "ws2_32")]
    unsafe extern "system" {
        fn WSAIoctl(
            s: usize,
            code: u32,
            inbuf: *const u32,
            inlen: u32,
            outbuf: *mut u8,
            outlen: u32,
            ret: *mut u32,
            ov: *mut u8,
            cb: *mut u8,
        ) -> i32;
    }
    let off = 0u32;
    let mut ret = 0u32;
    let r = unsafe {
        WSAIoctl(
            socket.as_raw_socket() as usize,
            SIO_UDP_CONNRESET,
            &off,
            4,
            core::ptr::null_mut(),
            0,
            &mut ret,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };
    if r == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

#[cfg(windows)]
fn set_buffers(socket: &UdpSocket, size: i32) -> std::io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    #[link(name = "ws2_32")]
    unsafe extern "system" {
        fn setsockopt(s: usize, level: i32, name: i32, val: *const i32, len: i32) -> i32;
    }
    const SOL_SOCKET: i32 = 0xFFFF;
    for opt in [0x1001, 0x1002] {
        if unsafe { setsockopt(socket.as_raw_socket() as usize, SOL_SOCKET, opt, &size, 4) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn set_buffers(_: &UdpSocket, _: i32) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn disable_connreset(_: &UdpSocket) -> std::io::Result<()> {
    Ok(())
}
