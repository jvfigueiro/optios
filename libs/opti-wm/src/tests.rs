// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Window Manager Unit Tests (libs/opti-wm)
// Date: 2026-11-10

extern crate alloc;

use super::*;

#[test]
fn test_z_order_sorting() {
    let mut wm = WindowManager::new(1280, 800);
    let w1 = Window::new(1, "Window 1", 100, 100, 400, 300);
    let w2 = Window::new(2, "Window 2", 150, 150, 400, 300);
    let w3 = Window::new(3, "Window 3", 200, 200, 400, 300);

    wm.add_window(w1);
    wm.add_window(w2);
    wm.add_window(w3);

    // Initial order should be w1, w2, w3
    let sorted = wm.windows_sorted_by_z();
    assert_eq!(sorted.len(), 3);
    assert_eq!(sorted[0].id, 1);
    assert_eq!(sorted[1].id, 2);
    assert_eq!(sorted[2].id, 3);

    // Bringing w1 to front should reorder to w2, w3, w1
    wm.bring_to_front(1);
    let sorted = wm.windows_sorted_by_z();
    assert_eq!(sorted[0].id, 2);
    assert_eq!(sorted[1].id, 3);
    assert_eq!(sorted[2].id, 1);
    assert_eq!(wm.active_window, Some(1));
}

#[test]
fn test_focus_on_click() {
    let mut wm = WindowManager::new(1280, 800);
    let w1 = Window::new(1, "Window 1", 100, 100, 400, 300);
    let w2 = Window::new(2, "Window 2", 300, 100, 400, 300);

    wm.add_window(w1);
    wm.add_window(w2);

    assert_eq!(wm.active_window, Some(2));
    assert!(wm.get_window(2).unwrap().focused);
    assert!(!wm.get_window(1).unwrap().focused);

    // Click inside w1 body (x=150, y=150)
    let event = wm.handle_mouse_down(150, 150, MouseButton::Left);
    assert_eq!(event, Some(WmEvent::WindowFocused(1)));
    assert_eq!(wm.active_window, Some(1));
    assert!(wm.get_window(1).unwrap().focused);
    assert!(!wm.get_window(2).unwrap().focused);
}

#[test]
fn test_hit_testing_titlebar_and_close() {
    let mut wm = WindowManager::new(1280, 800);
    let w1 = Window::new(1, "Window 1", 100, 100, 400, 300);
    wm.add_window(w1);

    // Click on titlebar (x=200, y=110)
    assert_eq!(wm.hit_test(200, 110), HitTarget::TitleBar(1));

    // Click on close button (top right: x=100+400-10=490, y=110)
    assert_eq!(wm.hit_test(490, 110), HitTarget::CloseButton(1));

    // Click inside body (x=200, y=150)
    assert_eq!(wm.hit_test(200, 150), HitTarget::Body(1));

    // Click outside on desktop (x=50, y=50)
    assert_eq!(wm.hit_test(50, 50), HitTarget::Desktop);
}

#[test]
fn test_window_drag_and_bounds_clamping() {
    let mut wm = WindowManager::new(1280, 800);
    let w1 = Window::new(1, "Window 1", 100, 100, 400, 300);
    wm.add_window(w1);

    // Mouse down on titlebar at (150, 110)
    wm.handle_mouse_down(150, 110, MouseButton::Left);
    let win = wm.get_window(1).unwrap();
    assert!(win.dragging);
    assert_eq!(win.drag_offset_x, 50);
    assert_eq!(win.drag_offset_y, 10);

    // Drag to (250, 210)
    let move_ev = wm.handle_mouse_move(250, 210, true);
    assert_eq!(move_ev, Some(WmEvent::WindowMoved(1, 200, 200)));
    let win = wm.get_window(1).unwrap();
    assert_eq!(win.x, 200);
    assert_eq!(win.y, 200);

    // Drag past top boundary (y=5 -> clamped to TOP_BAR_HEIGHT = 28)
    wm.handle_mouse_move(250, 15, true);
    let win = wm.get_window(1).unwrap();
    assert_eq!(win.y, TOP_BAR_HEIGHT);

    // Release mouse
    wm.handle_mouse_up(MouseButton::Left);
    let win = wm.get_window(1).unwrap();
    assert!(!win.dragging);
}

#[test]
fn test_minimize_and_restore() {
    let mut wm = WindowManager::new(1280, 800);
    let w1 = Window::new(1, "Window 1", 100, 100, 400, 300);
    wm.add_window(w1);

    // Click close button
    let ev = wm.handle_mouse_down(490, 110, MouseButton::Left);
    assert_eq!(ev, Some(WmEvent::WindowClosed(1)));
    assert!(wm.get_window(1).unwrap().minimized);

    // Hit test should now return Desktop since window is minimized
    assert_eq!(wm.hit_test(200, 150), HitTarget::Desktop);

    // Restore via toggle
    let ev = wm.toggle_minimize(1);
    assert_eq!(ev, Some(WmEvent::WindowRestored(1)));
    assert!(!wm.get_window(1).unwrap().minimized);
    assert_eq!(wm.active_window, Some(1));
}
