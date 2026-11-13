use crate::rules::Id;
use crate::session::{HostOptions, Level, Note, NoteKind, Session, parse_addr};
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

static PORT: AtomicU16 = AtomicU16::new(47800);

struct Client {
    s: Session,
    slot: usize,
}

impl std::ops::Deref for Client {
    type Target = Session;
    fn deref(&self) -> &Session {
        &self.s
    }
}

impl std::ops::DerefMut for Client {
    fn deref_mut(&mut self) -> &mut Session {
        &mut self.s
    }
}

struct World {
    host: Session,
    port: u16,
    dir: std::path::PathBuf,
    notes: Vec<Vec<Note>>,
}

impl World {
    fn new(admin_password: &str) -> Self {
        let port = PORT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("ct-test-{port}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let host = Session::host(HostOptions {
            port,
            steam: None,
            name: "Host".into(),
            password: "pw".into(),
            admin_password: admin_password.into(),
            max_players: Some(8),
            data_dir: Some(dir.clone()),
        })
        .unwrap();
        Self { host, port, dir, notes: vec![Vec::new()] }
    }

    fn join(&mut self, name: &str, pass: &str) -> Client {
        self.notes.push(Vec::new());
        let s = Session::connect(parse_addr(&format!("127.0.0.1:{}", self.port)).unwrap(), None, name, pass).unwrap();
        Client { s, slot: self.notes.len() - 1 }
    }

    fn pump(&mut self, clients: &mut [&mut Client], until: impl Fn(&World, &[&mut Client]) -> bool) -> Vec<bool> {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut alive = vec![true; clients.len()];
        loop {
            self.host.tick();
            self.host.drain_notes(&mut self.notes[0]);
            for (i, c) in clients.iter_mut().enumerate() {
                if alive[i] {
                    alive[i] = c.s.tick();
                    c.s.drain_notes(&mut self.notes[c.slot]);
                }
            }
            if until(self, clients) {
                return alive;
            }
            assert!(Instant::now() < deadline, "timed out");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn has(&self, who: usize, kind: NoteKind, needle: &str) -> bool {
        self.notes[who].iter().any(|n| n.kind == kind && n.text.contains(needle))
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn join_names_password_and_leave() {
    let mut w = World::new("");
    let mut a = w.join("Bob", "pw");
    let mut b = w.join("bob", "pw");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().count() == 3 && c.iter().all(|s| s.players().count() == 3));
    assert!(w.host.players().any(|p| p.name.ends_with("(2)")));
    assert_ne!(a.local_id(), b.local_id());
    let mut wrong = w.join("Eve", "nope");
    let alive = w.pump(&mut [&mut wrong], |w, _| w.has(3, NoteKind::Log, "wrong password"));
    assert!(!alive[0]);
    drop(a);
    w.pump(&mut [&mut b], |w, c| w.host.players().count() == 2 && c[0].players().count() == 2);
}

#[test]
fn chat_reaches_everyone_and_is_rate_limited() {
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    let mut b = w.join("Bob", "pw");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().count() == 3 && c.iter().all(|s| s.players().count() == 3));
    a.chat("hello <b>there</b>");
    w.pump(&mut [&mut a, &mut b], |w, _| (0..3).all(|i| w.has(i, NoteKind::Chat, "Alice: hello")));
    assert!(!w.has(2, NoteKind::Chat, "<b>"));
    for i in 0..10 {
        b.chat(&format!("spam {i}"));
    }
    w.pump(&mut [&mut a, &mut b], |w, _| w.has(2, NoteKind::Log, "too fast"));
    let delivered = w.notes[1].iter().filter(|n| n.kind == NoteKind::Chat && n.text.contains("Bob: spam")).count();
    assert!(delivered <= 6, "{delivered}");
}

#[test]
fn admin_login_and_rules_sync() {
    let mut w = World::new("secret");
    let mut a = w.join("Alice", "pw");
    w.pump(&mut [&mut a], |w, c| w.host.players().count() == 2 && c[0].players().count() == 2);
    a.command("rule PVP 1");
    w.pump(&mut [&mut a], |w, _| w.has(1, NoteKind::Log, "admin only"));
    a.command("login wrong");
    w.pump(&mut [&mut a], |w, _| w.has(1, NoteKind::Log, "admin login failed"));
    a.command("login secret");
    w.pump(&mut [&mut a], |w, _| w.has(1, NoteKind::Log, "you are now an admin"));
    a.chat("/rule PVP 1");
    w.pump(&mut [&mut a], |_, c| c[0].rules().flag(Id::PVP));
    assert!(w.host.rules().flag(Id::PVP));
    assert!(w.has(1, NoteKind::Chat, "Alice set PVP = 1"));
    let saved = std::fs::read_to_string(w.dir.join("mp_rules.ini")).unwrap();
    assert!(saved.contains("PVP = 1"));
    a.command("password hijack");
    w.pump(&mut [&mut a], |w, _| w.has(1, NoteKind::Log, "host only"));
    w.host.command("rules_save pvp");
    w.host.command("rules_reset");
    w.pump(&mut [&mut a], |_, c| !c[0].rules().flag(Id::PVP));
    w.host.command("rules_load pvp");
    w.pump(&mut [&mut a], |_, c| c[0].rules().flag(Id::PVP));
}

#[test]
fn kick_ban_unban() {
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    w.pump(&mut [&mut a], |w, c| w.host.players().count() == 2 && c[0].players().count() == 2);
    w.host.command("kick ali spam");
    let alive = w.pump(&mut [&mut a], |w, c| w.host.players().count() == 1 && c[0].players().count() == 0);
    assert!(!alive[0]);
    assert!(w.has(1, NoteKind::Log, "kicked: spam"));
    let mut a = w.join("Alice", "pw");
    w.pump(&mut [&mut a], |w, _| w.host.players().count() == 2);
    w.host.command("ban id:2 griefing");
    w.pump(&mut [&mut a], |w, _| w.host.players().count() == 1 && w.has(2, NoteKind::Log, "banned: griefing"));
    let mut again = w.join("Alice", "pw");
    let alive = w.pump(&mut [&mut again], |w, _| w.has(3, NoteKind::Log, "banned: griefing"));
    assert!(!alive[0]);
    assert!(std::fs::read_to_string(w.dir.join("mp_banlist.txt")).unwrap().contains("ip:127.0.0.1"));
    w.host.command("unban Alice");
    w.host.drain_notes(&mut w.notes[0]);
    assert!(w.has(0, NoteKind::Log, "unbanned Alice"));
    let mut back = w.join("Alice", "pw");
    w.pump(&mut [&mut back], |w, c| w.host.players().count() == 2 && c[0].players().count() == 2);
    w.host.command("kick @a");
    w.host.drain_notes(&mut w.notes[0]);
    assert!(w.notes[0].iter().any(|n| n.level == Level::Warning && n.text.contains("cannot kick the host")));
}

#[test]
fn poses_relay_through_host() {
    use crate::state::{LimbPose, Snapshot, flag};
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    let mut b = w.join("Bob", "pw");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().count() == 3 && c.iter().all(|s| s.players().count() == 3));
    let (aid, bid) = (a.local_id(), b.local_id());
    let mut sa = Snapshot::default();
    sa.state.x = 100.0;
    sa.state.y = -20.0;
    sa.state.flags = flag::IN_WORLD | flag::ALIVE;
    sa.state.limb_count = 15;
    for i in 0..15 {
        sa.limbs[i] = LimbPose { x: 100.0 + i as f32 * 0.25, y: -20.0, rot: i as f32 * 10.0 };
    }
    let mut sh = Snapshot::default();
    sh.state.x = 5.0;
    sh.state.flags = flag::IN_WORLD | flag::STANDING;
    a.set_local_state(Some(sa));
    w.host.set_local_state(Some(sh));
    w.pump(&mut [&mut a, &mut b], |w, c| {
        c[1].remote_state(aid).is_some() && c[0].remote_state(0).is_some() && w.host.remote_state(aid).is_some()
    });
    std::thread::sleep(std::time::Duration::from_millis(250));
    w.pump(&mut [&mut a, &mut b], |_, _| true);
    let seen = b.remote_state(aid).unwrap();
    assert!((seen.state.x - 100.0).abs() < 0.01);
    assert_eq!(seen.state.limb_count, 15);
    assert!((seen.limbs[14].x - 103.5).abs() < 0.01);
    assert!((a.remote_state(0).unwrap().state.x - 5.0).abs() < 0.01);
    assert!(b.remote_state(bid).is_none());
    drop(a);
    w.pump(&mut [&mut b], |_, c| c[0].remote_state(aid).is_none());
}

