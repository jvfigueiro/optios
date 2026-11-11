// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: PS/2 Mouse Driver (IRQ 12) & Packet State Machine
// Date: 2026-11-10

//! PS/2 Auxiliary Device (Mouse) driver.
//! Listens on IRQ 12 (vector 44), processes 3-byte packets, and updates
//! screen-clamped coordinates and button states.

use core::sync::atomic::{AtomicBool, Ordering};
use crate::arch::x86_64::io::{inb, outb};
use crate::arch::x86_64::pic;
use crate::info;

const DATA_PORT: u16 = 0x60;
const STATUS_PORT: u16 = 0x64;
const COMMAND_PORT: u16 = 0x64;

const SCREEN_WIDTH: i32 = 1280;
const SCREEN_HEIGHT: i32 = 800;

#[derive(Clone, Copy, Debug, Default)]
pub struct MouseState {
    pub x: i32,
    pub y: i32,
    pub btn_left: bool,
    pub btn_right: bool,
    pub btn_middle: bool,
}

static mut CURRENT_MOUSE: MouseState = MouseState {
    x: SCREEN_WIDTH / 2,
    y: SCREEN_HEIGHT / 2,
    btn_left: false,
    btn_right: false,
    btn_middle: false,
};

static mut PACKET_BUFFER: [u8; 3] = [0; 3];
static mut PACKET_PHASE: u8 = 0;
static MOUSE_LOCK: AtomicBool = AtomicBool::new(false);

fn mouse_wait_write() {
    for _ in 0..100_000 {
        if (unsafe { inb(STATUS_PORT) } & 0x02) == 0 {
            return;
        }
    }
}

fn mouse_wait_read() -> bool {
    for _ in 0..100_000 {
        if (unsafe { inb(STATUS_PORT) } & 0x01) != 0 {
            return true;
        }
    }
    false
}

fn mouse_write(cmd: u8) {
    mouse_wait_write();
    unsafe {
        outb(COMMAND_PORT, 0xD4);
    }
    mouse_wait_write();
    unsafe {
        outb(DATA_PORT, cmd);
    }
}

fn mouse_read() -> u8 {
    if mouse_wait_read() {
        unsafe { inb(DATA_PORT) }
    } else {
        0
    }
}

/// Initializes the PS/2 Auxiliary Device (Mouse) and unmasks IRQ 12.
pub fn init() {
    // 1. Enable auxiliary mouse device (command 0xA8 to 0x64)
    mouse_wait_write();
    unsafe {
        outb(COMMAND_PORT, 0xA8);
    }

    // 2. Read Compaq Status Byte (command 0x20 to 0x64)
    mouse_wait_write();
    unsafe {
        outb(COMMAND_PORT, 0x20);
    }
    let status = mouse_read();

    // Enable IRQ 12 (bit 1) and clear mouse clock disable (bit 5)
    let new_status = (status | 0x02) & !0x20;

    // Write Compaq Status Byte (command 0x60 to 0x64)
    mouse_wait_write();
    unsafe {
        outb(COMMAND_PORT, 0x60);
    }
    mouse_wait_write();
    unsafe {
        outb(DATA_PORT, new_status);
    }

    // 3. Set Defaults (0xF6) and Enable Data Reporting (0xF4)
    mouse_write(0xF6);
    let _ = mouse_read(); // ACK (0xFA)

    mouse_write(0xF4);
    let _ = mouse_read(); // ACK (0xFA)

    // 4. Unmask IRQ 12 on PIC
    pic::unmask_irq(12);

    info!("PS/2 Mouse initialized: Listening on IRQ 12 (port 0x60, Aux Device).");
}

