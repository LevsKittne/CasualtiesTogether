mod backup;
pub mod binds;
mod hook;
mod input;
mod ui;

use core::ffi::c_void;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::IDXGISwapChain;
use windows::core::Interface;

pub static VISIBLE: AtomicBool = AtomicBool::new(false);
pub static CHAT_OPEN: AtomicBool = AtomicBool::new(false);
pub static STASH_OPEN: AtomicBool = AtomicBool::new(false);
pub static INPUT_BLOCKED: AtomicBool = AtomicBool::new(false);
pub static WANTS_POINTER: AtomicBool = AtomicBool::new(false);
pub static WANTS_KEYBOARD: AtomicBool = AtomicBool::new(false);

struct State {
    ctx: egui::Context,
    renderer: egui_directx11::Renderer,
    start: Instant,
    ui: ui::UiState,
}

unsafe impl Send for State {}

static STATE: Mutex<Option<State>> = Mutex::new(None);

pub fn init(data_dir: Option<std::path::PathBuf>) -> Result<(), String> {
    crate::voice::load_settings(data_dir.clone());
    binds::load(data_dir);
    hook::install()
}

fn view_format(f: DXGI_FORMAT) -> Option<DXGI_FORMAT> {
    match f {
        DXGI_FORMAT_R8G8B8A8_TYPELESS | DXGI_FORMAT_R8G8B8A8_UNORM_SRGB => Some(DXGI_FORMAT_R8G8B8A8_UNORM),
        DXGI_FORMAT_B8G8R8A8_TYPELESS | DXGI_FORMAT_B8G8R8A8_UNORM_SRGB => Some(DXGI_FORMAT_B8G8R8A8_UNORM),
        DXGI_FORMAT_R10G10B10A2_TYPELESS => Some(DXGI_FORMAT_R10G10B10A2_UNORM),
        _ => None,
    }
}

pub(crate) unsafe fn frame(raw: *mut c_void) {
    let Some(sc) = (unsafe { IDXGISwapChain::from_raw_borrowed(&raw) }) else { return };
    let Ok(device) = (unsafe { sc.GetDevice::<ID3D11Device>() }) else { return };
    let Ok(context) = (unsafe { device.GetImmediateContext() }) else { return };
    let Ok(desc) = (unsafe { sc.GetDesc() }) else { return };
    let Ok(back) = (unsafe { sc.GetBuffer::<ID3D11Texture2D>(0) }) else { return };
    let mut tex = D3D11_TEXTURE2D_DESC::default();
    unsafe { back.GetDesc(&mut tex) };
    let mut rtv: Option<ID3D11RenderTargetView> = None;
    let made = match view_format(tex.Format) {
        Some(f) => {
            let vd = D3D11_RENDER_TARGET_VIEW_DESC {
                Format: f,
                ViewDimension: D3D11_RTV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_RENDER_TARGET_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_RTV { MipSlice: 0 } },
            };
            unsafe { device.CreateRenderTargetView(&back, Some(&vd), Some(&mut rtv)) }
        }
        None => unsafe { device.CreateRenderTargetView(&back, None, Some(&mut rtv)) },
    };
    let (Ok(()), Some(rtv)) = (made, rtv) else { return };

    let mut guard = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let Ok(renderer) = egui_directx11::Renderer::new(&device) else { return };
        let ctx = egui::Context::default();
        ctx.set_visuals(egui::Visuals::dark());
        unsafe { input::install(desc.OutputWindow) };
        *guard = Some(State { ctx, renderer, start: Instant::now(), ui: ui::UiState::new() });
    }
    let st = guard.as_mut().unwrap();
    let ppp = (tex.Height as f32 / 1080.0).clamp(1.0, 2.5);
    st.ctx.set_pixels_per_point(ppp);
    input::set_pixels_per_point(ppp);
    let raw_input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(tex.Width as f32 / ppp, tex.Height as f32 / ppp))),
        time: Some(st.start.elapsed().as_secs_f64()),
        events: input::take_events(),
        focused: true,
        ..Default::default()
    };
    let ui_state = &mut st.ui;
    let output = st.ctx.run_ui(raw_input, |ui| ui::draw(ui.ctx(), ui_state));
    let ui_active = VISIBLE.load(Ordering::Relaxed) || CHAT_OPEN.load(Ordering::Relaxed) || STASH_OPEN.load(Ordering::Relaxed);
    WANTS_POINTER.store(ui_active && (st.ctx.egui_wants_pointer_input() || st.ctx.is_pointer_over_egui()), Ordering::Relaxed);
    WANTS_KEYBOARD.store(ui_active && st.ctx.egui_wants_keyboard_input(), Ordering::Relaxed);
    let (renderer_output, _, _) = egui_directx11::split_output(output);
    let saved = unsafe { backup::Backup::capture(&context) };
    let _ = st.renderer.render(&context, &rtv, &st.ctx, renderer_output);
    unsafe { saved.restore(&context) };
}
