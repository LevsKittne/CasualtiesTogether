use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows::Win32::Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, OpenClipboard};
use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use super::binds::{self, Bind};
use super::{CHAT_OPEN, INPUT_BLOCKED, STASH_OPEN, VISIBLE, WANTS_KEYBOARD, WANTS_POINTER};
use std::sync::atomic::AtomicBool;

static SKIP_CHAR: AtomicBool = AtomicBool::new(false);

static OLD_PROC: AtomicIsize = AtomicIsize::new(0);
static PPP: AtomicU32 = AtomicU32::new(0x3F80_0000);
pub static EVENTS: Mutex<Vec<egui::Event>> = Mutex::new(Vec::new());
const CF_UNICODETEXT: u32 = 13;

pub fn set_pixels_per_point(ppp: f32) {
    PPP.store(ppp.to_bits(), Ordering::Relaxed);
}

pub unsafe fn install(hwnd: HWND) {
    if OLD_PROC.load(Ordering::SeqCst) != 0 {
        return;
    }
    let old = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, wndproc as *const () as isize) };
    OLD_PROC.store(old, Ordering::SeqCst);
}

pub fn take_events() -> Vec<egui::Event> {
    std::mem::take(&mut *EVENTS.lock().unwrap_or_else(|e| e.into_inner()))
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let swallow = std::panic::catch_unwind(|| handle(msg, wparam.0, lparam.0)).unwrap_or(false);
    if swallow {
        return LRESULT(0);
    }
    let old: WNDPROC = unsafe { std::mem::transmute(OLD_PROC.load(Ordering::SeqCst)) };
    unsafe { CallWindowProcW(old, hwnd, msg, wparam, lparam) }
}

fn push(e: egui::Event) {
    EVENTS.lock().unwrap_or_else(|e| e.into_inner()).push(e);
}

fn modifiers() -> egui::Modifiers {
    let down = |vk: VIRTUAL_KEY| unsafe { GetKeyState(vk.0 as i32) } < 0;
    let ctrl = down(VK_CONTROL);
    egui::Modifiers { alt: down(VK_MENU), ctrl, shift: down(VK_SHIFT), mac_cmd: false, command: ctrl }
}

fn pos(lparam: isize) -> egui::Pos2 {
    let ppp = f32::from_bits(PPP.load(Ordering::Relaxed));
    let x = (lparam & 0xFFFF) as u16 as i16 as f32;
    let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as f32;
    egui::pos2(x / ppp, y / ppp)
}

fn key(vk: u16) -> Option<egui::Key> {
    use egui::Key::*;
    Some(match VIRTUAL_KEY(vk) {
        VK_RETURN => Enter,
        VK_ESCAPE => Escape,
        VK_BACK => Backspace,
        VK_TAB => Tab,
        VK_DELETE => Delete,
        VK_LEFT => ArrowLeft,
        VK_RIGHT => ArrowRight,
        VK_UP => ArrowUp,
        VK_DOWN => ArrowDown,
        VK_HOME => Home,
        VK_END => End,
        VK_A => A,
        VK_C => C,
        VK_V => V,
        VK_X => X,
        VK_Z => Z,
        _ => return None,
    })
}

fn clipboard() -> Option<String> {
    unsafe {
        OpenClipboard(None).ok()?;
        let text = GetClipboardData(CF_UNICODETEXT).ok().and_then(|h| {
            let mem = HGLOBAL(h.0);
            let p = GlobalLock(mem) as *const u16;
            if p.is_null() {
                return None;
            }
            let mut len = 0;
            while *p.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
            let _ = GlobalUnlock(mem);
            Some(s)
        });
        let _ = CloseClipboard();
        text
    }
}

fn handle(msg: u32, wparam: usize, lparam: isize) -> bool {
    if msg == WM_CHAR && SKIP_CHAR.swap(false, Ordering::SeqCst) {
        return true;
    }
    if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
        let vk = wparam as u16;
        if let Some(b) = binds::capturing() {
            if vk != VK_ESCAPE.0 {
                binds::assign(b, vk);
            }
            binds::capture(None);
            SKIP_CHAR.store(true, Ordering::SeqCst);
            return true;
        }
        let typing = WANTS_KEYBOARD.load(Ordering::Relaxed) || INPUT_BLOCKED.load(Ordering::Relaxed);
        if !typing && (lparam >> 30) & 1 == 0 {
            if let Some(b) = binds::find(vk) {
                match b {
                    Bind::Menu => {
                        VISIBLE.fetch_xor(true, Ordering::SeqCst);
                    }
                    Bind::Chat => {
                        CHAT_OPEN.store(true, Ordering::SeqCst);
                    }
                    Bind::Cpr => crate::ui_action("cpr".into()),
                    Bind::Stash => {
                        STASH_OPEN.fetch_xor(true, Ordering::SeqCst);
                    }
                    Bind::Ptt => {}
                    Bind::Spectate => crate::ui_action("spectate".into()),
                }
                SKIP_CHAR.store(true, Ordering::SeqCst);
                return true;
            }
        }
    }
    if !VISIBLE.load(Ordering::Relaxed) && !CHAT_OPEN.load(Ordering::Relaxed) && !STASH_OPEN.load(Ordering::Relaxed) {
        return false;
    }
    let ptr = WANTS_POINTER.load(Ordering::Relaxed);
    let kb = WANTS_KEYBOARD.load(Ordering::Relaxed);
    let button = |b: egui::PointerButton, pressed: bool| {
        push(egui::Event::PointerButton { pos: pos(lparam), button: b, pressed, modifiers: modifiers() });
        ptr
    };
    match msg {
        WM_MOUSEMOVE => {
            push(egui::Event::PointerMoved(pos(lparam)));
            ptr
        }
        WM_LBUTTONDOWN | WM_LBUTTONDBLCLK => button(egui::PointerButton::Primary, true),
        WM_LBUTTONUP => button(egui::PointerButton::Primary, false),
        WM_RBUTTONDOWN | WM_RBUTTONDBLCLK => button(egui::PointerButton::Secondary, true),
        WM_RBUTTONUP => button(egui::PointerButton::Secondary, false),
        WM_MBUTTONDOWN => button(egui::PointerButton::Middle, true),
        WM_MBUTTONUP => button(egui::PointerButton::Middle, false),
        WM_MOUSEWHEEL => {
            let delta = ((wparam >> 16) as u16 as i16) as f32 / 120.0;
            push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, delta),
                phase: egui::TouchPhase::Move,
                modifiers: modifiers(),
            });
            ptr
        }
        WM_CHAR => {
            if let Some(c) = char::from_u32(wparam as u32) {
                if !c.is_control() {
                    push(egui::Event::Text(c.to_string()));
                }
            }
            kb
        }
        WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP => {
            let pressed = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
            let m = modifiers();
            if pressed && m.ctrl && wparam as u16 == VK_V.0 && kb {
                if let Some(text) = clipboard() {
                    push(egui::Event::Paste(text));
                }
                return true;
            }
            if let Some(k) = key(wparam as u16) {
                push(egui::Event::Key { key: k, physical_key: None, pressed, repeat: false, modifiers: m });
                if pressed && k == egui::Key::Escape && !kb {
                    VISIBLE.store(false, Ordering::SeqCst);
                    CHAT_OPEN.store(false, Ordering::SeqCst);
                    STASH_OPEN.store(false, Ordering::SeqCst);
                    return true;
                }
            }
            kb && pressed
        }
        WM_INPUT => ptr || kb,
        _ => false,
    }
}
