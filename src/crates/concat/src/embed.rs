// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Clips Kitty contributors

//! `--embed`: the editor drawn inside another program's window.
//!
//! Clips Kitty Editor runs as the Editor page of the Clips Kitty desktop
//! app, and the person must never see a second window. So, on Windows and
//! only when asked for with `--embed <HWND>`, the window is created as a
//! borderless child of the host's window (winit's parent-window support,
//! which gives it `WS_CHILD` from the start): no title bar, no taskbar
//! entry, and it moves, hides and closes with the host.
//!
//! The host steers it through the window's own API server (the JSON-RPC
//! lines on loopback, with a token), which in this mode always runs, with a
//! few methods of the host's own beside the API's:
//!
//! - `host.set_bounds {x, y, width, height}`: where the editor sits, in
//!   physical pixels of the host window's client area;
//! - `host.set_visible {visible}`: shown or hidden, as the host switches
//!   pages;
//! - `host.focus`: the keyboard to the editor; `host.blur`: back to the
//!   host's window (a click on the host's page does not take it back);
//! - `host.close`: the editor's own close (the project is saved first);
//! - `host.info`: the editor's window, its parent and where it is.
//!
//! The editor's own file dialogs are owned by the host's top-level window,
//! so they open over it and never as a window of their own on the taskbar.
//! When the host's window goes away the editor closes itself.
//!
//! Without `--embed` nothing here runs and nothing changes.

use std::net::SocketAddr;
use std::sync::OnceLock;

/// A rectangle in physical pixels of the host window's client area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// Left edge.
    pub x: i32,
    /// Top edge.
    pub y: i32,
    /// Width, at least 1.
    pub width: i32,
    /// Height, at least 1.
    pub height: i32,
}

impl Bounds {
    /// `x,y,width,height`, in whole pixels; a width or height under 1 is
    /// refused.
    pub fn parse(text: &str) -> Option<Bounds> {
        let parts: Vec<i32> = text
            .split(',')
            .map(|part| part.trim().parse::<i32>())
            .collect::<Result<_, _>>()
            .ok()?;
        let [x, y, width, height] = parts[..] else {
            return None;
        };
        (width >= 1 && height >= 1).then_some(Bounds {
            x,
            y,
            width,
            height,
        })
    }
}

/// What `--embed` asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Embed {
    /// The host window the editor is a child of.
    pub parent: isize,
    /// Where it starts, when the host said.
    pub bounds: Option<Bounds>,
    /// Where the API listens for the host; loopback on a free port when
    /// not given.
    pub api: Option<SocketAddr>,
}

/// A window handle as `--embed` takes it: decimal, or hexadecimal with
/// `0x`. Zero is no window.
pub fn parse_window(text: &str) -> Option<isize> {
    let text = text.trim();
    let value = match text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
    {
        Some(hex) => u64::from_str_radix(hex, 16).ok()?,
        None => text.parse::<u64>().ok()?,
    };
    (value != 0).then_some(value as isize)
}

static EMBED: OnceLock<Embed> = OnceLock::new();

/// Whether this run is embedded in a host's window.
pub fn active() -> bool {
    EMBED.get().is_some()
}

/// The embedding this run was started with, if any.
pub fn get() -> Option<&'static Embed> {
    EMBED.get()
}

/// Where the API listens for the host.
pub fn api_address() -> SocketAddr {
    EMBED
        .get()
        .and_then(|embed| embed.api)
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)))
}

