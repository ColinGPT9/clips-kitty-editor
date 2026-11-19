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
//! - `host.info`: the editor's window, its parent and where it is, and
//!   whether it draws in software;
//! - `host.state`: what the host polls: the project and whether it has
//!   unsaved changes, a running export, the exports that ended, whether
//!   the editor is shown, has the keyboard and draws in software;
//! - `host.open_project {path, exportDir, exportName, destination}`:
//!   another project, opened as File > Open does (the open one is saved
//!   and closed first).
//!
//! `--export-dir` and `--export-name` (or `host.open_project`) say where
//! the Export sheet writes and what the file is called to begin with; the
//! sheet then shows no folder row, and `--export-note` (`destination`)
//! is the line it shows there instead: where the host puts the file. `--hidden` keeps the window unshown
//! until `host.set_visible`. The host's token comes in the environment
//! (`CONCAT_EMBED_TOKEN`) and is required: none is minted or printed, and
//! nothing is listed in `automation.json`. Ctrl+F6 in the editor gives the
//! keyboard back to the host.
//!
//! The editor's own file dialogs are owned by the host's top-level window,
//! so they open over it and never as a window of their own on the taskbar.
//! When the host's window goes away the editor closes itself.
//!
//! Without `--embed` nothing here runs and nothing changes.
#![cfg_attr(not(windows), allow(dead_code))]

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

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
    /// The folder the Export sheet writes into, when the host named one
    /// (`--export-dir`): the sheet then offers no other.
    pub export_dir: Option<PathBuf>,
    /// The name the Export sheet starts with (`--export-name`).
    pub export_name: Option<String>,
    /// The line the Export sheet shows under the name, saying where the
    /// host puts the file (`--export-note`).
    pub export_note: Option<String>,
    /// Not shown until the host asks (`--hidden`).
    pub hidden: bool,
}

/// A window handle as `--embed` takes it: decimal, or hexadecimal with
/// `0x`. Zero is no window.
pub fn parse_window(text: &str) -> Option<isize> {
    let text = text.trim();
    let value = match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
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

/// The token the host gave in `CONCAT_EMBED_TOKEN`. Required: [`set`]
/// refuses to start without one that is long enough, so none is ever
/// minted or said aloud while embedded.
pub fn token() -> Option<String> {
    std::env::var("CONCAT_EMBED_TOKEN")
        .ok()
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty())
}

