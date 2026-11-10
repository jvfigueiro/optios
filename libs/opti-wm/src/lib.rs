// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Pure Window Geometry, Z-Order & Hit-Testing Manager (libs/opti-wm)
// Date: 2026-11-10

//! Pure `#![no_std]` window geometry, z-order, hit-testing, and event handling library.

#![no_std]

extern crate alloc;
use alloc::vec::Vec;

pub const TITLEBAR_HEIGHT: i32 = 26;
pub const CLOSE_BTN_WIDTH: i32 = 24;
pub const TOP_BAR_HEIGHT: i32 = 28;
pub const BOTTOM_DOCK_HEIGHT: i32 = 40;

/// Mouse button identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// Window Manager interaction event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WmEvent {
    WindowFocused(usize),
    WindowMoved(usize, i32, i32),
    WindowClosed(usize),
    WindowRestored(usize),
}

/// Hit test target result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    TitleBar(usize),
    CloseButton(usize),
    Body(usize),
    Desktop,
}

/// A desktop window with position, dimensions, z-order, and state flags.
#[derive(Debug, Clone)]
pub struct Window {
    pub id: usize,
    pub title: &'static str,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub z_order: usize,
    pub focused: bool,
    pub minimized: bool,
    pub dragging: bool,
    pub drag_offset_x: i32,
    pub drag_offset_y: i32,
}

impl Window {
    pub fn new(id: usize, title: &'static str, x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            id,
            title,
            x,
            y,
            width,
            height,
            z_order: id,
            focused: false,
            minimized: false,
            dragging: false,
            drag_offset_x: 0,
            drag_offset_y: 0,
        }
    }

    /// Tests if a coordinate falls within the window boundaries (titlebar + body).
    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        if self.minimized {
            return false;
        }
        let w = self.width as i32;
        let h = self.height as i32;
        px >= self.x && px < (self.x + w) && py >= self.y && py < (self.y + h)
    }

    /// Hit-tests a coordinate against this window's components.
    pub fn hit_test(&self, px: i32, py: i32) -> Option<HitTarget> {
        if !self.contains_point(px, py) {
            return None;
        }

        let w = self.width as i32;
        // Check if inside titlebar
        if py < self.y + TITLEBAR_HEIGHT {
            // Check close button on top-right
            let close_btn_x = self.x + w - CLOSE_BTN_WIDTH;
            if px >= close_btn_x {
                Some(HitTarget::CloseButton(self.id))
            } else {
                Some(HitTarget::TitleBar(self.id))
            }
        } else {
            Some(HitTarget::Body(self.id))
        }
    }
}

/// Window Manager maintaining the collection of open windows, active focus, and z-order.
pub struct WindowManager {
    pub screen_width: u32,
    pub screen_height: u32,
    pub windows: Vec<Window>,
    pub active_window: Option<usize>,
    next_z: usize,
}