/// Interrupt handler for IRQ 12 (PS/2 Mouse).
pub fn on_irq12() {
    let status = unsafe { inb(STATUS_PORT) };
    if (status & 0x01) == 0 {
        return;
    }

    let byte = unsafe { inb(DATA_PORT) };

    unsafe {
        let phase = &raw mut PACKET_PHASE;
        let buf = &raw mut PACKET_BUFFER;

        match *phase {
            0 => {
                // Byte 1: Bit 3 must be 1. If not, discard (desynchronized).
                if (byte & 0x08) != 0 {
                    (*buf)[0] = byte;
                    *phase = 1;
                }
            }
            1 => {
                (*buf)[1] = byte;
                *phase = 2;
            }
            2 => {
                (*buf)[2] = byte;
                *phase = 0;

                let flags = (*buf)[0];
                let raw_dx = (*buf)[1];
                let raw_dy = (*buf)[2];

                let x_overflow = (flags & 0x40) != 0;
                let y_overflow = (flags & 0x80) != 0;

                if !x_overflow && !y_overflow {
                    let btn_left = (flags & 0x01) != 0;
                    let btn_right = (flags & 0x02) != 0;
                    let btn_middle = (flags & 0x04) != 0;

                    let x_sign = (flags & 0x10) != 0;
                    let y_sign = (flags & 0x20) != 0;

                    let dx = if x_sign { (raw_dx as i16 | !0xFF) as i32 } else { raw_dx as i32 };
                    let dy = if y_sign { (raw_dy as i16 | !0xFF) as i32 } else { raw_dy as i32 };

                    // PS/2 Y is positive upwards, screen Y is positive downwards
                    let screen_dy = -dy;

                    let cur = &raw mut CURRENT_MOUSE;
                    let mut new_x = (*cur).x + dx;
                    let mut new_y = (*cur).y + screen_dy;

                    if new_x < 0 { new_x = 0; }
                    if new_x >= SCREEN_WIDTH { new_x = SCREEN_WIDTH - 1; }
                    if new_y < 0 { new_y = 0; }
                    if new_y >= SCREEN_HEIGHT { new_y = SCREEN_HEIGHT - 1; }

                    (*cur).x = new_x;
                    (*cur).y = new_y;
                    (*cur).btn_left = btn_left;
                    (*cur).btn_right = btn_right;
                    (*cur).btn_middle = btn_middle;

                    // Publish agnostic input events to Driver Kit queue
                    crate::drivers::kit::input::push_event(crate::drivers::kit::InputEvent::PointerMove {
                        dx,
                        dy: screen_dy,
                    });
                    crate::drivers::kit::input::push_event(crate::drivers::kit::InputEvent::PointerAbs {
                        x: new_x as u32,
                        y: new_y as u32,
                    });
                    crate::drivers::kit::input::push_event(crate::drivers::kit::InputEvent::MouseButton {
                        button: crate::drivers::kit::MouseButtonKind::Left,
                        pressed: btn_left,
                    });
                    crate::drivers::kit::input::push_event(crate::drivers::kit::InputEvent::MouseButton {
                        button: crate::drivers::kit::MouseButtonKind::Right,
                        pressed: btn_right,
                    });
                    if btn_middle {
                        crate::drivers::kit::input::push_event(crate::drivers::kit::InputEvent::MouseButton {
                            button: crate::drivers::kit::MouseButtonKind::Middle,
                            pressed: true,
                        });
                    }
                }
            }
            _ => {
                *phase = 0;
            }
        }
    }
}

/// Updates mouse coordinates and button states from relative deltas (e.g. USB HID mouse).
pub fn update_relative(dx: i32, dy: i32, btn_left: bool, btn_right: bool, btn_middle: bool) {
    let cur = &raw mut CURRENT_MOUSE;
    let mut new_x = unsafe { (*cur).x } + dx;
    let mut new_y = unsafe { (*cur).y } + dy;

    if new_x < 0 { new_x = 0; }
    if new_x >= SCREEN_WIDTH { new_x = SCREEN_WIDTH - 1; }
    if new_y < 0 { new_y = 0; }
    if new_y >= SCREEN_HEIGHT { new_y = SCREEN_HEIGHT - 1; }

    unsafe {
        (*cur).x = new_x;
        (*cur).y = new_y;
        (*cur).btn_left = btn_left;
        (*cur).btn_right = btn_right;
        (*cur).btn_middle = btn_middle;
    }

    // Publish agnostic input events to Driver Kit queue
    crate::drivers::kit::input::push_event(crate::drivers::kit::InputEvent::PointerMove { dx, dy });
    crate::drivers::kit::input::push_event(crate::drivers::kit::InputEvent::PointerAbs {
        x: new_x as u32,
        y: new_y as u32,
    });
    crate::drivers::kit::input::push_event(crate::drivers::kit::InputEvent::MouseButton {
        button: crate::drivers::kit::MouseButtonKind::Left,
        pressed: btn_left,
    });
    crate::drivers::kit::input::push_event(crate::drivers::kit::InputEvent::MouseButton {
        button: crate::drivers::kit::MouseButtonKind::Right,
        pressed: btn_right,
    });
    if btn_middle {
        crate::drivers::kit::input::push_event(crate::drivers::kit::InputEvent::MouseButton {
            button: crate::drivers::kit::MouseButtonKind::Middle,
            pressed: true,
        });
    }
}

/// Returns a copy of the current mouse state.
pub fn get_state() -> MouseState {
    while MOUSE_LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let state = unsafe { CURRENT_MOUSE };
    MOUSE_LOCK.store(false, Ordering::Release);
    state
}
