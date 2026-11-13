pub mod i18n;

use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const GAME_EXE: &str = "CasualtiesUnknown.exe";
const GAME_DIR: &str = "Casualties Unknown";

const MB_OK: u32 = 0x0;
const MB_YESNO: u32 = 0x4;
const MB_ICONERROR: u32 = 0x10;
const MB_ICONQUESTION: u32 = 0x20;
const MB_ICONINFO: u32 = 0x40;
const IDYES: i32 = 6;
const HKEY_CURRENT_USER: isize = 0x80000001u32 as i32 as isize;
const HKEY_LOCAL_MACHINE: isize = 0x80000002u32 as i32 as isize;
const RRF_RT_REG_SZ: u32 = 0x2;
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[link(name = "user32")]
unsafe extern "system" {
    fn MessageBoxW(hwnd: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegGetValueW(key: isize, sub: *const u16, value: *const u16, flags: u32, ty: *mut u32, data: *mut c_void, len: *mut u32) -> i32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn silent() -> bool {
    std::env::args().any(|a| a.eq_ignore_ascii_case("/silent"))
}

fn msg(title: &str, text: &str, kind: u32) -> i32 {
    if silent() {
        if kind & MB_ICONERROR != 0 {
            std::process::exit(1);
        }
        return IDYES;
    }
    let (t, c) = (wide(text), wide(title));
    unsafe { MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), kind) }
}

pub fn info(title: &str, text: &str) {
    msg(title, text, MB_OK | MB_ICONINFO);
}

pub fn error(title: &str, text: &str) {
    msg(title, text, MB_OK | MB_ICONERROR);
}

pub fn ask(title: &str, text: &str) -> bool {
    msg(title, text, MB_YESNO | MB_ICONQUESTION) == IDYES
}

fn reg_string(root: isize, sub: &str, value: &str) -> Option<String> {
    let (s, v) = (wide(sub), wide(value));
    let mut buf = vec![0u16; 1024];
    let mut len = (buf.len() * 2) as u32;
    let r = unsafe { RegGetValueW(root, s.as_ptr(), v.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len) };
    if r != 0 {
        return None;
    }
    let n = (len as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..n]))
}

fn steam_libraries() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(p) = reg_string(HKEY_CURRENT_USER, "Software\\Valve\\Steam", "SteamPath") {
        roots.push(PathBuf::from(p));
    }
    if let Some(p) = reg_string(HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath") {
        roots.push(PathBuf::from(p));
    }
    let mut libs = roots.clone();
    for root in &roots {
        let Ok(text) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) else { continue };
        for line in text.lines() {
            let parts: Vec<&str> = line.split('"').collect();
            if parts.len() >= 4 && parts[1].eq_ignore_ascii_case("path") {
                libs.push(PathBuf::from(parts[3].replace("\\\\", "\\")));
            }
        }
    }
    libs
}

pub fn exe_path() -> PathBuf {
    std::env::current_exe().unwrap_or_default()
}

pub fn is_game_dir(dir: &Path) -> bool {
    dir.join(GAME_EXE).is_file()
}

pub fn find_game() -> Option<(PathBuf, bool)> {
    let exe = exe_path();
    let mut dir = exe.parent();
    while let Some(d) = dir {
        if is_game_dir(d) {
            return Some((d.to_path_buf(), true));
        }
        dir = d.parent();
    }
    steam_libraries()
        .into_iter()
        .map(|l| l.join("steamapps").join("common").join(GAME_DIR))
        .find(|d| is_game_dir(d))
        .map(|d| (d, false))
}

pub fn write_all(base: &Path, files: &[(&str, &[u8])]) -> Result<(), String> {
    for (rel, data) in files {
        let path = base.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&path, data).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

pub fn self_delete() {
    let exe = exe_path();
    let _ = Command::new("cmd")
        .raw_arg(format!("/c ping 127.0.0.1 -n 3 >nul & del /f /q \"{}\"", exe.display()))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}
