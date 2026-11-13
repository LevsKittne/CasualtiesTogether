use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, Ordering};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyNameTextW, MAPVK_VK_TO_VSC, MapVirtualKeyW};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bind {
    Menu = 0,
    Chat = 1,
    Cpr = 2,
    Stash = 3,
    Ptt = 4,
    Spectate = 5,
}

pub const ALL: [(Bind, &str, &str, u16); 6] = [
    (Bind::Menu, "menu", "Multiplayer menu", 0x70),
    (Bind::Chat, "chat", "Open chat", b'Y' as u16),
    (Bind::Cpr, "cpr", "CPR on nearest player", b'V' as u16),
    (Bind::Stash, "stash", "Shared stash", b'I' as u16),
    (Bind::Ptt, "ptt", "Push to talk", b'G' as u16),
    (Bind::Spectate, "spectate", "Spectate next player", b'N' as u16),
];

static KEYS: Mutex<[u16; 6]> = Mutex::new([0x70, b'Y' as u16, b'V' as u16, b'I' as u16, b'G' as u16, b'N' as u16]);
static PATH: Mutex<Option<PathBuf>> = Mutex::new(None);
pub static CAPTURE: AtomicI32 = AtomicI32::new(-1);

pub fn load(dir: Option<PathBuf>) {
    let Some(dir) = dir else { return };
    let path = dir.join("ct_keybinds.ini");
    if let Ok(text) = std::fs::read_to_string(&path) {
        let mut keys = KEYS.lock().unwrap();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let Ok(vk) = v.trim().parse::<u16>() else { continue };
            if let Some(i) = ALL.iter().position(|(_, name, _, _)| *name == k.trim()) {
                keys[i] = vk;
            }
        }
    }
    *PATH.lock().unwrap() = Some(path);
}

fn save() {
    let Some(path) = PATH.lock().unwrap().clone() else { return };
    let keys = *KEYS.lock().unwrap();
    let text: String = ALL.iter().enumerate().map(|(i, (_, name, _, _))| format!("{name} = {}\r\n", keys[i])).collect();
    let _ = std::fs::write(path, text);
}

pub fn key(b: Bind) -> u16 {
    KEYS.lock().unwrap()[b as usize]
}

pub fn find(vk: u16) -> Option<Bind> {
    let keys = *KEYS.lock().unwrap();
    ALL.iter().enumerate().find(|(i, _)| keys[*i] == vk).map(|(_, (b, _, _, _))| *b)
}

pub fn assign(b: Bind, vk: u16) {
    let mut keys = KEYS.lock().unwrap();
    for k in keys.iter_mut() {
        if *k == vk {
            *k = 0;
        }
    }
    keys[b as usize] = vk;
    drop(keys);
    save();
}

pub fn reset() {
    *KEYS.lock().unwrap() = ALL.map(|a| a.3);
    save();
}

pub fn capture(b: Option<Bind>) {
    CAPTURE.store(b.map_or(-1, |b| b as i32), Ordering::SeqCst);
}

pub fn capturing() -> Option<Bind> {
    match CAPTURE.load(Ordering::SeqCst) {
        0 => Some(Bind::Menu),
        1 => Some(Bind::Chat),
        2 => Some(Bind::Cpr),
        3 => Some(Bind::Stash),
        4 => Some(Bind::Ptt),
        5 => Some(Bind::Spectate),
        _ => None,
    }
}

pub fn name(vk: u16) -> String {
    if vk == 0 {
        return "none".into();
    }
    if (0x70..=0x87).contains(&vk) {
        return format!("F{}", vk - 0x6F);
    }
    unsafe {
        let scan = MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC);
        let mut buf = [0u16; 32];
        let extended = matches!(vk, 0x21..=0x28 | 0x2D | 0x2E) as i32;
        let n = GetKeyNameTextW(((scan as i32) << 16) | (extended << 24), &mut buf);
        if n > 0 { String::from_utf16_lossy(&buf[..n as usize]) } else { format!("key {vk}") }
    }
}
