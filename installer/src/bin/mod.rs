#![windows_subsystem = "windows"]

use installer::i18n::{fill, text};
use installer::*;

const TITLE: &str = "Casualties: Together Setup";

const FILES: &[(&str, &[u8])] = &[
    ("CasualtiesTogether.dll", include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../dist/ModLoader/mods/CasualtiesTogether/CasualtiesTogether.dll"))),
    ("ct.dll", include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../dist/ModLoader/mods/CasualtiesTogether/ct.dll"))),
];

fn main() {
    let t = text();
    let Some((game, local)) = find_game() else {
        error(TITLE, t.not_found);
        return;
    };
    let shown = game.display().to_string();
    if !local && !ask(TITLE, &fill(t.ask_mod, &[&shown])) {
        return;
    }
    let loader = game.join("version.dll").is_file() && game.join("ModLoader").join("core").join("ModLoader.dll").is_file();
    if !loader {
        error(TITLE, &fill(t.no_loader, &[&shown]));
        return;
    }
    let dir = game.join("ModLoader").join("mods").join("CasualtiesTogether");
    let update = dir.join("CasualtiesTogether.dll").is_file();
    if let Err(e) = write_all(&dir, FILES) {
        error(TITLE, &fill(t.write_failed, &[&e]));
        return;
    }
    info(TITLE, &fill(t.mod_done, &[if update { t.updated } else { t.installed }, &dir.display().to_string()]));
    self_delete();
}
