// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Agnostic Input Device Event Abstraction & Global Queue
// Date: 2026-11-10

//! Agnostic input device abstraction providing normalized input events
//! (Keyboard, Mouse/Pointer, Touch) and a thread-safe, lock-free ring buffer.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Normalized keycodes independent of hardware transport (PS/2, USB HID, Bluetooth).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum KeyCode {
    Unknown(u16),
    A, B, C, D, E, F, G, H, I, J, K, L, M,
    N, O, P, Q, R, S, T, U, V, W, X, Y, Z,
    Num0, Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9,
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    Enter,
    Escape,
    Backspace,
    Tab,
    Space,
    Minus,
    Equal,
    LeftBracket,
    RightBracket,
    Backslash,
    Semicolon,
    Apostrophe,
    Grave,
    Comma,
    Period,
    Slash,
    LeftCtrl,
    RightCtrl,
    LeftShift,
    RightShift,
    LeftAlt,
    RightAlt,
    CapsLock,
    ScrollLock,
    NumLock,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Delete,
}

/// Normalized pointer button identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum MouseButtonKind {
    Left,
    Right,
    Middle,
    Other(u8),
}

/// Unified, transport-agnostic input event representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum InputEvent {
    /// A keyboard key was pressed down.
    KeyPress { keycode: KeyCode },
    /// A keyboard key was released.
    KeyRelease { keycode: KeyCode },
    /// Relative pointer movement delta.
    PointerMove { dx: i32, dy: i32 },
    /// Absolute pointer coordinates (e.g. tablet, touchscreen, or clamped cursor).
    PointerAbs { x: u32, y: u32 },
    /// Pointer button state transition.
    MouseButton { button: MouseButtonKind, pressed: bool },
}

const QUEUE_CAPACITY: usize = 256;

static mut EVENT_QUEUE: [Option<InputEvent>; QUEUE_CAPACITY] = [None; QUEUE_CAPACITY];
static HEAD: AtomicUsize = AtomicUsize::new(0);
static TAIL: AtomicUsize = AtomicUsize::new(0);

/// Pushes an input event into the global agnostic event queue.
/// Discards oldest unread event if queue is saturated to prevent stalling ISRs.
pub fn push_event(event: InputEvent) {
    let head = HEAD.load(Ordering::Relaxed);
    let next_head = (head + 1) % QUEUE_CAPACITY;

    let tail = TAIL.load(Ordering::Acquire);
    if next_head == tail {
        // Queue full: advance tail to drop oldest event
        TAIL.store((tail + 1) % QUEUE_CAPACITY, Ordering::Release);
    }

    unsafe {
        EVENT_QUEUE[head] = Some(event);
    }
    HEAD.store(next_head, Ordering::Release);
}

/// Retrieves the next pending input event, if available.
#[allow(dead_code)]
pub fn pop_event() -> Option<InputEvent> {
    let tail = TAIL.load(Ordering::Relaxed);
    let head = HEAD.load(Ordering::Acquire);

    if tail == head {
        return None;
    }

    let event = unsafe { EVENT_QUEUE[tail].take() };
    TAIL.store((tail + 1) % QUEUE_CAPACITY, Ordering::Release);
    event
}

/// Returns true if the input event queue currently holds no pending events.
#[allow(dead_code)]
pub fn is_empty() -> bool {
    TAIL.load(Ordering::Relaxed) == HEAD.load(Ordering::Relaxed)
}
