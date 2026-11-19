// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Clips Kitty contributors

//! The name this build goes by: Clips Kitty Editor, a modified build of
//! Concat.
//!
//! Concat's TRADEMARK.md asks a modified build to carry another name, so
//! everything a person reads says the name here, and Concat's own name is
//! kept for the credit (Settings > About, PATCHES.md, the notices). The
//! brand lives in this file, in `ui/brand.slint` (the window's title and
//! the About page's credit, which Slint needs as literals) and in the two
//! palettes' accent rows; nothing else names it, so merging upstream stays
//! a matter of these few places. See PATCHES.md at the repository's root.

/// The name a person sees: the window's title, the dialogs, the messages.
pub const NAME: &str = "Clips Kitty Editor";

/// The program's file name, without `.exe`; what `--version` starts with.
pub const BINARY: &str = "clips-kitty-editor";

/// The folder the settings, the models and the logs are kept under: this
/// build's own, never the one a Concat on the same machine uses, so the
/// two share no setting, no crash flag and no model folder.
pub const IDENTIFIER: &str = "com.clipskitty.editor";

/// The editor this one is built from, for the credit.
pub const BASED_ON: &str = "Concat";

/// Where the editor this one is built from lives.
pub const BASED_ON_URL: &str = "https://github.com/jub0t/Concat";

/// Where this build's own source is, as the licence asks.
pub const SOURCE_URL: &str = "https://github.com/ColinGPT9/clips-kitty-editor";

/// The licence of both.
pub const LICENCE: &str = "AGPL-3.0-or-later";

/// The accent a fresh install wears, by the name the theme lists it
/// under: Clips Kitty's blue (the "Sky" row of both palettes).
pub const DEFAULT_ACCENT: &str = "sky";

/// Whether a folder left by an unreleased upstream build under its former
/// identifier is taken over at start. Never: nothing of a Concat on this
/// machine is moved or read.
pub const ADOPTS_FORMER_FOLDER: bool = false;

/// Whether Settings offers to look for, download and install releases.
/// Not in this build: that page asks the upstream project's release list
/// and installs its packages, which are Concat and not this editor. Clips
/// Kitty updates the editor with itself.
pub const UPDATES: bool = false;

/// Interface lines (by locale key) that name something the upstream
/// project owns and serves, not the editor itself: Settings' "Concat's
/// mirror" download source (the upstream project's GitHub release) and
/// the Audio page's "Concat sounds" (its asset repository). Branding them
/// would say they are this build's, which they are not, and leaving them
/// would show another product's name, so they say what the thing is
/// without naming a product. One English line for every language: the
/// locales stay upstream's, word for word (K-D2).
pub const OVERRIDES: &[(&str, &str)] = &[
    ("settings.concatsMirror", "Project mirror (GitHub)"),
    (
        "sounds.offline",
        "Could not reach the sound library. Check the connection and try again.",
    ),
];

/// This build's own line for `key`, when it has one (see [`OVERRIDES`]).
pub fn line_for(key: &str) -> Option<&'static str> {
    OVERRIDES
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, line)| *line)
}

/// `text` with the upstream name replaced by this build's: what every
/// sentence shown to a person goes through, so the locales stay as
/// upstream writes them and a merge never touches them.
pub fn branded(text: &str) -> String {
    if text.contains(BASED_ON) {
        text.replace(BASED_ON, NAME)
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sentence_names_this_build_and_not_the_one_it_is_built_from() {
        assert_eq!(
            branded("Concat could not save this project"),
            "Clips Kitty Editor could not save this project"
        );
        assert_eq!(branded("Restart Concat?"), "Restart Clips Kitty Editor?");
        assert_eq!(branded("Nothing to rename"), "Nothing to rename");
    }

    #[test]
    fn the_upstream_projects_own_services_are_named_without_a_product() {
        for (key, line) in OVERRIDES {
            assert_eq!(line_for(key), Some(*line));
            assert!(!line.contains(BASED_ON) && !line.contains(NAME), "{key}");
        }
        assert_eq!(line_for("settings.concatsMirror"), Some("Project mirror (GitHub)"));
        assert_eq!(line_for("settings.restartTitle"), None);
    }

    #[test]
    fn the_folder_is_this_builds_own() {
        assert_ne!(IDENTIFIER, "app.concat.editor");
        assert!(!IDENTIFIER.contains("concat"));
        assert!(BINARY.chars().all(|c| c.is_ascii_lowercase() || c == '-'));
    }
}
