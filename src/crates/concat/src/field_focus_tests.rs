// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Clips Kitty contributors

//! A text field's drawn box is all field: a press anywhere inside it gives
//! the words the keyboard. A press in a box's padding used to fall through
//! to the window's own keyboard scope (`keys` in `ui/app.slint`), which
//! takes the focus on a press on nothing; the field let go without a sign
//! a person would notice, and the letters typed next ran the one-key
//! shortcuts (w trimmed a caption, m muted a clip) instead of being
//! written. Slint has no test backend in this build, so these read the
//! interface's source for the parts that close that gap.

use std::path::{Path, PathBuf};

fn ui_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ui")
}

fn read(relative: &str) -> String {
    std::fs::read_to_string(ui_dir().join(relative))
        .unwrap_or_else(|error| panic!("reading ui/{relative}: {error}"))
}

fn slint_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            slint_files(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "slint") {
            found.push(path);
        }
    }
}

/// The frame of a block of typed words takes a press off the words and
/// hands it to them, from under the words (so a caret is still placed and
/// a selection still dragged by the words themselves).
#[test]
fn the_resize_box_hands_a_press_to_its_words() {
    let source = read("primitives/resize-box.slint");
    assert!(source.contains("callback focus-field();"));
    let touch = source
        .find("clicked => { root.focus-field(); }")
        .expect("a TouchArea that asks for the field's focus");
    let flick = source.find("flick := Flickable").unwrap();
    assert!(
        touch < flick,
        "the TouchArea is under the words, not over them"
    );
}

/// Every box of typed words answers that press with its own field.
#[test]
fn every_resize_box_focuses_its_field() {
    let mut files = Vec::new();
    slint_files(&ui_dir(), &mut files);
    let mut boxes = 0;
    for file in files {
        let source = std::fs::read_to_string(&file).unwrap();
        let mut rest = source.as_str();
        while let Some(at) = rest.find("ResizeBox {") {
            let body = &rest[at..];
            // The box's own properties come before its TextInput child.
            let head = &body[..body.find("TextInput").unwrap_or(body.len())];
            if !head.contains("export component") {
                boxes += 1;
                assert!(
                    head.contains("focus-field =>"),
                    "a ResizeBox in {} leaves a press in its padding to the shortcuts",
                    file.display()
                );
            }
            rest = &body[1..];
        }
    }
    assert!(
        boxes >= 4,
        "found {boxes} ResizeBox uses; the caption, script and prompt boxes at least"
    );
}

/// The other fields drawn with padding round their words: the colour's
/// hex (the picker's and the inspector's, whose hash is part of it) and a
/// timeline tab's rename.
#[test]
fn padded_fields_take_a_press_in_their_padding() {
    for (file, call) in [
        (
            "primitives/color-picker.slint",
            "clicked => { hex.focus(); }",
        ),
        ("timeline/tabs.slint", "clicked => { rename.focus(); }"),
        ("inspector/controls.slint", "clicked => { input.focus(); }"),
    ] {
        assert!(read(file).contains(call), "{file}: {call}");
    }
}
