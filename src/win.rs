//! Windows helpers for the in-game overlay: find the League game window,
//! make the overlay window click-through and always on top, and keep it
//! lined up with the game.

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    pub type Hwnd = isize;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// The League game window (not the client/lobby), if a game is open.
    pub fn game_window() -> Option<Hwnd> {
        let class = wide("RiotWindowClass");
        let title = wide("League of Legends (TM) Client");
        let h = unsafe { FindWindowW(class.as_ptr(), title.as_ptr()) };
        (!h.is_null()).then_some(h as Hwnd)
    }

    /// The game's drawing area in screen pixels: (x, y, width, height).
    pub fn client_rect(h: Hwnd) -> Option<(i32, i32, i32, i32)> {
        let hwnd = h as HWND;
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        let mut p = POINT { x: 0, y: 0 };
        unsafe {
            if GetClientRect(hwnd, &mut r) == 0 || ClientToScreen(hwnd, &mut p) == 0 {
                return None;
            }
            if IsIconic(hwnd) != 0 {
                return None;
            }
        }
        let (w, hgt) = (r.right - r.left, r.bottom - r.top);
        (w > 100 && hgt > 100).then_some((p.x, p.y, w, hgt))
    }

    /// A window's outer rectangle: (x, y, width, height).
    pub fn window_rect(h: Hwnd) -> Option<(i32, i32, i32, i32)> {
        let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        (unsafe { GetWindowRect(h as HWND, &mut r) } != 0).then_some((r.left, r.top, r.right - r.left, r.bottom - r.top))
    }

    /// The primary screen: (0, 0, width, height).
    pub fn primary_screen() -> (i32, i32, i32, i32) {
        unsafe { (0, 0, GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) }
    }

    pub fn foreground() -> Hwnd {
        unsafe { GetForegroundWindow() as Hwnd }
    }

    pub const VK_TAB_KEY: i32 = 0x09;

    /// Whether a key is held right now.
    pub fn key_down(vk: i32) -> bool {
        unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(vk) as u16 & 0x8000 != 0 }
    }

    /// Clicks pass through, never takes focus, stays above other windows.
    pub fn make_overlay(h: Hwnd) {
        let hwnd = h as HWND;
        unsafe {
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            let add = (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW) as isize;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | add);
            // A layered window needs this once to be drawn at all.
            SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
        }
    }

    /// Moves the overlay over `rect` and shows it, or hides it.
    pub fn place(h: Hwnd, rect: Option<(i32, i32, i32, i32)>) {
        let hwnd = h as HWND;
        unsafe {
            match rect {
                Some((x, y, w, hgt)) => {
                    SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, hgt, SWP_NOACTIVATE | SWP_SHOWWINDOW);
                }
                None => {
                    ShowWindow(hwnd, SW_HIDE);
                }
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub type Hwnd = isize;
    pub fn game_window() -> Option<Hwnd> {
        None
    }
    pub fn client_rect(_: Hwnd) -> Option<(i32, i32, i32, i32)> {
        None
    }
    pub fn primary_screen() -> (i32, i32, i32, i32) {
        (0, 0, 1920, 1080)
    }
    pub fn window_rect(_: Hwnd) -> Option<(i32, i32, i32, i32)> {
        None
    }
    pub fn foreground() -> Hwnd {
        0
    }
    pub const VK_TAB_KEY: i32 = 0x09;
    pub fn key_down(_: i32) -> bool {
        false
    }
    pub fn make_overlay(_: Hwnd) {}
    pub fn place(_: Hwnd, _: Option<(i32, i32, i32, i32)>) {}
}

pub use imp::*;