/// Says where the host's API is, on standard output, as one JSON line:
/// what a host that did not choose the port reads. Never the token: that
/// stays between the host and the editor's environment. A host that
/// started the editor with no pipe there loses nothing but this.
pub fn announce(address: Option<SocketAddr>) {
    use std::io::Write;
    let line = serde_json::json!({
        "embed": "ready",
        "api": address.map(|address| address.to_string()),
        "pid": std::process::id(),
    });
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Records `--embed` for the run. Refused off Windows, when the window
/// named is not one, without the host's token in the environment, and
/// for an API address that is not this machine's own.
pub fn set(embed: Embed) -> Result<(), String> {
    match token() {
        None => {
            return Err(
                "--embed needs the host's token in CONCAT_EMBED_TOKEN; none was given".to_owned(),
            );
        }
        Some(token) => concat_server::check_token(&token)
            .map_err(|why| format!("CONCAT_EMBED_TOKEN will not do: {why}"))?,
    }
    if let Some(address) = embed.api
        && !address.ip().is_loopback()
    {
        return Err(format!(
            "--api {address} is not this machine's own address; an embedded editor listens on loopback only"
        ));
    }
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
        if embed.hidden {
            imp::start_hidden();
        }
        if let Some(folder) = &embed.export_dir {
            set_export_target(
                folder.to_string_lossy().into_owned(),
                embed.export_name.clone(),
                embed.export_note.clone(),
            );
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
    use slint::winit_030::winit::dpi::{PhysicalPosition, PhysicalSize};
    use slint::winit_030::winit::platform::windows::WindowAttributesExtWindows;
    use slint::winit_030::winit::raw_window_handle::{RawWindowHandle, Win32WindowHandle};
    // Not resizable: the host sizes it, and a child wants no sizing frame.
    let mut attributes = attributes
        .with_decorations(false)
        .with_undecorated_shadow(false)
        .with_resizable(false);
    // Made where the host wants it and at that size, so it is never seen
    // anywhere else; or, for a start that shows nothing until asked
    // (`--hidden`), far outside the host's window, where a child is not
    // drawn, until `attach` has hidden it.
    if let Some(bounds) = imp::bounds() {
        attributes = attributes
            .with_inner_size(PhysicalSize::new(
                bounds.width.max(1) as u32,
                bounds.height.max(1) as u32,
            ))
            .with_position(PhysicalPosition::new(bounds.x, bounds.y));
    }
    if !imp::wants_visible() {
        attributes = attributes.with_position(PhysicalPosition::new(-32000, -32000));
    }
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
        // Not resizable, as far as winit knows: the host sizes it, and
        // winit then leaves the sizing frame (WS_THICKFRAME) off whenever
        // it writes the window's style again, and the window's edges are
        // no place to start a resize drag from (platform.rs).
        window.set_resizable(false);
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

/// A key pressed in the editor. The first Space that starts playback after
/// launch handed the keyboard to the host's window within 100 ms, with no
/// one asking, so the next Space - the pause - went to the host's page
/// (K1 tests, three runs of three; PATCHES.md, F4): if the keyboard lands
/// on the host's own window just after a key here, it is taken back.
pub fn key_pressed() {
    #[cfg(windows)]
    imp::keep_focus_after_key();
}

/// The editor's window got the keyboard, or lost it: said in the log with
/// what has it now, which is what a key that "was dropped" is traced by.
pub fn focus_changed(focused: bool) {
    #[cfg(windows)]
    imp::focus_changed(focused);
    #[cfg(not(windows))]
    let _ = focused;
}

/// F6 was pressed in the editor: with Ctrl held, the keyboard goes back to
/// the host's window - the way out of the editor for someone who uses no
/// pointer - and the key is spent. Whether it was.
pub fn leaves_on_f6() -> bool {
    #[cfg(windows)]
    {
        imp::leaves_on_f6()
    }
    #[cfg(not(windows))]
    {
        false
    }
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

/// The version of the `host.*` methods: 2 since `host.state` and
/// `host.open_project`.
pub const HOST_API: u32 = 2;

/// How many ended exports `host.state` keeps.
const EXPORTS_KEPT: usize = 20;

/// Where the host wants exports written.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ExportTarget {
    /// The folder.
    dir: String,
    /// The name the sheet starts with, until the sheet has taken it.
    name: Option<String>,
    /// Where the host puts the finished file, in the host's words; shown
    /// read-only under the name. Empty when the host said nothing.
    note: String,
}

/// What the window last said about itself, for `host.state`: written on
/// the event loop's thread each time the window publishes
/// (studio/publish.rs), read on the API's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seen {
    /// The open project's folder and name; `None` on the empty screen.
    pub project: Option<(String, String)>,
    /// An edit is not on disk yet: made, with its autosave still to run.
    pub dirty: bool,
    /// The Export sheet is rendering.
    pub exporting: bool,
    /// How far that render is, `0..=1`.
    pub fraction: f32,
    /// What it is doing, as the sheet says it.
    pub stage: String,
}

/// One export of the window's that ended.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Ended {
    seq: u64,
    state: &'static str,
    project: String,
    output: String,
    width: u32,
    height: u32,
    error: String,
}

/// Everything `host.state` says that is not the window's own to read.
#[derive(Default)]
struct Told {
    /// The project's folder as the host spelled it, so the host can match
    /// what it is told against what it asked for, letter for letter.
    given: Option<String>,
    target: Option<ExportTarget>,
    seen: Seen,
    ended: VecDeque<Ended>,
    seq: u64,
}

fn told() -> MutexGuard<'static, Told> {
    static TOLD: OnceLock<Mutex<Told>> = OnceLock::new();
    TOLD.get_or_init(|| Mutex::new(Told::default()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Saves that have been handed to a worker and have not come back.
static SAVES: AtomicU32 = AtomicU32::new(0);

/// A save was handed to a worker (studio/edit.rs).
pub fn save_began() {
    SAVES.fetch_add(1, Ordering::SeqCst);
}

/// That save came back, written or not.
pub fn save_ended() {
    let _ = SAVES.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |held| {
        Some(held.saturating_sub(1))
    });
}

/// The project this run was started on, as the host spelled it.
pub fn set_project(path: Option<&Path>) {
    told().given = path.map(|path| path.to_string_lossy().into_owned());
}

fn set_export_target(dir: String, name: Option<String>, note: Option<String>) {
    told().target = Some(ExportTarget {
        dir,
        name: name.filter(|name| !name.trim().is_empty()),
        note: note.map(|note| note.trim().to_owned()).unwrap_or_default(),
    });
}

/// The line the Export sheet shows where the folder row was: what the
/// host does with the file. Empty when the host said nothing.
pub fn export_note() -> String {
    told()
        .target
        .as_ref()
        .map(|target| target.note.clone())
        .unwrap_or_default()
}

/// Counts the projects opened while embedded: the timeline pane scrolls
/// its lanes to the first one with something on it once per opening
/// (see `timeline-open-seq` in editor.slint). A host's project opens
/// with its audio lanes under the picture, and the pane's own rule (the
/// foot of the stack in view) showed only those, empty.
static OPENED: AtomicI32 = AtomicI32::new(0);

/// A project was opened (studio/project_files.rs); nothing when not embedded.
pub fn note_opened() {
    if active() {
        OPENED.fetch_add(1, Ordering::SeqCst);
    }
}

/// How many projects have opened while embedded; 0 when not embedded.
pub fn opened() -> i32 {
    OPENED.load(Ordering::SeqCst)
}

/// The panels an embedded editor starts with: the library, the picture
/// and the inspector side by side across the top, and the timeline the
/// whole width under them. A host's page is narrower than a window of the
/// editor's own, and upstream's arrangement gives the timeline's right
/// end to the inspector, which cut off the timeline's tools in a
/// 1056-pixel area. Not remembered between starts while embedded.
pub fn embedded_dock() -> crate::dock::Dock {
    use crate::dock::Dock;
    use crate::ui::PaneKind;
    Dock::Split {
        columns: false,
        ratio: 0.6,
        first: Box::new(Dock::Split {
            columns: true,
            ratio: 0.27,
            first: Dock::leaf(PaneKind::Media),
            second: Box::new(Dock::Split {
                columns: true,
                ratio: 0.7,
                first: Dock::leaf(PaneKind::Preview),
                second: Dock::leaf(PaneKind::Inspector),
            }),
        }),
        second: Dock::leaf(PaneKind::Timeline),
    }
}

/// The narrowest the library and the inspector are let be while
/// embedded, and the least the rest of their row keeps, in logical pixels.
/// Under these the library's tabs, its Import button and the inspector's
/// tabs were cut off in a host's 1056-pixel area.
const SIDE_MIN: f32 = 300.0;
const REST_MIN: f32 = 420.0;

/// Embedded only: widens a column split's library or inspector to
/// [`SIDE_MIN`] where the area allows, taking it from the other side.
/// Never narrows what a person widened. Called when the area changes size.
pub fn fit_sides(node: &mut crate::dock::Dock, width: f32, height: f32) {
    use crate::dock::{Dock, SEAT_GAP};
    use crate::ui::PaneKind;
    let Dock::Split {
        columns,
        ratio,
        first,
        second,
    } = node
    else {
        return;
    };
    let along = if *columns { width } else { height };
    let usable = (along - SEAT_GAP).max(0.0);
    let side = |node: &Dock| {
        matches!(
            node,
            Dock::Leaf(PaneKind::Media) | Dock::Leaf(PaneKind::Inspector)
        )
    };
    if *columns && usable >= SIDE_MIN + REST_MIN {
        let floor = SIDE_MIN / usable;
        let (head_side, tail_side) = (side(&**first), side(&**second));
        if head_side && !tail_side && *ratio < floor {
            *ratio = floor;
        } else if tail_side && !head_side && *ratio > 1.0 - floor {
            *ratio = 1.0 - floor;
        }
    }
    let head = usable * *ratio;
    if *columns {
        fit_sides(first, head, height);
        fit_sides(second, usable - head, height);
    } else {
        fit_sides(first, width, head);
        fit_sides(second, width, usable - head);
    }
}

/// Whether the host named the folder exports go to: the Export sheet then
/// shows no folder row, no Browse and no "Show in folder".
pub fn export_fixed() -> bool {
    told().target.is_some()
}

/// The folder the Export sheet writes into, and - the first time it is
/// asked after the host named one - the name it starts with. `None` when
/// the host named no folder.
pub fn export_target() -> Option<(String, Option<String>)> {
    let mut told = told();
    let target = told.target.as_mut()?;
    Some((target.dir.clone(), target.name.take()))
}

/// The window says where it is; see [`Seen`].
pub fn tell(seen: Seen) {
    let mut told = told();
    if told.seen != seen {
        told.seen = seen;
    }
}

/// A folder's spelling with one kind of separator, none at its end, and
/// no capitals: how two spellings are compared when the folder itself
/// cannot be asked.
fn plain(text: &str) -> String {
    let separator = std::path::MAIN_SEPARATOR;
    text.replace(['/', separator], "/")
        .trim_end_matches('/')
        .to_lowercase()
}

/// Whether two spellings name one folder.
fn same_folder(one: &str, other: &str) -> bool {
    if one == other {
        return true;
    }
    match (std::fs::canonicalize(one), std::fs::canonicalize(other)) {
        (Ok(one), Ok(other)) => one == other,
        _ => plain(one) == plain(other),
    }
}

/// `path` as the host spelled it, when it is the folder the host named.
fn as_given(given: Option<&str>, path: &str) -> String {
    match given {
        Some(given) if same_folder(given, path) => given.to_owned(),
        _ => path.to_owned(),
    }
}

/// An export of the window's ended: `state` is `done` (the file is
/// complete under its final name), `failed` or `cancelled` (no file under
/// that name was finished). Called by the Export sheet (panes/export.rs).
pub fn export_ended(
    state: &'static str,
    project: &str,
    output: &str,
    (width, height): (u32, u32),
    error: &str,
) {
    let mut told = told();
    told.seq += 1;
    let project = as_given(told.given.as_deref(), project);
    let ended = Ended {
        seq: told.seq,
        state,
        project,
        output: output.replace('/', std::path::MAIN_SEPARATOR_STR),
        width,
        height,
        error: error.to_owned(),
    };
    log::info!(
        "embed: export {} ended, {state}: {}",
        ended.seq,
        ended.output
    );
    told.ended.push_back(ended);
    while told.ended.len() > EXPORTS_KEPT {
        told.ended.pop_front();
    }
}

/// What `host.state` answers; `visible` and `focused` are the window's,
/// and the three after them say what it draws on.
fn state(
    visible: bool,
    focused: bool,
    (software, adapter, backend): (bool, String, String),
) -> serde_json::Value {
    use serde_json::json;
    let told = told();
    let saving = SAVES.load(Ordering::SeqCst) > 0;
    json!({
        "hostApi": HOST_API,
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "visible": visible,
        "focused": focused,
        "graphics": {
            "software": software,
            "adapter": adapter,
            "backend": backend,
        },
        "project": told.seen.project.as_ref().map(|(path, name)| json!({
            "path": as_given(told.given.as_deref(), path),
            "name": name,
            "dirty": told.seen.dirty || saving,
        })),
        "export": {
            "running": told.seen.exporting,
            "fraction": if told.seen.exporting { told.seen.fraction } else { 0.0 },
            "stage": if told.seen.exporting { told.seen.stage.as_str() } else { "" },
        },
        "exports": told.ended.iter().map(|ended| json!({
            "seq": ended.seq,
            "state": ended.state,
            "project": ended.project,
            "output": ended.output,
            "width": ended.width,
            "height": ended.height,
            "error": ended.error,
        })).collect::<Vec<_>>(),
    })
}

/// What a caller is refused with while an export runs.
const BUSY: &str = "busy: an export is running; open another project when it has finished";

/// `host.open_project`: the project in the folder `path`, opened the way
/// File > Open opens one - the open project is saved and closed first -
/// with the Export sheet pointed at `exportDir` and named `exportName`.
/// Answers once the project is on screen. Refused with a sentence that
/// starts `busy:` while an export runs; refused, with the open project
/// left open, for a folder that will not read or that another process
/// holds.
#[cfg_attr(not(windows), allow(dead_code))]
fn open_project(params: &serde_json::Value) -> Result<serde_json::Value, String> {
    let text = |key: &str| {
        params
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned)
    };
    let path = text("path").ok_or_else(|| "path is the project's folder".to_owned())?;
    let export_dir = text("exportDir");
    let export_name = text("exportName");
    let destination = text("destination");
    if told().seen.exporting {
        return Err(BUSY.to_owned());
    }
    let (answer, answered) = std::sync::mpsc::channel::<Result<(), String>>();
    let given = path.clone();
    crate::host::on_ui(move |studio, _, _| {
        let switched = switch_project(studio, &given);
        if switched.is_ok() {
            if let Some(dir) = export_dir {
                set_export_target(dir, export_name, destination);
            }
            // An Export sheet left open across the switch takes the new
            // folder and name now, not only at its next opening.
            if studio.export.open
                && studio.export.phase != crate::ui::ExportPhase::Running
                && let Some((folder, name)) = export_target()
            {
                studio.export.folder = folder;
                if let Some(name) = name {
                    studio.export.name = name;
                }
            }
        }
        let _ = answer.send(switched);
    });
    match answered.recv_timeout(std::time::Duration::from_secs(60)) {
        Ok(Ok(())) => {
            told().given = Some(path);
            Ok(serde_json::json!({}))
        }
        Ok(Err(why)) => Err(why),
        Err(_) => {
            Err("the editor did not answer in time; the project may still be opening".to_owned())
        }
    }
}

/// [`open_project`], on the event loop's thread.
#[cfg_attr(not(windows), allow(dead_code))]
fn switch_project(studio: &mut crate::studio::Studio, path: &str) -> Result<(), String> {
    if studio.export.phase == crate::ui::ExportPhase::Running {
        return Err(BUSY.to_owned());
    }
    let folder = std::path::absolute(path).map_err(|error| format!("{path}: {error}"))?;
    let folder = folder
        .to_str()
        .ok_or_else(|| "the project's folder is not valid Unicode".to_owned())?
        .to_owned();
    // Read and checked before the open project is touched: a folder that
    // is no project, or one another process holds, leaves it as it was.
    concat_host::projects::open(&folder)?;
    if let Some(owner) = concat_host::lock::ProjectLock::holder(Path::new(&folder)) {
        let pid = owner
            .pid
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "?".to_owned());
        return Err(crate::i18n::tf("studio.projectOpenElsewhere", &[&pid]));
    }
    let before = studio
        .session
        .as_ref()
        .map(|session| session.path().to_owned());
    match studio.open_project_at(&folder) {
        Ok(()) => Ok(()),
        Err(why) => {
            // The open project was closed for this one, which then would
            // not open after all: back to the one that was open.
            if studio.session.is_none()
                && let Some(before) = before
                && let Err(again) = studio.open_project_at(&before)
            {
                log::error!(
                    "embed: {folder} did not open ({why}), and the project that was open would not open again: {again}"
                );
            }
            Err(why)
        }
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
            Err(format!(
                "{method} is only for an embedded editor on Windows"
            ))
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

    /// What a thread's windows are doing: which has the keyboard.
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct GUITHREADINFO {
        pub size: u32,
        pub flags: u32,
        pub active: HWND,
        pub focus: HWND,
        pub capture: HWND,
        pub menu_owner: HWND,
        pub move_size: HWND,
        pub caret: HWND,
        pub caret_rect: RECT,
    }

    pub const GWL_STYLE: i32 = -16;
    pub const WS_THICKFRAME: isize = 0x0004_0000;
    pub const WS_MAXIMIZEBOX: isize = 0x0001_0000;
    pub const SW_HIDE: i32 = 0;
    pub const VK_CONTROL: i32 = 0x11;
    pub const SWP_NOZORDER: u32 = 0x0004;
    pub const SWP_FRAMECHANGED: u32 = 0x0020;
    pub const GA_PARENT: u32 = 1;
    pub const GA_ROOT: u32 = 2;
    pub const HWND_TOP: HWND = 0;
    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const SWP_ASYNCWINDOWPOS: u32 = 0x4000;
    pub const GW_CHILD: u32 = 5;
    pub const GW_HWNDNEXT: u32 = 2;

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetWindow(hwnd: HWND, command: u32) -> HWND;
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
        pub fn GetWindowLongPtrW(hwnd: HWND, index: i32) -> isize;
        pub fn SetWindowLongPtrW(hwnd: HWND, index: i32, value: isize) -> isize;
        pub fn ShowWindow(hwnd: HWND, command: i32) -> i32;
        pub fn GetKeyState(key: i32) -> i16;
        pub fn GetWindowThreadProcessId(hwnd: HWND, process: *mut u32) -> u32;
        pub fn GetGUIThreadInfo(thread: u32, info: *mut GUITHREADINFO) -> i32;
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

    /// `--hidden`: not shown until the host asks.
    pub fn start_hidden() {
        VISIBLE.store(false, Ordering::SeqCst);
    }

    /// Takes the sizing frame off the window, when it has one: Slint asks
    /// for a resizable window, and a child the host sizes wants no frame
    /// of its own to be dragged by or measured with. On the event loop's
    /// thread.
    fn strip_frame(child: isize) {
        // SAFETY: plain Win32 calls on this thread's own window.
        unsafe {
            let style = win::GetWindowLongPtrW(child, win::GWL_STYLE);
            let wanted = style & !(win::WS_THICKFRAME | win::WS_MAXIMIZEBOX);
            if wanted != style {
                win::SetWindowLongPtrW(child, win::GWL_STYLE, wanted);
                win::SetWindowPos(
                    child,
                    0,
                    0,
                    0,
                    0,
                    0,
                    win::SWP_NOMOVE
                        | win::SWP_NOSIZE
                        | win::SWP_NOZORDER
                        | win::SWP_NOACTIVATE
                        | win::SWP_FRAMECHANGED,
                );
            }
        }
    }

    /// Whether the editor's window, or something in it, has the keyboard:
    /// asked of the window's own thread, from any thread.
    fn has_focus(child: isize) -> bool {
        if child == 0 {
            return false;
        }
        let mut info = win::GUITHREADINFO {
            size: std::mem::size_of::<win::GUITHREADINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: plain Win32 reads; the struct says its own size.
        unsafe {
            let thread = win::GetWindowThreadProcessId(child, std::ptr::null_mut());
            thread != 0
                && win::GetGUIThreadInfo(thread, &mut info) != 0
                && info.focus != 0
                && (info.focus == child || win::IsChild(child, info.focus) != 0)
        }
    }

    pub fn focus_changed(focused: bool) {
        if focused {
            log::debug!("embed: the editor has the keyboard");
            return;
        }
        // SAFETY: plain Win32 reads on the event loop's thread.
        let (now, foreground) = unsafe { (win::GetFocus(), win::GetForegroundWindow()) };
        let child = CHILD.load(Ordering::SeqCst);
        // SAFETY: GetAncestor takes any value.
        let root = unsafe { win::GetAncestor(child, win::GA_ROOT) };
        log::debug!(
            "embed: the editor lost the keyboard to window {now:#x} (the host's is {root:#x}, the foreground window {foreground:#x})"
        );
    }

    pub fn leaves_on_f6() -> bool {
        // SAFETY: GetKeyState reads this thread's own key state.
        let control = unsafe { win::GetKeyState(win::VK_CONTROL) } < 0;
        if control {
            log::info!("embed: Ctrl+F6, the keyboard goes back to the host");
            give_focus_back();
        }
        control
    }

    pub fn bounds() -> Option<Bounds> {
        BOUNDS.lock().ok().and_then(|held| *held)
    }

    /// Whether the host wants the editor shown.
    pub fn wants_visible() -> bool {
        VISIBLE.load(Ordering::SeqCst)
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
        strip_frame(hwnd);
        if first && !VISIBLE.load(Ordering::SeqCst) {
            // `--hidden`: off the screen before its first frame is, and
            // winit is told the same below so its idea of the window stays
            // the window's.
            // SAFETY: ShowWindow on this thread's own window.
            unsafe { win::ShowWindow(hwnd, win::SW_HIDE) };
        }
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
        keep_frameless();
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

    /// [`strip_frame`] on the editor's window, after anything that may
    /// have written its style again: a resize, a show.
    pub fn keep_frameless() {
        let child = CHILD.load(Ordering::SeqCst);
        if child != 0 {
            strip_frame(child);
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
            keep_frameless();
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
        GAVE_BACK.fetch_add(1, Ordering::SeqCst);
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

    /// How many times the keyboard was handed back on purpose (host.blur,
    /// Ctrl+F6, hiding): a key's watch below stands down after one.
    static GAVE_BACK: AtomicU32 = AtomicU32::new(0);

    /// See [`super::key_pressed`]. On the event loop's thread, from the
    /// window's own key event. Only the host's top-level window is watched
    /// for, and only while it is the foreground window: a dialog, another
    /// program or a click on the host's page (which never moves the
    /// keyboard; see [`give_focus_back`]) is left alone.
    pub fn keep_focus_after_key() {
        let child = CHILD.load(Ordering::SeqCst);
        // SAFETY: a plain Win32 read on this thread's own queue.
        if child == 0 || unsafe { win::GetFocus() } != child {
            return;
        }
        let gave = GAVE_BACK.load(Ordering::SeqCst);
        for delay in [60u64, 200, 500] {
            slint::Timer::single_shot(Duration::from_millis(delay), move || {
                let child = CHILD.load(Ordering::SeqCst);
                if child == 0
                    || GAVE_BACK.load(Ordering::SeqCst) != gave
                    || !VISIBLE.load(Ordering::SeqCst)
                {
                    return;
                }
                // SAFETY: plain Win32 calls on this thread's own window and
                // the root it shares an input queue with.
                unsafe {
                    let root = win::GetAncestor(child, win::GA_ROOT);
                    if root != 0 && win::GetFocus() == root && win::GetForegroundWindow() == root {
                        log::info!(
                            "embed: the keyboard went to the host's window just after a key in the editor, with nobody asking; taken back"
                        );
                        win::SetFocus(child);
                    }
                }
            });
        }
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
                let mut raised = 0u32;
                loop {
                    std::thread::sleep(Duration::from_millis(250));
                    // SAFETY: IsWindow takes any value.
                    if unsafe { win::IsWindow(parent) } == 0 {
                        break;
                    }
                    if keep_on_top(parent) {
                        raised += 1;
                        if raised <= 8 {
                            log::info!(
                                "embed: another of the host's windows was above the editor's; the editor's is on top again"
                            );
                        }
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

    /// Puts the editor's window back above the host's other child windows
    /// when one of them is on top of it while it is shown. Chromium makes a
    /// child window of its own after start (for accessibility and touch)
    /// and puts it first: drawn under the editor, it still took every click
    /// meant for the editor (seen in the K1 tests on Electron 31). Says
    /// whether it had to. Any thread: the move is asynchronous.
    fn keep_on_top(parent: isize) -> bool {
        let child = CHILD.load(Ordering::SeqCst);
        if child == 0 || !wants_visible() {
            return false;
        }
        // SAFETY: plain Win32 reads of window handles; a stale handle reads
        // as none, and the move below only fails.
        unsafe {
            if win::IsWindowVisible(child) == 0 {
                return false;
            }
            let first = win::GetWindow(parent, win::GW_CHILD);
            if first == 0 || first == child {
                return false;
            }
            // Only a window that is shown and overlaps the editor's counts.
            let mut mine = win::RECT::default();
            if win::GetWindowRect(child, &mut mine) == 0 {
                return false;
            }
            let mut above = first;
            let mut covered = false;
            let mut steps = 0;
            while above != 0 && above != child && steps < 64 {
                let mut theirs = win::RECT::default();
                if win::IsWindowVisible(above) != 0
                    && win::GetWindowRect(above, &mut theirs) != 0
                    && theirs.left < mine.right
                    && mine.left < theirs.right
                    && theirs.top < mine.bottom
                    && mine.top < theirs.bottom
                {
                    covered = true;
                    break;
                }
                above = win::GetWindow(above, win::GW_HWNDNEXT);
                steps += 1;
            }
            if !covered {
                return false;
            }
            win::SetWindowPos(
                child,
                win::HWND_TOP,
                0,
                0,
                0,
                0,
                win::SWP_NOMOVE | win::SWP_NOSIZE | win::SWP_NOACTIVATE | win::SWP_ASYNCWINDOWPOS,
            );
        }
        true
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
        // SAFETY: a plain Win32 read of the window's style.
        let style = if child != 0 {
            unsafe { win::GetWindowLongPtrW(child, win::GWL_STYLE) }
        } else {
            0
        };
        json!({
            "hwnd": child,
            "parent": parent,
            "actualParent": actual_parent,
            "software": crate::gpu::graphics().0,
            "focused": has_focus(child),
            "style": style,
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
                        "width and height are at least 1; hide the editor instead".to_owned()
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
            "host.state" => {
                let child = CHILD.load(Ordering::SeqCst);
                // SAFETY: a plain Win32 read of window state.
                let visible = child != 0 && unsafe { win::IsWindowVisible(child) } != 0;
                Ok(super::state(
                    visible,
                    has_focus(child),
                    crate::gpu::graphics(),
                ))
            }
            "host.open_project" => super::open_project(params),
            other => Err(format!(
                "{other} is not a host method: host.set_bounds, host.set_visible, host.focus, host.blur, host.close, host.info, host.state or host.open_project"
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

    /// `host.state` always has every key, `project` null on the empty
    /// screen; an ended export is listed once, numbered from 1, oldest
    /// first, and only the last twenty are kept. One test, because what
    /// it reads is the process's.
    #[test]
    fn the_state_a_host_polls_has_every_key_and_numbers_its_exports() {
        let graphics = || (false, "A GPU".to_owned(), "Direct3D 12".to_owned());
        let empty = state(false, false, graphics());
        for key in [
            "hostApi", "version", "pid", "visible", "focused", "graphics", "project", "export",
            "exports",
        ] {
            assert!(empty.get(key).is_some(), "{key} is missing");
        }
        assert_eq!(empty["hostApi"], 2);
        assert!(empty["project"].is_null());
        assert_eq!(empty["export"]["running"], false);
        assert_eq!(empty["export"]["fraction"], 0.0);
        assert_eq!(empty["export"]["stage"], "");
        assert_eq!(empty["graphics"]["software"], false);
        assert_eq!(empty["graphics"]["backend"], "Direct3D 12");
        assert_eq!(empty["exports"].as_array().map(Vec::len), Some(0));

        set_project(Some(Path::new("Projects/My clip")));
        tell(Seen {
            project: Some(("Projects/My clip".to_owned(), "My clip".to_owned())),
            dirty: true,
            exporting: true,
            fraction: 0.5,
            stage: "Rendering video".to_owned(),
        });
        let busy = state(true, true, graphics());
        assert_eq!(busy["project"]["path"], "Projects/My clip");
        assert_eq!(busy["project"]["name"], "My clip");
        assert_eq!(busy["project"]["dirty"], true);
        assert_eq!(busy["export"]["running"], true);
        assert_eq!(busy["export"]["fraction"], 0.5);
        assert_eq!(busy["visible"], true);
        assert_eq!(busy["focused"], true);

        // A save in flight keeps the project dirty after the edit's flag
        // has gone.
        tell(Seen {
            project: Some(("Projects/My clip".to_owned(), "My clip".to_owned())),
            ..Seen::default()
        });
        save_began();
        assert_eq!(state(true, true, graphics())["project"]["dirty"], true);
        save_ended();
        assert_eq!(state(true, true, graphics())["project"]["dirty"], false);
        save_ended();
        assert_eq!(state(true, true, graphics())["project"]["dirty"], false);

        export_ended(
            "done",
            "Projects/My clip",
            "out/My clip.mp4",
            (1080, 1920),
            "",
        );
        export_ended(
            "cancelled",
            "Projects/My clip",
            "out/My clip.mp4",
            (1080, 1920),
            "",
        );
        export_ended(
            "failed",
            "Projects/Other",
            "out/x.mp4",
            (1920, 1080),
            "the disk is full",
        );
        let after = state(true, false, graphics());
        let exports = after["exports"].as_array().expect("a list");
        assert_eq!(exports.len(), 3);
        assert_eq!(exports[0]["seq"], 1);
        assert_eq!(exports[0]["state"], "done");
        assert_eq!(exports[0]["project"], "Projects/My clip");
        assert_eq!(exports[0]["width"], 1080);
        assert_eq!(exports[0]["height"], 1920);
        assert_eq!(exports[1]["state"], "cancelled");
        assert_eq!(exports[2]["seq"], 3);
        assert_eq!(exports[2]["error"], "the disk is full");
        for key in [
            "seq", "state", "project", "output", "width", "height", "error",
        ] {
            assert!(exports[0].get(key).is_some(), "{key} is missing");
        }
        for _ in 0..30 {
            export_ended(
                "done",
                "Projects/My clip",
                "out/again.mp4",
                (1080, 1920),
                "",
            );
        }
        let later = state(true, false, graphics());
        let exports = later["exports"].as_array().expect("a list");
        assert_eq!(exports.len(), EXPORTS_KEPT);
        assert_eq!(exports[EXPORTS_KEPT - 1]["seq"], 33);
        assert_eq!(exports[0]["seq"], 33 - EXPORTS_KEPT as u64 + 1);

        // The folder the host names is where the sheet writes, and its
        // name is handed over once.
        assert_eq!(export_target(), None);
        assert!(!export_fixed());
        set_export_target(
            "out".to_owned(),
            Some("My clip".to_owned()),
            Some(" Replaces this clip ".to_owned()),
        );
        assert!(export_fixed());
        assert_eq!(export_note(), "Replaces this clip");
        assert_eq!(
            export_target(),
            Some(("out".to_owned(), Some("My clip".to_owned())))
        );
        assert_eq!(export_target(), Some(("out".to_owned(), None)));
    }

    #[test]
    fn an_embedded_area_keeps_room_for_the_library_and_the_inspector() {
        use crate::dock::{DockLayout, lay_out};
        use crate::ui::PaneKind;
        // A host's 1056 x 785 area: each side column at least SIDE_MIN.
        let mut dock = embedded_dock();
        fit_sides(&mut dock, 1040.0, 769.0);
        let mut out = DockLayout::default();
        lay_out(&dock, (8.0, 8.0, 1040.0, 769.0), &mut out);
        let width = |kind| {
            out.seats
                .iter()
                .find(|seat| seat.kind == kind)
                .map(|seat| seat.width)
                .unwrap()
        };
        assert!(width(PaneKind::Media) >= SIDE_MIN - 0.5);
        assert!(width(PaneKind::Inspector) >= SIDE_MIN - 0.5);
        assert!(width(PaneKind::Preview) >= 400.0);
        // The timeline is the whole width.
        assert!(width(PaneKind::Timeline) >= 1040.0 - 0.5);
        // A wider column a person chose is left as it is.
        let mut wide = embedded_dock();
        fit_sides(&mut wide, 2400.0, 1200.0);
        let mut out = DockLayout::default();
        lay_out(&wide, (0.0, 0.0, 2400.0, 1200.0), &mut out);
        let media = out
            .seats
            .iter()
            .find(|s| s.kind == PaneKind::Media)
            .unwrap();
        assert!((media.width - (2400.0 - 8.0) * 0.27).abs() < 0.5);
    }

    #[test]
    fn two_spellings_of_a_folder_are_one_folder() {
        assert!(same_folder("Projects/My clip", "Projects/My clip"));
        assert!(same_folder("Projects/My clip/", "projects/my clip"));
        assert!(!same_folder("Projects/My clip", "Projects/Other"));
        assert_eq!(as_given(Some("P/One"), "p/one/"), "P/One");
        assert_eq!(as_given(Some("P/One"), "P/Two"), "P/Two");
        assert_eq!(as_given(None, "P/Two"), "P/Two");
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
