//! Notification-area (tray) icon with a small menu, plus balloon
//! notifications, written directly against Win32 (copied from Neo). It runs on GPUI's own
//! message loop (same thread), so there's no extra event loop or thread.
//! Windows 10/11 shows the balloons as regular toasts titled "Poro" (from the
//! exe's version info), with no installer or app registration needed.

use std::sync::{Mutex, OnceLock};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_LARGE_ICON, NIIF_USER,
    NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NIN_BALLOONUSERCLICK, NIN_SELECT,
    NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, GetCursorPos,
    GetSystemMetrics, IsIconic, LoadImageW, PostMessageW, RegisterClassW, RegisterWindowMessageW,
    SetForegroundWindow, SetMenuDefaultItem, ShowWindow, TrackPopupMenu, HICON, IMAGE_ICON, LR_DEFAULTCOLOR,
    MF_SEPARATOR, MF_STRING, SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, SW_HIDE, SW_RESTORE, SW_SHOW,
    TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CONTEXTMENU, WM_NULL, WNDCLASSW,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrayEvent {
    /// Left click on the icon, "Open Poro" in the menu, or a notification clicked.
    Open,
    Quit,
    /// Explorer restarted; the icon must be added again.
    Readd,
}

/// Where tray clicks go; replaced each time the icon is created (the
/// "Exit into tray" setting can be turned off and on again).
static EVENTS: Mutex<Option<async_channel::Sender<TrayEvent>>> = Mutex::new(None);
static TASKBAR_CREATED: OnceLock<u32> = OnceLock::new();

const WM_TRAY: u32 = WM_APP + 1;
/// Keyboard "select" on the icon (NIN_SELECT | NINF_KEY).
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
const ID_OPEN: i32 = 1;
const ID_QUIT: i32 = 2;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn copy_wide(dst: &mut [u16], s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..w.len()].copy_from_slice(&w);
    dst[w.len()] = 0;
}

fn send(event: TrayEvent) {
    if let Some(tx) = EVENTS.lock().ok().and_then(|g| g.clone()) {
        let _ = tx.try_send(event);
    }
}

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    big_icon: HICON,
}

impl Tray {
    /// Creates the icon. Must be called on the GPUI (main) thread.
    pub fn new() -> Option<(Tray, async_channel::Receiver<TrayEvent>)> {
        let (tx, rx) = async_channel::unbounded();
        *EVENTS.lock().ok()? = Some(tx);
        unsafe {
            let hinst = GetModuleHandleW(std::ptr::null());
            let class = wide("PoroTrayWindow");
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(wndproc);
            wc.hInstance = hinst;
            wc.lpszClassName = class.as_ptr();
            RegisterClassW(&wc);
            // A hidden top-level window (not message-only) so it also hears
            // the "TaskbarCreated" broadcast when Explorer restarts.
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                return None;
            }
            let _ = TASKBAR_CREATED.set(RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()));
            let load = |cx, cy| {
                LoadImageW(hinst, 1usize as *const u16, IMAGE_ICON, GetSystemMetrics(cx), GetSystemMetrics(cy), LR_DEFAULTCOLOR)
                    as HICON
            };
            let tray = Tray { hwnd, icon: load(SM_CXSMICON, SM_CYSMICON), big_icon: load(SM_CXICON, SM_CYICON) };
            tray.add();
            Some((tray, rx))
        }
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
        d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        d.hWnd = self.hwnd;
        d.uID = 1;
        d
    }

    /// Adds the icon (again, after Explorer restarts).
    pub fn add(&self) {
        let mut d = self.data();
        d.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
        d.uCallbackMessage = WM_TRAY;
        d.hIcon = self.icon;
        copy_wide(&mut d.szTip, "Poro");
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &d);
            d.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            Shell_NotifyIconW(NIM_SETVERSION, &d);
        }
    }

    /// Shows a notification. Clicking it sends `TrayEvent::Open`.
    pub fn notify(&self, title: &str, body: &str) {
        let mut d = self.data();
        d.uFlags = NIF_INFO;
        copy_wide(&mut d.szInfoTitle, title);
        copy_wide(&mut d.szInfo, body);
        d.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
        d.hBalloonIcon = self.big_icon;
        unsafe {
            Shell_NotifyIconW(NIM_MODIFY, &d);
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let d = self.data();
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &d);
            DestroyWindow(self.hwnd);
        }
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_TRAY {
        match (lparam as u32) & 0xffff {
            NIN_SELECT | NIN_KEYSELECT | NIN_BALLOONUSERCLICK => send(TrayEvent::Open),
            WM_CONTEXTMENU => show_menu(hwnd),
            _ => {}
        }
        return 0;
    }
    if TASKBAR_CREATED.get() == Some(&msg) {
        send(TrayEvent::Readd);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

unsafe fn show_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    AppendMenuW(menu, MF_STRING, ID_OPEN as usize, wide("Open Poro").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    AppendMenuW(menu, MF_STRING, ID_QUIT as usize, wide("Quit Poro").as_ptr());
    SetMenuDefaultItem(menu, ID_OPEN as u32, 0);
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    // Required so the menu closes when clicking elsewhere.
    SetForegroundWindow(hwnd);
    let cmd = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON, pt.x, pt.y, 0, hwnd, std::ptr::null());
    PostMessageW(hwnd, WM_NULL, 0, 0);
    DestroyMenu(menu);
    match cmd {
        ID_OPEN => send(TrayEvent::Open),
        ID_QUIT => send(TrayEvent::Quit),
        _ => {}
    }
}

/// The native handle of a GPUI window, as an integer (so it's `Send`).
pub fn hwnd_of(window: &gpui::Window) -> Option<isize> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

pub fn hide_window(hwnd: isize) {
    unsafe {
        ShowWindow(hwnd as HWND, SW_HIDE);
    }
}

pub fn show_window(hwnd: isize) {
    unsafe {
        let h = hwnd as HWND;
        ShowWindow(h, if IsIconic(h) != 0 { SW_RESTORE } else { SW_SHOW });
        SetForegroundWindow(h);
    }
}
