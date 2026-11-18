# Clips Kitty Editor: what this build changes

Clips Kitty Editor is a modified build of [Concat](https://github.com/jub0t/Concat)
(AGPL-3.0-or-later), made to run as the Editor page inside the
[Clips Kitty](https://github.com/ColinGPT9/clips-studio) desktop app on Windows.
It is not Concat and is not offered as Concat: Concat's
[TRADEMARK.md](TRADEMARK.md) asks a modified build to carry another name, so
this one does. Concat's name is kept where it is a fact: the credit in
Settings > About, the licence and notice files, this file, and the source.

- Source of this build: https://github.com/ColinGPT9/clips-kitty-editor (branch `kitty/embed`)
- Built from: https://github.com/jub0t/Concat (`main` at a7cda105, 26 commits after v0.2.11)
- Licence: AGPL-3.0-or-later, unchanged. [LICENSE](LICENSE) and
  [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) ship with every build, with this file.

Everything below is on top of upstream. Without `--embed`, the editor behaves
as upstream's does, apart from the name, the icon, the colours, the data
folder and the removed update page.

## 1. The name, in as few places as possible

The brand is kept in three places so that merging upstream stays cheap:

| Place | Holds |
|---|---|
| `src/crates/concat-host/src/brand.rs` | the name, the program's file name, the data folder id, the credit, the two source links, the default accent, and two switches (no update page, no adoption of a Concat folder) |
| `src/crates/concat/ui/brand.slint` | the same name and credit as Slint literals: the window's title and the About page's credit |
| `src/crates/concat/ui/theme/dark.slint`, `light.slint` | the colours (section 3) |

What reads them:

- **Every sentence a person sees.** The 26 locale files are left exactly as
  upstream writes them. `i18n::t` (the one function every string of the
  interface comes through, Rust's and Slint's alike) replaces the word
  "Concat" with "Clips Kitty Editor" in the sentence it returns. So "Restart
  Concat?" reads "Restart Clips Kitty Editor?" in every language, and a merge
  never touches a locale.
- **The window's title** (`ui/app.slint`), the start-up failure dialog and its
  title (`platform.rs`), Settings > About's name and system report
  (`sysinfo.rs`), the build line in the log (`machine.rs`).
- **Messages made in Rust** that reach a notice: "a ... project already exists",
  "is not a ... project", "open in another ...", "saved by a newer ...", the
  microphone format message, the reason Windows is given for staying awake
  (`projects.rs`, `lock.rs`, `session.rs`, `record.rs`, `awake.rs`).
- **The default folder for new projects**: `Desktop\Clips Kitty Editor`
  (`panes/start.rs`).
- **The program**: `clips-kitty-editor.exe` (`[[bin]]` in
  `src/crates/concat/Cargo.toml`; the crate and the library keep the name
  `concat`). `--version` prints `clips-kitty-editor <version>`. Its Windows
  file properties say "Clips Kitty Editor" and credit Concat in the copyright
  line (`build.rs`).
- **The icon**: Clips Kitty's own (`assets/icons/clips-kitty-editor.ico`, and a
  256 px PNG of it for the window), in place of the Concat logo, which is not
  used anywhere in this build. The upstream logo files stay in `assets/` only
  because removing them would make every merge conflict; nothing compiles them in.

## 2. Its own data folder

`com.clipskitty.editor` in place of `app.concat.editor` (`dirs.rs` reads
`brand::IDENTIFIER`): settings in `%APPDATA%\com.clipskitty.editor`, models,
caches and logs in `%LOCALAPPDATA%\com.clipskitty.editor`. Nothing is shared
with a Concat on the same PC: not the settings, not the crash flag, not the
models. The one-time adoption of a folder left by an unreleased upstream build
(`org.standalones.concat`) is switched off, so nothing of a Concat is ever
moved or read.

Environment variable names are kept as upstream's (`CONCAT_EMBED_TOKEN`,
`CONCAT_MCP_ROOTS`, `CONCAT_GPU`, ...). No person sees them, and renaming them
would touch dozens of files on every merge.

## 3. Colours

Clips Kitty's theme, from its `theme.css`: base `#0a1628`, surface `#13243d`,
raised `#1c3354`, ink `#f1f5f9`, muted `#94a3b8`, accent `#38bdf8`.

- `dark.slint`: the surfaces (panel, raised, well, field and its two states,
  the launch ladder, lanes, ruler, page) are that navy ladder, each step kept
  where upstream's grey one was; the ink and the muted ink are Clips Kitty's;
  the palette's own playhead is the accent.
