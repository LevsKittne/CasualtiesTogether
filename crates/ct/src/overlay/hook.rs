use core::ffi::c_void;
use retour::RawDetour;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HRESULT, Interface, w};

type PresentFn = unsafe extern "system" fn(*mut c_void, u32, u32) -> HRESULT;

static ORIG_PRESENT: AtomicUsize = AtomicUsize::new(0);
static DETOUR: OnceLock<SendDetour> = OnceLock::new();

struct SendDetour(#[allow(dead_code)] RawDetour);
unsafe impl Send for SendDetour {}
unsafe impl Sync for SendDetour {}

pub fn install() -> Result<(), String> {
    if DETOUR.get().is_some() {
        return Ok(());
    }
    let present = unsafe { find_present()? };
    let detour = unsafe { RawDetour::new(present as *const (), present_hook as *const ()) }.map_err(|e| format!("detour: {e}"))?;
    unsafe { detour.enable() }.map_err(|e| format!("enable detour: {e}"))?;
    ORIG_PRESENT.store(detour.trampoline() as *const () as usize, Ordering::SeqCst);
    let _ = DETOUR.set(SendDetour(detour));
    Ok(())
}

unsafe fn find_present() -> Result<usize, String> {
    unsafe {
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("ct-overlay-probe"),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            8,
            8,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| format!("probe window: {e}"))?;
        let result = probe(hwnd);
        let _ = DestroyWindow(hwnd);
        result
    }
}

unsafe fn probe(hwnd: HWND) -> Result<usize, String> {
    let desc = DXGI_SWAP_CHAIN_DESC {
        BufferDesc: DXGI_MODE_DESC { Width: 8, Height: 8, Format: DXGI_FORMAT_R8G8B8A8_UNORM, ..Default::default() },
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 1,
        OutputWindow: hwnd,
        Windowed: true.into(),
        SwapEffect: DXGI_SWAP_EFFECT_DISCARD,
        Flags: 0,
    };
    let mut swapchain: Option<IDXGISwapChain> = None;
    let mut device: Option<ID3D11Device> = None;
    unsafe {
        D3D11CreateDeviceAndSwapChain(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            None,
            D3D11_SDK_VERSION,
            Some(&desc),
            Some(&mut swapchain),
            Some(&mut device),
            None,
            None,
        )
        .map_err(|e| format!("probe device: {e}"))?;
        let sc = swapchain.ok_or("no probe swapchain")?;
        let vtable = *(sc.as_raw() as *const *const usize);
        Ok(*vtable.add(8))
    }
}

unsafe extern "system" fn present_hook(this: *mut c_void, sync: u32, flags: u32) -> HRESULT {
    let _ = std::panic::catch_unwind(|| unsafe { super::frame(this) });
    let orig: PresentFn = unsafe { std::mem::transmute(ORIG_PRESENT.load(Ordering::SeqCst)) };
    unsafe { orig(this, sync, flags) }
}