#[test]
fn world_params_edits_and_late_join() {
    use crate::world::TileEdit;
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    w.pump(&mut [&mut a], |w, c| w.host.players().count() == 2 && c[0].players().count() == 2);
    let g1 = w.host.world_begin(b"seed-1".to_vec());
    w.host.world_ready(g1);
    w.pump(&mut [&mut a], |_, c| c[0].world_params().1 == b"seed-1");
    assert_eq!(a.world_params().0, g1);
    w.host.tile_edit(TileEdit { x: 10, y: 20, block: 0 });
    w.host.tile_edit(TileEdit { x: 10, y: 21, block: 3 });
    w.pump(&mut [&mut a], |_, c| c[0].world.snapshot().len() == 2);
    let mut buf = [TileEdit::default(); 64];
    assert_eq!(a.drain_tiles(&mut buf), 0, "edits must wait until the client finished generating");
    a.world_ready(g1);
    assert_eq!(a.drain_tiles(&mut buf), 2);
    a.tile_edit(TileEdit { x: 5, y: 5, block: 7 });
    w.pump(&mut [&mut a], |w, _| w.host.world.snapshot().len() == 3);
    let n = w.host.drain_tiles(&mut buf);
    assert_eq!(n, 1);
    assert_eq!(buf[0], TileEdit { x: 5, y: 5, block: 7 });
    let mut late = w.join("Late", "pw");
    w.pump(&mut [&mut a, &mut late], |_, c| c[1].world_params().0 == g1 && c[1].world.snapshot().len() == 3);
    late.world_ready(g1);
    let n = late.drain_tiles(&mut buf);
    let mut got: Vec<TileEdit> = buf[..n].to_vec();
    got.sort_by_key(|e| (e.x, e.y));
    assert_eq!(got, vec![TileEdit { x: 5, y: 5, block: 7 }, TileEdit { x: 10, y: 20, block: 0 }, TileEdit { x: 10, y: 21, block: 3 }]);
    a.tile_edit(TileEdit { x: 1, y: 1, block: 1 });
    w.pump(&mut [&mut a, &mut late], |_, c| c[1].world.snapshot().len() == 4);
    assert_eq!(late.drain_tiles(&mut buf), 1);
    let g2 = w.host.world_begin(b"seed-2".to_vec());
    w.host.world_ready(g2);
    w.pump(&mut [&mut a, &mut late], |_, c| c.iter().all(|s| s.world_params().0 == g2));
    assert!(a.world.snapshot().is_empty());
    assert!(a.status().contains("generating"));
}