impl WindowManager {
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        Self {
            screen_width,
            screen_height,
            windows: Vec::new(),
            active_window: None,
            next_z: 1,
        }
    }

    /// Adds a window and brings it to front.
    pub fn add_window(&mut self, mut window: Window) -> usize {
        let id = window.id;
        window.z_order = self.next_z;
        self.next_z += 1;
        self.windows.push(window);
        self.bring_to_front(id);
        id
    }

    /// Retrieves an immutable reference to a window by ID.
    pub fn get_window(&self, id: usize) -> Option<&Window> {
        self.windows.iter().find(|w| w.id == id)
    }

    /// Retrieves a mutable reference to a window by ID.
    pub fn get_window_mut(&mut self, id: usize) -> Option<&mut Window> {
        self.windows.iter_mut().find(|w| w.id == id)
    }

    /// Returns a list of non-minimized windows sorted by z-order (ascending, back to front).
    pub fn windows_sorted_by_z(&self) -> Vec<&Window> {
        let mut list: Vec<&Window> = self.windows.iter().filter(|w| !w.minimized).collect();
        list.sort_by_key(|w| w.z_order);
        list
    }

    /// Hit-tests a coordinate from top-most window to bottom-most window.
    pub fn hit_test(&self, px: i32, py: i32) -> HitTarget {
        let mut list: Vec<&Window> = self.windows.iter().filter(|w| !w.minimized).collect();
        // Sort descending (top to bottom)
        list.sort_by(|a, b| b.z_order.cmp(&a.z_order));

        for win in list {
            if let Some(target) = win.hit_test(px, py) {
                return target;
            }
        }
        HitTarget::Desktop
    }

    /// Brings a window to the top of the z-order and marks it focused.
    pub fn bring_to_front(&mut self, id: usize) {
        let new_z = self.next_z;
        self.next_z += 1;

        for win in self.windows.iter_mut() {
            if win.id == id {
                win.z_order = new_z;
                win.focused = true;
                win.minimized = false;
            } else {
                win.focused = false;
            }
        }
        self.active_window = Some(id);
    }

    /// Toggles the minimized state of a window.
    pub fn toggle_minimize(&mut self, id: usize) -> Option<WmEvent> {
        if let Some(win) = self.get_window_mut(id) {
            win.minimized = !win.minimized;
            if win.minimized {
                win.focused = false;
                win.dragging = false;
                Some(WmEvent::WindowClosed(id))
            } else {
                let event = Some(WmEvent::WindowRestored(id));
                self.bring_to_front(id);
                event
            }
        } else {
            None
        }
    }

    /// Handles mouse button press event.
    pub fn handle_mouse_down(&mut self, px: i32, py: i32, btn: MouseButton) -> Option<WmEvent> {
        if btn != MouseButton::Left {
            return None;
        }

        match self.hit_test(px, py) {
            HitTarget::CloseButton(id) => self.toggle_minimize(id),
            HitTarget::TitleBar(id) => {
                self.bring_to_front(id);
                if let Some(win) = self.get_window_mut(id) {
                    win.dragging = true;
                    win.drag_offset_x = px - win.x;
                    win.drag_offset_y = py - win.y;
                }
                Some(WmEvent::WindowFocused(id))
            }
            HitTarget::Body(id) => {
                self.bring_to_front(id);
                Some(WmEvent::WindowFocused(id))
            }
            HitTarget::Desktop => {
                for win in self.windows.iter_mut() {
                    win.focused = false;
                }
                self.active_window = None;
                None
            }
        }
    }

    /// Handles mouse move event, updating dragged window position with boundary clamping.
    pub fn handle_mouse_move(&mut self, px: i32, py: i32, left_down: bool) -> Option<WmEvent> {
        if !left_down {
            for win in self.windows.iter_mut() {
                win.dragging = false;
            }
            return None;
        }

        let screen_w = self.screen_width as i32;
        let screen_h = self.screen_height as i32;

        for win in self.windows.iter_mut() {
            if win.dragging {
                let mut new_x = px - win.drag_offset_x;
                let mut new_y = py - win.drag_offset_y;

                let win_w = win.width as i32;

                // Screen clamping: keep titlebar accessible
                if new_x < -win_w + 60 {
                    new_x = -win_w + 60;
                }
                if new_x > screen_w - 60 {
                    new_x = screen_w - 60;
                }
                if new_y < TOP_BAR_HEIGHT {
                    new_y = TOP_BAR_HEIGHT;
                }
                if new_y > screen_h - BOTTOM_DOCK_HEIGHT - TITLEBAR_HEIGHT {
                    new_y = screen_h - BOTTOM_DOCK_HEIGHT - TITLEBAR_HEIGHT;
                }

                win.x = new_x;
                win.y = new_y;
                return Some(WmEvent::WindowMoved(win.id, new_x, new_y));
            }
        }

        None
    }

    /// Handles mouse button release event.
    pub fn handle_mouse_up(&mut self, btn: MouseButton) -> Option<WmEvent> {
        if btn == MouseButton::Left {
            for win in self.windows.iter_mut() {
                win.dragging = false;
            }
        }
        None
    }
}

#[cfg(test)]
mod tests;
