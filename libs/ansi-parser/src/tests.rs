// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: ANSI Escape Sequence Parser Unit Tests
// Date: 2026-11-10

extern crate alloc;
use alloc::vec::Vec;
use super::*;

fn parse_str(s: &str) -> Vec<AnsiAction> {
    let mut parser = AnsiParser::new();
    let mut actions = Vec::new();
    for &b in s.as_bytes() {
        parser.parse_byte(b, |act| actions.push(act));
    }
    actions
}

#[test]
fn test_plain_text() {
    let actions = parse_str("Hello, OptiOS!");
    assert_eq!(actions.len(), 14);
    assert_eq!(actions[0], AnsiAction::Print(b'H'));
    assert_eq!(actions[13], AnsiAction::Print(b'!'));
}

#[test]
fn test_sgr_reset() {
    let actions = parse_str("\x1b[0m");
    assert_eq!(actions, alloc::vec![AnsiAction::ResetAttributes]);

    let actions_empty = parse_str("\x1b[m");
    assert_eq!(actions_empty, alloc::vec![AnsiAction::ResetAttributes]);
}

#[test]
fn test_sgr_standard_colors() {
    let actions = parse_str("\x1b[31mRed\x1b[32mGreen\x1b[36mCyan\x1b[0m");
    assert_eq!(
        actions,
        alloc::vec![
            AnsiAction::SetFg(AnsiColor::Red),
            AnsiAction::Print(b'R'),
            AnsiAction::Print(b'e'),
            AnsiAction::Print(b'd'),
            AnsiAction::SetFg(AnsiColor::Green),
            AnsiAction::Print(b'G'),
            AnsiAction::Print(b'r'),
            AnsiAction::Print(b'e'),
            AnsiAction::Print(b'e'),
            AnsiAction::Print(b'n'),
            AnsiAction::SetFg(AnsiColor::Cyan),
            AnsiAction::Print(b'C'),
            AnsiAction::Print(b'y'),
            AnsiAction::Print(b'a'),
            AnsiAction::Print(b'n'),
            AnsiAction::ResetAttributes,
        ]
    );
}

#[test]
fn test_sgr_bright_colors() {
    let actions = parse_str("\x1b[96mBrightCyan\x1b[97mBrightWhite");
    assert_eq!(actions[0], AnsiAction::SetFg(AnsiColor::BrightCyan));
    assert_eq!(actions[11], AnsiAction::SetFg(AnsiColor::BrightWhite));
}

#[test]
fn test_sgr_compound() {
    let actions = parse_str("\x1b[1;36;40mText");
    assert_eq!(
        actions,
        alloc::vec![
            AnsiAction::SetBold(true),
            AnsiAction::SetFg(AnsiColor::Cyan),
            AnsiAction::SetBg(AnsiColor::Black),
            AnsiAction::Print(b'T'),
            AnsiAction::Print(b'e'),
            AnsiAction::Print(b'x'),
            AnsiAction::Print(b't'),
        ]
    );
}

#[test]
fn test_clear_screen() {
    let actions = parse_str("\x1b[2J");
    assert_eq!(actions, alloc::vec![AnsiAction::ClearScreen]);
}

#[test]
fn test_cursor_home_and_position() {
    let actions_home = parse_str("\x1b[H");
    assert_eq!(
        actions_home,
        alloc::vec![AnsiAction::SetCursorPosition { row: 0, col: 0 }]
    );

    let actions_pos = parse_str("\x1b[10;25H");
    assert_eq!(
        actions_pos,
        alloc::vec![AnsiAction::SetCursorPosition { row: 9, col: 24 }]
    );
}

#[test]
fn test_malformed_escape_sequence_recovery() {
    // Esc followed by a non-bracket character should reset to ground and print that char
    let actions = parse_str("\x1bXHello");
    assert_eq!(
        actions,
        alloc::vec![
            AnsiAction::Print(b'X'),
            AnsiAction::Print(b'H'),
            AnsiAction::Print(b'e'),
            AnsiAction::Print(b'l'),
            AnsiAction::Print(b'l'),
            AnsiAction::Print(b'o'),
        ]
    );
}

#[test]
fn test_unknown_csi_sequence_ignored() {
    // Esc [ ? 2 5 h (show cursor, unhandled) should be safely dropped without breaking subsequent text
    let actions = parse_str("\x1b[?25hNormal");
    assert_eq!(
        actions,
        alloc::vec![
            AnsiAction::Print(b'N'),
            AnsiAction::Print(b'o'),
            AnsiAction::Print(b'r'),
            AnsiAction::Print(b'm'),
            AnsiAction::Print(b'a'),
            AnsiAction::Print(b'l'),
        ]
    );
}

#[test]
fn test_color_to_rgb32() {
    assert_eq!(AnsiColor::Black.to_rgb32(false), 0x00000000);
    assert_eq!(AnsiColor::Cyan.to_rgb32(false), 0x0033CCCC);
    assert_eq!(AnsiColor::BrightCyan.to_rgb32(false), 0x0055FFFF);
    assert_eq!(AnsiColor::Default.to_rgb32(true), 0x00000000);
    assert_eq!(AnsiColor::Default.to_rgb32(false), 0x00E0E0E0);
}