#[test]
fn entity_calls_route_through_host() {
    use crate::world::{CALL_PERSIST, EntityCall, METHOD_DESTROY};
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    let mut b = w.join("Bob", "pw");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().count() == 3 && c.iter().all(|s| s.players().count() == 3));
    let g1 = w.host.world_begin(b"w".to_vec());
    w.host.world_ready(g1);
    w.pump(&mut [&mut a, &mut b], |_, c| c.iter().all(|s| s.world_params().0 == g1));
    a.world_ready(g1);
    b.world_ready(g1);
    let call = |e: u32, m: u16, f: u8| EntityCall { entity: e, method: m, flags: f, seed: 42, args: vec![9] };
    a.entity_call(call(7, 3, 0));
    let mut got = [None, None, None];
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while got.iter().any(|g| g.is_none()) {
        assert!(std::time::Instant::now() < deadline);
        w.pump(&mut [&mut a, &mut b], |_, _| true);
        if got[0].is_none() { got[0] = w.host.next_entity_call(); }
        if got[1].is_none() { got[1] = a.next_entity_call(); }
        if got[2].is_none() { got[2] = b.next_entity_call(); }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert!(got.iter().all(|g| g.as_ref().unwrap() == &call(7, 3, 0)));
    w.host.entity_call(call(8, 1, CALL_PERSIST));
    w.host.entity_call(call(9, METHOD_DESTROY, 0));
    w.pump(&mut [&mut a, &mut b], |_, c| c[1].world.journal().is_empty());
    assert!(w.host.next_entity_call().is_none(), "host does not echo its own calls");
    let mut late = w.join("Late", "pw");
    w.pump(&mut [&mut a, &mut b, &mut late], |_, c| c[2].world_params().0 == g1);
    std::thread::sleep(std::time::Duration::from_millis(100));
    w.pump(&mut [&mut a, &mut b, &mut late], |_, _| true);
    assert!(late.next_entity_call().is_none());
    late.world_ready(g1);
    let first = late.next_entity_call().unwrap();
    let second = late.next_entity_call().unwrap();
    assert_eq!((first.entity, second.entity), (8, 9));
    assert!(late.next_entity_call().is_none());
}