- The "Sky" accent row is Clips Kitty's blue in both palettes (`#38bdf8` on
  dark; `#0369a1` on light, where the bright blue cannot be read as ink), and
  it is the default accent (`prefs.rs` asks `brand::DEFAULT_ACCENT`). The other
  five accents, the light palette's surfaces, and every layout are upstream's.

## 4. Credit

Settings > About shows the name and version, then "Based on Concat",
"AGPL-3.0-or-later", and two links: Concat's repository and this build's own
source (`ui/dialogs/settings.slint`, reading `brand.slint`).

## 5. No calls to the upstream project's servers on the editor's behalf

Searched for: every `http://` and `https://` literal in the Rust sources, and
every caller of the network module.

| Found | What it did | In this build |
|---|---|---|
| `concat-host/src/updates.rs` (`api.github.com/repos/jub0t/Concat/releases`), reached only from Settings > Version | lists Concat's releases, downloads and installs one | **Off.** The Version page is not in Settings (it would install Concat over this editor), and `check_updates` returns at once (`brand::UPDATES` is false). The code stays for merges. |
| Launch screen links: guides, "report a problem", Discord, "what's new", release notes (`panes/start.rs`) | open a web page when clicked | "Report a problem", Discord, GitHub, "what's new" and release notes now open this build's own repository (issues, this file). The two guide links still open Concat's guides, which describe the editor's features. Embedded, the launch screen is not shown at all. |
| Model downloads (`concat-host/src/models.rs`): `github.com/jub0t/Concat/releases/download/models-v1/...`, `huggingface.co`, `hf-mirror.com`, and the model authors' own release pages | fetch a speech, cut-out, enhance or voice model when a person presses Download for that feature | **Kept, unchanged.** They are downloads a person starts, of files this editor needs to do what was asked. Settings calls the first source "Clips Kitty Editor's mirror"; it is the upstream project's GitHub release. |
| Sound library (`raw.githubusercontent.com/concatapp/assets`, and that repository's releases) | fetches the list of free sounds when a person opens the Audio page's shelf and presses Load; a sound when one is picked | **Kept, unchanged**, for the same reason. |
| Telemetry, analytics, crash upload, a "what's new" fetch, an update check at start | | **None found.** Nothing is sent anywhere without a press; the crash handler writes a local log only. |

## 6. `--embed`: the editor inside another program's window (Windows)

```
clips-kitty-editor.exe --embed <HWND, decimal or 0x hex>
        [--bounds x,y,w,h] [--api 127.0.0.1:PORT] [--project DIR]
        [--export-dir DIR] [--export-name NAME] [--hidden]
```

The window is created as a borderless child of the host's window: no title
bar, no taskbar entry; it moves, hides and closes with the host. The host
steers it over the editor's own JSON-RPC server (loopback, token) with
`host.*` methods answered beside the API's. Code: `src/crates/concat/src/embed.rs`,
with small hooks in `launch.rs`, `main.rs`, `lib.rs`, `platform.rs`,
`panes/settings.rs`, `panes/export.rs`, `studio/publish.rs`, `studio/edit.rs`,
`studio/keyframes.rs`, `gpu.rs`, `ui/app.slint`, `ui/dialogs/export.slint`,
`ui/dialogs/settings.slint`, and `HostMethods` in `concat-server`.

### Start

- The token comes in `CONCAT_EMBED_TOKEN` and is **required** (20 characters
  or more, as the server's own rule asks). Without it, with a weak one, with
  an `--api` address that is not loopback, or with options that do not parse,
  the editor writes one line to standard error and exits with code 2. It never
  shows a dialog for this, never mints a token, never prints one.
- **No `automation.json`** is written while embedded: the address and token
  stay between the host and the editor, so no other program on the PC finds
  the embedded editor's API.
- `--hidden`: the window is created outside the host's visible area and hidden
  before its first frame; it shows when the host calls `host.set_visible`.
  Without `--hidden` it is created at `--bounds`, at that size.
- The child has no sizing frame (`WS_THICKFRAME`): it is created not
  resizable, and the style bit is cleared again after anything that rewrites
  the window's style.
- **The start-up crash flag is not used while embedded.** Upstream marks each
  start and, if the process dies in its first 15 seconds, draws the next start
  in software. A host starts and stops the editor at will, so embedded the mark
  is not left, `softwareAfterCrash` is neither written nor obeyed, and
  Settings' "Draw on the GPU" switch is not shown or obeyed. A kill at any
  moment cannot put the next start in software. What the editor draws on is
  reported instead (`host.state`, `host.info`), for the host to say.

### Host methods

| Method | Does |
|---|---|
| `host.set_bounds {x, y, width, height}` | where the editor sits, in physical pixels of the host window's client area |
| `host.set_visible {visible}` | shown or hidden; hiding gives the keyboard back |
| `host.focus`, `host.blur` | the keyboard to the editor, or back to the host's window |
| `host.close` | the editor's own close: the project is saved, then the process leaves |
| `host.info` | the window, its parent, its place, its style bits, `software`, `focused` |
| `host.state` | what the host polls (below) |
| `host.open_project {path, exportDir, exportName}` | another project, opened as File > Open does: the open project is saved and closed first. Answers `{}` once it is on screen. Refused with a sentence starting `busy:` while an export runs. A folder that is not a project, or that another process holds, is refused before the open project is touched, so it stays open; if the new project fails later than that, the one that was open is opened again |

`host.state` (every key always present; `project` is null on the empty screen):

```json
{"hostApi": 2, "version": "0.2.11", "pid": 1234,
 "visible": true, "focused": false,
 "graphics": {"software": false, "adapter": "<name>", "backend": "Direct3D 12"},
 "project": {"path": "<dir>", "name": "My clip", "dirty": false},
 "export": {"running": false, "fraction": 0.0, "stage": ""},
 "exports": [{"seq": 1, "state": "done", "project": "<dir>", "output": "<file>",
              "width": 1080, "height": 1920, "error": ""}]}
```

- `project.path` and an export's `project` are spelled exactly as the host
  gave the folder (`--project`, `host.open_project`), so the host can compare
  strings.
- `dirty` is true from an edit until the save that follows it is on disk: the
  edit's flag, the 1.5 s autosave timer, or a save still being written.
- `graphics.software` is true when the window or the preview draws in
  software for any reason: a software adapter (WARP), `CONCAT_GPU=software`,
  or previews fallen back to the CPU after the device was lost.
- `exports` holds the last 20 exports of the Export sheet that ended since the
  process started, oldest first; `seq` starts at 1 and only grows. `state` is
  `done`, `failed` or `cancelled`. **A `done` entry is added only after the
  file is complete under its final name**: the engine renders to a file beside
  it and renames it over the name as its last step
  (`concat-export/src/lib.rs`, `std::fs::rename(finished, &output)`), and the
  sheet's `Finished` message, where the entry is made, is sent after that
  returns. A failed or cancelled export leaves no file under the final name
  (an older file of that name, if the person chose to replace it, is left as
  it was).

### The Export sheet, when the host named the folder

With `--export-dir` (or `exportDir`), the file goes into that folder (made if
missing); the sheet shows no folder line, no Browse and no "Show in folder".
The name starts as `--export-name` and can still be changed; the question
before replacing a file of the same name stays.

### Not offered while embedded

File > Open project, Close project and Exit, and their keys (Ctrl+O, Ctrl+W,
Ctrl+Q); the launch screen with New project, Open, Open recent and Import
CapCut draft (an empty ground is shown when no project is open); Settings >
Automation; Settings' "Draw on the GPU" switch; the window's own minimise,
maximise and close buttons. The host offers what replaces them.

### Keyboard

A press in the editor gives it the keyboard. `host.blur`, hiding the editor,
or **Ctrl+F6** pressed in the editor gives it back to the host's window.

### When the host goes away

The editor watches the host's window; when it is gone the project is saved and
closed and the process leaves (a host should start the editor detached, so
that its own crash does not take the editor down before it can save).

## 7. "The first pause after launch is dropped" (stage 1's report)

See the end of this file for what was found.

## 8. Build

`.github/workflows/build-windows.yml` (by hand): the same environment as
upstream's Windows job, the `release` profile with thin LTO, and the staged
folder uploaded as an artifact (`clips-kitty-editor.exe`, FFmpeg 8.1 LGPL
shared DLLs, ONNX Runtime, whisper's CPU modules, the Visual C++ runtime,
LICENSE, THIRD_PARTY_NOTICES.md, PATCHES.md). `scripts/ci/smoke-windows.ps1`
tries whichever program name the bundle has.

## Findings on "the first pause after launch is dropped"

Not yet looked into on a build of this branch; this section is filled in
after the build is tried.
