use ct_net::{Addr, Config, Endpoint, Event, PeerId, Transport, UdpTransport, channel};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

struct Lossy {
    inner: UdpTransport,
    loss_permille: AtomicU64,
    rng: AtomicU64,
}

impl Lossy {
    fn new(loss: f64) -> (Arc<Self>, SocketAddr) {
        let inner = UdpTransport::bind("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = inner.local_addr().unwrap();
        let seed = ct_net::random_u32() as u64 | 1;
        (Arc::new(Self { inner, loss_permille: AtomicU64::new((loss * 1000.0) as u64), rng: AtomicU64::new(seed) }), addr)
    }

    fn roll(&self) -> u64 {
        let mut x = self.rng.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng.store(x, Ordering::Relaxed);
        x % 1000
    }
}

impl Transport for Lossy {
    fn handles(&self, addr: &Addr) -> bool {
        self.inner.handles(addr)
    }

    fn send(&self, to: &Addr, data: &[u8]) {
        let loss = self.loss_permille.load(Ordering::Relaxed);
        if self.roll() >= loss {
            self.inner.send(to, data);
        }
        if loss < 1000 && self.roll() < loss / 4 {
            self.inner.send(to, data);
        }
    }

    fn recv(&self, buf: &mut [u8]) -> Option<(usize, Addr)> {
        self.inner.recv(buf)
    }

    fn waits(&self) -> bool {
        true
    }
}

struct Pair {
    server: Endpoint,
    client: Endpoint,
    on_server: PeerId,
    on_client: PeerId,
}

fn pair(loss: f64) -> Pair {
    let (st, saddr) = Lossy::new(loss);
    let (ct, _) = Lossy::new(loss);
    let server = Endpoint::new(Config { accept_incoming: true, ..Default::default() }, vec![st]);
    let client = Endpoint::new(Config::default(), vec![ct]);
    let on_client = client.connect(Addr::Udp(saddr), b"hello");
    let mut on_server = 0;
    let mut connected = false;
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut ev = Vec::new();
    while !(connected && on_server != 0) {
        assert!(Instant::now() < deadline, "handshake timed out");
        server.poll(&mut ev);
        for e in ev.drain(..) {
            if let Event::Request { peer, payload, .. } = e {
                assert_eq!(payload, b"hello");
                server.accept(peer);
                on_server = peer;
            }
        }
        client.poll(&mut ev);
        for e in ev.drain(..) {
            if let Event::Connected { peer, .. } = e {
                assert_eq!(peer, on_client);
                connected = true;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Pair { server, client, on_server, on_client }
}

fn collect(ep: &Endpoint, want: usize, secs: u64) -> Vec<(u8, Vec<u8>)> {
    let mut got = Vec::new();
    let mut ev = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(secs);
    while got.len() < want && Instant::now() < deadline {
        ep.poll(&mut ev);
        for e in ev.drain(..) {
            if let Event::Message { channel, data, .. } = e {
                got.push((channel, data));
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    got
}

#[test]
fn reliable_ordered_under_loss() {
    let p = pair(0.2);
    for i in 0u32..2000 {
        p.client.send(p.on_client, channel::STATE, &i.to_le_bytes()).unwrap();
        if i % 50 == 0 {
            p.client.flush();
        }
    }
    p.client.flush();
    let got = collect(&p.server, 2000, 20);
    assert_eq!(got.len(), 2000);
    for (i, (ch, d)) in got.iter().enumerate() {
        assert_eq!(*ch, channel::STATE);
        assert_eq!(u32::from_le_bytes(d[..4].try_into().unwrap()), i as u32);
    }
    let stats = p.client.stats(p.on_client).unwrap();
    assert!(stats.resends > 0);
}

#[test]
fn large_messages_fragment_and_compress() {
    let p = pair(0.1);
    let text: Vec<u8> = (0..2_000_000u32).map(|i| b"casualties"[(i % 10) as usize]).collect();
    let mut noise = vec![0u8; 300_000];
    let mut x = 0x9E3779B97F4A7C15u64;
    for b in &mut noise {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *b = x as u8;
    }
    p.server.send(p.on_server, channel::BULK, &text).unwrap();
    p.server.send(p.on_server, channel::BULK, &noise).unwrap();
    p.server.flush();
    let got = collect(&p.client, 2, 30);
    assert_eq!(got.len(), 2);
    assert!(got[0].1 == text);
    assert!(got[1].1 == noise);
    let stats = p.server.stats(p.on_server).unwrap();
    eprintln!("bulk: sent {} bytes, resends {}, lost {}", stats.bytes_sent, stats.resends, stats.packets_lost);
    assert!(stats.bytes_sent < 750_000, "compression not applied: {}", stats.bytes_sent);
}

#[test]
fn sequenced_drops_stale() {
    let p = pair(0.0);
    for i in 0u16..200 {
        p.client.send(p.on_client, channel::SNAPSHOT, &i.to_le_bytes()).unwrap();
        p.client.flush();
    }
    let got = collect(&p.server, 200, 2);
    let vals: Vec<u16> = got.iter().map(|(_, d)| u16::from_le_bytes(d[..2].try_into().unwrap())).collect();
    assert!(!vals.is_empty());
    assert!(vals.windows(2).all(|w| w[0] < w[1]));
}

#[test]
fn rtt_and_disconnect() {
    let p = pair(0.0);
    std::thread::sleep(Duration::from_millis(500));
    let rtt = p.client.stats(p.on_client).unwrap().rtt_ms;
    assert!(rtt > 0.0 && rtt < 250.0, "rtt {rtt}");
    p.client.disconnect(p.on_client, "bye");
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut ev = Vec::new();
    let mut reason = None;
    while reason.is_none() && Instant::now() < deadline {
        p.server.poll(&mut ev);
        for e in ev.drain(..) {
            if let Event::Disconnected { peer, reason: r } = e {
                assert_eq!(peer, p.on_server);
                reason = Some(r);
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(reason.as_deref(), Some("bye"));
}

#[test]
fn timeout_when_server_dies() {
    let (st, saddr) = Lossy::new(0.0);
    let server = Endpoint::new(Config { accept_incoming: true, ..Default::default() }, vec![st.clone()]);
    let (ct, _) = Lossy::new(0.0);
    let client = Endpoint::new(Config { timeout: Duration::from_millis(800), ..Default::default() }, vec![ct]);
    let id = client.connect(Addr::Udp(saddr), b"");
    let mut ev = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut connected = false;
    while !connected && Instant::now() < deadline {
        server.poll(&mut ev);
        for e in ev.drain(..) {
            if let Event::Request { peer, .. } = e {
                server.accept(peer);
            }
        }
        client.poll(&mut ev);
        connected |= ev.drain(..).any(|e| matches!(e, Event::Connected { .. }));
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(connected);
    st.loss_permille.store(1000, Ordering::Relaxed);
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut timed_out = false;
    while !timed_out && Instant::now() < deadline {
        client.poll(&mut ev);
        timed_out |= ev.drain(..).any(|e| matches!(e, Event::Disconnected { peer, .. } if peer == id));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(timed_out);
}