#[test]
fn motion_relays_to_other_clients_only() {
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    let mut b = w.join("Bob", "pw");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().count() == 3 && c.iter().all(|s| s.players().count() == 3));
    let g1 = w.host.world_begin(b"w".to_vec());
    w.host.world_ready(g1);
    w.pump(&mut [&mut a, &mut b], |_, c| c.iter().all(|s| s.world_params().0 == g1));
    a.world_ready(g1);
    b.world_ready(g1);
    a.send_motion(b"fly");
    let mut got_b = None;
    let mut got_h = None;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while got_b.is_none() || got_h.is_none() {
        assert!(std::time::Instant::now() < deadline);
        w.pump(&mut [&mut a, &mut b], |_, _| true);
        got_b = got_b.or_else(|| b.next_motion());
        got_h = got_h.or_else(|| w.host.next_motion());
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    assert_eq!(&got_b.unwrap()[5..], b"fly");
    assert_eq!(&got_h.unwrap()[5..], b"fly");
    assert!(a.next_motion().is_none());
}

#[test]
fn shared_stats_pool_and_stash() {
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    w.pump(&mut [&mut a], |w, c| w.host.players().count() == 2 && c[0].players().count() == 2);
    assert!(w.host.needs_sync([80.0, 80.0, 80.0]).is_none());
    w.host.command("rule SharedStats 1");
    w.host.command("rule SharedStash 1");
    w.pump(&mut [&mut a], |_, c| c[0].rules().flag(Id::SharedStats) && c[0].rules().flag(Id::SharedStash));
    assert_eq!(w.host.needs_sync([80.0, 70.0, 60.0]), Some([80.0, 70.0, 60.0]));
    w.pump(&mut [&mut a], |_, c| c[0].mods.pool.is_some());
    let start = a.needs_sync([50.0, 50.0, 50.0]).unwrap();
    assert_eq!(start, [80.0, 70.0, 60.0]);
    let after_eat = a.needs_sync([start[0] + 30.0, start[1] - 10.0, start[2]]).unwrap();
    assert!((after_eat[0] - 110.0).abs() < 0.01);
    w.pump(&mut [&mut a], |w, _| w.host.mods.pool.is_some_and(|p| (p[0] - 110.0).abs() < 0.01 && (p[1] - 65.0).abs() < 0.01));
    let host_view = w.host.needs_sync([80.0, 70.0, 60.0]).unwrap();
    assert!((host_view[0] - 110.0).abs() < 0.01, "{host_view:?}");
    a.stash_deposit("bandage", 0.75);
    w.pump(&mut [&mut a], |w, c| w.host.stash().len() == 1 && c[0].stash().len() == 1);
    let id = a.stash()[0].id;
    assert_eq!(a.stash()[0].by, "Alice");
    a.stash_take(id);
    w.pump(&mut [&mut a], |w, c| w.host.stash().is_empty() && c[0].stash().is_empty() && !c[0].mods.grants.is_empty());
    assert_eq!(a.take_grants(), vec![("bandage".to_owned(), 0.75)]);
    assert!(std::fs::read_to_string(w.dir.join("mp_stash.txt")).unwrap().is_empty());
    w.host.command("rule SharedStash 0");
    w.pump(&mut [&mut a], |_, c| !c[0].rules().flag(Id::SharedStash));
    a.stash_deposit("rag", 1.0);
    w.pump(&mut [&mut a], |w, _| w.has(1, NoteKind::Log, "stash is disabled"));
}

#[test]
fn file_transfer_reaches_everyone() {
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    let mut b = w.join("Bob", "pw");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().count() == 3 && c.iter().all(|s| s.players().count() == 3));
    let ad = w.dir.join("a");
    let bd = w.dir.join("b");
    std::fs::create_dir_all(&ad).unwrap();
    std::fs::create_dir_all(&bd).unwrap();
    a.data_dir = Some(ad.clone());
    b.data_dir = Some(bd.clone());
    let big: Vec<u8> = (0..700_000u32).map(|i| (i * 31 % 251) as u8).collect();
    w.host.send_file("save.sv", big.clone()).unwrap();
    w.pump(&mut [&mut a, &mut b], |_, c| c.iter().all(|s| !s.received_files().is_empty()));
    assert_eq!(std::fs::read(ad.join("mp_received").join("Host_save.sv")).unwrap(), big);
    assert_eq!(std::fs::read(bd.join("mp_received").join("Host_save.sv")).unwrap(), big);
    a.send_file("../evil name.sv", b"tiny".to_vec()).unwrap();
    w.pump(&mut [&mut a, &mut b], |w, c| c[1].received_files().len() == 2 && !w.host.received_files().is_empty());
    assert_eq!(std::fs::read(bd.join("mp_received").join("Alice_file")).unwrap(), b"tiny");
    assert!(w.dir.join("mp_received").join("Alice_file").exists());
}