/// The token the host gave in `CONCAT_EMBED_TOKEN`, if it gave one; the
/// server mints one otherwise and [`announce`] says it.
pub fn token() -> Option<String> {
    std::env::var("CONCAT_EMBED_TOKEN")
        .ok()
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

/// Says where the host's API is, on standard output, as one JSON line:
/// what a host that did not choose the port or the token reads. A host
/// that started the editor with no pipe there loses nothing but this.
pub fn announce(address: Option<SocketAddr>, token: &str) {
    use std::io::Write;
    let mut line = serde_json::json!({
        "embed": "ready",
        "api": address.map(|address| address.to_string()),
        "pid": std::process::id(),
    });
    if self::token().is_none() {
        line["token"] = serde_json::Value::String(token.to_owned());
    }
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Records `--embed` for the run. Refused off Windows, and when the
/// window named is not one.
pub fn set(embed: Embed) -> Result<(), String> {
    #[cfg(windows)]
    {
        // SAFETY: IsWindow takes any value and says whether it is a window.
        if unsafe { win::IsWindow(embed.parent) } == 0 {
            return Err(format!(
                "--embed names window {:#x}, which does not exist",
                embed.parent
            ));
        }
        if let Some(bounds) = embed.bounds {
            imp::set_bounds(bounds);
        }
        let parent = embed.parent;
        EMBED
            .set(embed)
            .map_err(|_| "--embed was given twice".to_owned())?;
        log::info!("embed: inside window {parent:#x}, as its child");
        imp::watch_parent(parent);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = embed;
        Err("--embed is only on Windows".to_owned())
    }
}

/// The attributes of the editor's window when embedded: a child of the
/// host's window, with no frame, no shadow and no taskbar entry (a child
/// window never has one).
#[cfg(windows)]
pub fn window_attributes(
    attributes: slint::winit_030::winit::window::WindowAttributes,
) -> slint::winit_030::winit::window::WindowAttributes {
    use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
    use slint::winit_030::winit::raw_window_handle::{RawWindowHandle, Win32WindowHandle};
    let attributes = attributes
        .with_decorations(false)
        .with_undecorated_shadow(false);
    let Some(parent) = EMBED
        .get()
        .and_then(|embed| std::num::NonZeroIsize::new(embed.parent))
    else {
        return attributes;
    };
    let handle = RawWindowHandle::Win32(Win32WindowHandle::new(parent));
    // SAFETY: the handle names a window that existed when `set` checked it,
    // and winit only passes it to CreateWindowExW as the parent; a parent
    // that has gone since makes the creation fail, which is reported.
    unsafe { attributes.with_parent_window(Some(handle)) }
}

/// The editor's window is up: remember it and put it where the host said.
/// Called with the first event the window gets.
#[cfg(not(target_os = "android"))]
pub fn attach(window: &slint::winit_030::winit::window::Window) {
    #[cfg(windows)]
    {
        use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        if let Ok(handle) = window.window_handle()
            && let RawWindowHandle::Win32(handle) = handle.as_raw()
        {
            imp::attach(handle.hwnd.get());
        }
    }
    #[cfg(not(windows))]
    let _ = window;
}

/// The editor's window was resized to `width` × `height` physical pixels:
/// if that is not what the host asked for - Slint sizing the window to its
/// preferred size as it opens - put it back.
pub fn resized(width: u32, height: u32) {
    #[cfg(windows)]
    imp::resized(width, height);
    #[cfg(not(windows))]
    let _ = (width, height);
}

/// A press in the editor: the keyboard comes with it. A child window does
/// not take the focus by itself, and the host's own window takes it back
/// as it is activated, so it is asked again shortly after while the pointer
/// is still over the editor.
pub fn pressed() {
    #[cfg(windows)]
    imp::take_focus_after_press();
}

/// The host's top-level window, as the owner of the editor's dialogs, so
/// they open over the host and not as separate windows.
#[cfg(windows)]
pub fn dialog_owner() -> Option<DialogOwner> {
    let parent = EMBED.get()?.parent;
    // SAFETY: GetAncestor takes any value; a window that is gone gives 0.
    let root = unsafe { win::GetAncestor(parent, win::GA_ROOT) };
    std::num::NonZeroIsize::new(if root != 0 { root } else { parent }).map(DialogOwner)
}

/// A window that owns a dialog: the host's, when embedded.
#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
pub struct DialogOwner(std::num::NonZeroIsize);

#[cfg(windows)]
impl slint::winit_030::winit::raw_window_handle::HasWindowHandle for DialogOwner {
    fn window_handle(
        &self,
    ) -> Result<
        slint::winit_030::winit::raw_window_handle::WindowHandle<'_>,
        slint::winit_030::winit::raw_window_handle::HandleError,
    > {
        use slint::winit_030::winit::raw_window_handle::{
            RawWindowHandle, Win32WindowHandle, WindowHandle,
        };
        let raw = RawWindowHandle::Win32(Win32WindowHandle::new(self.0));
        // SAFETY: the handle is only read by the dialog, which is shown and
        // gone while the host's window - the editor's own parent - is up.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

#[cfg(windows)]
impl slint::winit_030::winit::raw_window_handle::HasDisplayHandle for DialogOwner {
    fn display_handle(
        &self,
    ) -> Result<
        slint::winit_030::winit::raw_window_handle::DisplayHandle<'_>,
        slint::winit_030::winit::raw_window_handle::HandleError,
    > {
        Ok(slint::winit_030::winit::raw_window_handle::DisplayHandle::windows())
    }
}

/// The host's own methods on the API, `host.*`, answered here rather than
/// by the API. See the module's documentation.
pub fn host_methods() -> concat_server::HostMethods {
    concat_server::HostMethods::new(|method, params| {
        #[cfg(windows)]
        {
            imp::call(method, params)
        }
        #[cfg(not(windows))]
        {
            let _ = params;
            Err(format!("{method} is only for an embedded editor on Windows"))
        }
    })
}

/// The Win32 calls this needs, declared here rather than pulling a crate in.
#[cfg(windows)]
#[allow(non_snake_case, clippy::upper_case_acronyms)]
mod win {
    pub type HWND = isize;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct RECT {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct POINT {
        pub x: i32,
        pub y: i32,
    }

    pub const GA_PARENT: u32 = 1;
    pub const GA_ROOT: u32 = 2;
    pub const HWND_TOP: HWND = 0;
    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const SWP_ASYNCWINDOWPOS: u32 = 0x4000;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn IsWindow(hwnd: HWND) -> i32;
        pub fn IsWindowVisible(hwnd: HWND) -> i32;
        pub fn IsChild(parent: HWND, hwnd: HWND) -> i32;
        pub fn SetWindowPos(
            hwnd: HWND,
            after: HWND,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
        pub fn GetAncestor(hwnd: HWND, flags: u32) -> HWND;
        pub fn SetFocus(hwnd: HWND) -> HWND;
        pub fn GetFocus() -> HWND;
        pub fn GetForegroundWindow() -> HWND;
        pub fn GetWindowRect(hwnd: HWND, rect: *mut RECT) -> i32;
        pub fn ScreenToClient(hwnd: HWND, point: *mut POINT) -> i32;
        pub fn GetCursorPos(point: *mut POINT) -> i32;
        pub fn WindowFromPoint(point: POINT) -> HWND;
    }
}

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
    use std::time::Duration;

    use serde_json::{Value, json};

    use super::{Bounds, EMBED, win};

    /// The editor's own window, once it is up; 0 before.
    static CHILD: AtomicIsize = AtomicIsize::new(0);
    /// Where the host wants it.
    static BOUNDS: Mutex<Option<Bounds>> = Mutex::new(None);
    /// Whether the host wants it shown.
    static VISIBLE: AtomicBool = AtomicBool::new(true);
    /// How many times a resize that was not the host's was undone: a cap,
    /// so a window and a host that disagree cannot ping-pong for ever.
    static UNDONE: AtomicU32 = AtomicU32::new(0);

    pub fn set_bounds(bounds: Bounds) {
        if let Ok(mut held) = BOUNDS.lock() {
            *held = Some(bounds);
        }
    }

    fn bounds() -> Option<Bounds> {
        BOUNDS.lock().ok().and_then(|held| *held)
    }

    /// Puts the window where the host wants it, on top of the host's other
    /// children (Chromium raises its own over it on a maximise or a
    /// restore), without activating anything. Asynchronous, so it can be
    /// called from any thread and never waits on the window's.
    fn place() {
        let child = CHILD.load(Ordering::SeqCst);
        if child == 0 {
            return;
        }
        let flags = win::SWP_NOACTIVATE | win::SWP_ASYNCWINDOWPOS;
        // SAFETY: plain Win32 calls on a window handle; a handle gone stale
        // makes them fail, nothing more.
        unsafe {
            match bounds() {
                Some(bounds) => win::SetWindowPos(
                    child,
                    win::HWND_TOP,
                    bounds.x,
                    bounds.y,
                    bounds.width.max(1),
                    bounds.height.max(1),
                    flags,
                ),
                None => win::SetWindowPos(
                    child,
                    win::HWND_TOP,
                    0,
                    0,
                    0,
                    0,
                    flags | win::SWP_NOMOVE | win::SWP_NOSIZE,
                ),
            };
        }
    }

    pub fn attach(hwnd: isize) {
        let first = CHILD.swap(hwnd, Ordering::SeqCst) == 0;
        if first {
            let parent = EMBED.get().map(|embed| embed.parent).unwrap_or_default();
            // SAFETY: GetAncestor takes any value.
            let actual = unsafe { win::GetAncestor(hwnd, win::GA_PARENT) };
            log::info!(
                "embed: the editor's window {hwnd:#x} is up, a child of {actual:#x} (asked for {parent:#x})"
            );
        }
        place();
        if !VISIBLE.load(Ordering::SeqCst) {
            apply_visible(false);
        }
    }

    pub fn resized(width: u32, height: u32) {
        let Some(bounds) = bounds() else {
            return;
        };
        if CHILD.load(Ordering::SeqCst) == 0
            || (bounds.width as u32 == width && bounds.height as u32 == height)
        {
            return;
        }
        if UNDONE.fetch_add(1, Ordering::SeqCst) < 64 {
            log::debug!(
                "embed: resized to {width}x{height}, the host asked for {}x{}; put back",
                bounds.width,
                bounds.height
            );
            place();
        }
    }

    /// Shows or hides the window, through winit so its idea of the window
    /// stays the window's. On the event loop's thread.
    fn apply_visible(visible: bool) {
        let done = slint::invoke_from_event_loop(move || {
            if !visible {
                give_focus_back();
            }
            crate::host::Shell::with(|_, app| {
                use slint::winit_030::WinitWindowAccessor;
                slint::ComponentHandle::window(&app).with_winit_window(|window| {
                    window.set_visible(visible);
                });
            });
            if visible {
                place();
            }
        });
        if done.is_err() {
            log::warn!("embed: wanted the editor shown or hidden, and its event loop is gone");
        }
    }

    /// The keyboard to the editor, on the event loop's thread.
    fn focus() {
        let _ = slint::invoke_from_event_loop(|| {
            let child = CHILD.load(Ordering::SeqCst);
            if child != 0 {
                // SAFETY: SetFocus on this thread's own window.
                unsafe { win::SetFocus(child) };
            }
        });
    }

    /// The keyboard back to the host's top-level window, when the editor has
    /// it. A click on the host's own page does not move it: the host's
    /// window was active all along, so it sees no reason to take the focus,
    /// and keys went on reaching the editor. This thread shares the host's
    /// input queue (a child window's thread always does), so it can hand the
    /// focus over. On the event loop's thread.
    fn give_focus_back() {
        let child = CHILD.load(Ordering::SeqCst);
        if child == 0 {
            return;
        }
        // SAFETY: plain Win32 calls on this thread's own window and its root.
        unsafe {
            let focused = win::GetFocus();
            if focused == child || (focused != 0 && win::IsChild(child, focused) != 0) {
                let root = win::GetAncestor(child, win::GA_ROOT);
                if root != 0 {
                    win::SetFocus(root);
                }
            }
        }
    }

    fn blur() {
        let _ = slint::invoke_from_event_loop(give_focus_back);
    }

    /// Whether the pointer is over the editor's window (or something in it).
    fn pointer_over(child: isize) -> bool {
        let mut point = win::POINT::default();
        // SAFETY: plain Win32 reads.
        unsafe {
            if win::GetCursorPos(&mut point) == 0 {
                return false;
            }
            let under = win::WindowFromPoint(point);
            under == child || win::IsChild(child, under) != 0
        }
    }

    pub fn take_focus_after_press() {
        let child = CHILD.load(Ordering::SeqCst);
        if child == 0 {
            return;
        }
        // SAFETY: SetFocus on this thread's own window; this runs on the
        // event loop's thread, from the window's own events.
        unsafe {
            if win::GetFocus() != child {
                win::SetFocus(child);
            }
        }
        // The host's window, activated by this same press, hands the focus
        // to its own content a moment later; take it back while the pointer
        // is still here - not once it has gone to the host's page.
        for delay in [150u64, 400] {
            slint::Timer::single_shot(Duration::from_millis(delay), move || {
                let child = CHILD.load(Ordering::SeqCst);
                // SAFETY: as above, on the event loop's thread.
                unsafe {
                    if child != 0 && win::GetFocus() != child && pointer_over(child) {
                        let root = win::GetAncestor(child, win::GA_ROOT);
                        if win::GetForegroundWindow() == root {
                            win::SetFocus(child);
                        }
                    }
                }
            });
        }
    }

    /// Closes the editor the way the system's close does: the project is
    /// saved, then the loop ends.
    fn close(reason: &'static str) {
        let _ = slint::invoke_from_event_loop(move || {
            log::info!("embed: closing, {reason}");
            let mut found = None;
            crate::host::Shell::with(|_, app| found = Some(app));
            if let Some(app) = found {
                slint::ComponentHandle::window(&app)
                    .dispatch_event(slint::platform::WindowEvent::CloseRequested);
            }
        });
    }

    /// Ends the editor when the host's window goes: the project is saved and
    /// the process leaves, without Slint's close (its hide reaches winit,
    /// which panics on the destroyed window: seen on run 44021141732's
    /// build). Should the event loop not take the errand, the process
    /// leaves a little later anyway.
    pub fn watch_parent(parent: isize) {
        let started = std::thread::Builder::new()
            .name("embed parent watch".to_owned())
            .spawn(move || {
                loop {
                    std::thread::sleep(Duration::from_millis(250));
                    // SAFETY: IsWindow takes any value.
                    if unsafe { win::IsWindow(parent) } == 0 {
                        break;
                    }
                }
                log::warn!("embed: the host's window {parent:#x} is gone; the editor closes");
                CHILD.store(0, Ordering::SeqCst);
                let _ = slint::invoke_from_event_loop(|| {
                    crate::embedded_host_gone();
                });
                std::thread::sleep(Duration::from_secs(10));
                log::warn!("embed: the event loop did not end after the host went; exiting");
                std::process::exit(0);
            });
        if let Err(error) = started {
            log::warn!("embed: could not watch the host's window: {error}");
        }
    }

    fn number(params: &Value, key: &str) -> Result<i32, String> {
        params
            .get(key)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && value.abs() < 1.0e6)
            .map(|value| value.round() as i32)
            .ok_or_else(|| format!("{key} is a number of pixels"))
    }

    fn info() -> Value {
        let child = CHILD.load(Ordering::SeqCst);
        let parent = EMBED.get().map(|embed| embed.parent).unwrap_or_default();
        let mut rect = win::RECT::default();
        let mut at = win::POINT::default();
        // SAFETY: plain Win32 reads of window state.
        let (visible, actual_parent, placed) = unsafe {
            let placed = child != 0 && win::GetWindowRect(child, &mut rect) != 0 && {
                at.x = rect.left;
                at.y = rect.top;
                win::ScreenToClient(parent, &mut at) != 0
            };
            (
                child != 0 && win::IsWindowVisible(child) != 0,
                if child != 0 {
                    win::GetAncestor(child, win::GA_PARENT)
                } else {
                    0
                },
                placed,
            )
        };
        json!({
            "hwnd": child,
            "parent": parent,
            "actualParent": actual_parent,
            "visible": visible,
            "wantVisible": VISIBLE.load(Ordering::SeqCst),
            "bounds": placed.then(|| json!({
                "x": at.x,
                "y": at.y,
                "width": rect.right - rect.left,
                "height": rect.bottom - rect.top,
            })),
            "wantBounds": bounds().map(|bounds| json!({
                "x": bounds.x,
                "y": bounds.y,
                "width": bounds.width,
                "height": bounds.height,
            })),
        })
    }

    pub fn call(method: &str, params: &Value) -> Result<Value, String> {
        match method {
            "host.set_bounds" => {
                let bounds = Bounds {
                    x: number(params, "x")?,
                    y: number(params, "y")?,
                    width: number(params, "width")?,
                    height: number(params, "height")?,
                };
                if bounds.width < 1 || bounds.height < 1 {
                    return Err(
                        "width and height are at least 1; hide the editor instead".to_owned(),
                    );
                }
                set_bounds(bounds);
                UNDONE.store(0, Ordering::SeqCst);
                place();
                Ok(json!({}))
            }
            "host.set_visible" => {
                let visible = params
                    .get("visible")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| "visible is true or false".to_owned())?;
                VISIBLE.store(visible, Ordering::SeqCst);
                if CHILD.load(Ordering::SeqCst) != 0 {
                    apply_visible(visible);
                }
                Ok(json!({}))
            }
            "host.focus" => {
                focus();
                Ok(json!({}))
            }
            "host.blur" => {
                blur();
                Ok(json!({}))
            }
            "host.close" => {
                close("as the host asked");
                Ok(json!({}))
            }
            "host.info" => Ok(info()),
            other => Err(format!(
                "{other} is not a host method: host.set_bounds, host.set_visible, host.focus, host.blur, host.close or host.info"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_handle_is_decimal_or_hexadecimal_and_never_zero() {
        assert_eq!(parse_window("12345"), Some(12345));
        assert_eq!(parse_window("0x1A2b"), Some(0x1a2b));
        assert_eq!(parse_window("0X10"), Some(16));
        assert_eq!(parse_window("0"), None);
        assert_eq!(parse_window("0x"), None);
        assert_eq!(parse_window("-5"), None);
        assert_eq!(parse_window("window"), None);
    }

    #[test]
    fn bounds_are_four_whole_numbers_with_a_size() {
        assert_eq!(
            Bounds::parse("220,0,1164,861"),
            Some(Bounds {
                x: 220,
                y: 0,
                width: 1164,
                height: 861
            })
        );
        assert_eq!(
            Bounds::parse(" -4, 8 ,10,20"),
            Some(Bounds {
                x: -4,
                y: 8,
                width: 10,
                height: 20
            })
        );
        assert_eq!(Bounds::parse("1,2,0,5"), None);
        assert_eq!(Bounds::parse("1,2,3"), None);
        assert_eq!(Bounds::parse("1,2,3,4,5"), None);
        assert_eq!(Bounds::parse("a,b,c,d"), None);
    }
}