fn pump_all(list: &mut [&mut Session], secs: u64, until: impl Fn(&[&mut Session]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(secs);
    let mut sink = Vec::new();
    loop {
        for s in list.iter_mut() {
            s.tick();
            s.drain_notes(&mut sink);
        }
        if until(list) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn host_crash_migrates_to_lowest_id() {
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    let mut b = w.join("Bob", "pw");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().count() == 3 && c.iter().all(|s| s.players().count() == 3));
    let g1 = w.host.world_begin(b"seed-m".to_vec());
    w.host.world_ready(g1);
    w.pump(&mut [&mut a, &mut b], |_, c| c.iter().all(|s| s.world_params().0 == g1 && s.successors.len() == 2));
    let (ida, idb) = (a.local_id(), b.local_id());
    assert!(ida < idb);
    w.host.kill();
    pump_all(&mut [&mut a.s, &mut b.s], 30, |l| l[0].is_host() && l[0].players().count() == 2 && l[1].host_id() == l[0].local_id() && l[1].players().count() == 2);
    assert_eq!(b.local_id(), idb);
    assert_eq!(a.world_params().0, g1);
    assert_eq!(b.world_params().0, g1);
    a.chat("hello after migration");
    pump_all(&mut [&mut a.s, &mut b.s], 5, |l| l[1].players().any(|p| p.name == "Alice"));
}

#[test]
fn manual_host_transfer_keeps_ids() {
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    let mut b = w.join("Bob", "pw");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().count() == 3 && c.iter().all(|s| s.players().count() == 3));
    let g1 = w.host.world_begin(b"seed-t".to_vec());
    w.host.world_ready(g1);
    w.pump(&mut [&mut a, &mut b], |_, c| c.iter().all(|s| s.world_params().0 == g1 && s.successors.len() == 2));
    let (ida, idb) = (a.local_id(), b.local_id());
    assert!(w.host.transfer_host(idb).is_ok());
    let host = &mut w.host;
    pump_all(&mut [host, &mut a.s, &mut b.s], 15, |l| {
        l[2].is_host() && l[2].players().count() == 3 && l.iter().all(|s| s.host_id() == idb && s.players().count() == 3)
    });
    assert_eq!(w.host.local_id(), 0);
    assert!(!w.host.is_host());
    assert_eq!(a.local_id(), ida);
    assert_eq!(a.world_params().0, g1);
    assert_eq!(w.host.world_params().0, g1);
    assert!(b.transfer_host(0).is_ok());
    let host = &mut w.host;
    pump_all(&mut [host, &mut a.s, &mut b.s], 15, |l| l[0].is_host() && l.iter().all(|s| s.host_id() == 0 && s.players().count() == 3));
}

#[test]
fn voice_relays_with_sender_id_rewritten() {
    use crate::voice::{self, Capture, STREAM_VOICE, Tx};
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    let mut b = w.join("Bob", "pw");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().count() == 3 && c.iter().all(|s| s.players().count() == 3));
    let ida = a.local_id();
    let mut tx = Tx::new(false, 16000).unwrap();
    let pcm: Vec<f32> = (0..voice::FRAME * 5).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
    let mut pkts = Vec::new();
    tx.feed(Capture { rate: voice::RATE, samples: &pcm, gain: 1.0, open: false, threshold: 0.0, flags: 0 }, 999, STREAM_VOICE, &mut pkts);
    assert_eq!(pkts.len(), 5);
    a.send_voice(&pkts);
    w.pump(&mut [&mut a, &mut b], |_, _| voice::with_mixer(|m| m.active().contains(&(ida, STREAM_VOICE))));
    assert!(!voice::with_mixer(|m| m.active().contains(&(999, STREAM_VOICE))));
}

#[test]
fn rename_reaches_everyone_and_stays_unique() {
    let mut w = World::new("");
    let mut a = w.join("Alice", "pw");
    let mut b = w.join("Bob", "pw");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().count() == 3 && c.iter().all(|s| s.players().count() == 3));
    a.command("name Zed");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().any(|p| p.name == "Zed") && c.iter().all(|s| s.players().any(|p| p.name == "Zed")));
    b.command("name Zed");
    w.pump(&mut [&mut a, &mut b], |w, c| w.host.players().any(|p| p.name.starts_with("Zed") && p.name != "Zed") && c[0].players().filter(|p| p.name.starts_with("Zed")).count() == 2);
    assert_eq!(a.my_name, "Zed");
    assert!(w.host.rules().flag(Id::Collision));
    assert!(!w.host.rules().flag(Id::FriendlyFire));
}
