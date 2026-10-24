// Window positions on Wayland. Wayland doesn't let a normal window choose where it goes,
// but desktop widgets and panels can use the "layer shell" protocol: the window belongs
// to one monitor, is stuck to its top and left edges, and margins say how far from them.
// So a position here is a monitor plus two margins, and we always know it, because we set it.
// GNOME has no layer shell: there, and on X11, setup() returns false and nothing changes.
use crate::position::ZPos;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

// Makes `window` a layer-shell window at (x, y), counted from the primary monitor's
// top-left like WindowX/WindowY, on the layer that matches AlwaysOnTop.
// Must run before the window is shown.
// Returns false (and does nothing) when the desktop has no layer shell.
pub fn setup(window: &impl IsA<gtk4::Window>, pos: (i32, i32), z: ZPos) -> bool {
    let window = window.as_ref();
    // is_supported() prints a CRITICAL warning when asked on X11, so don't ask there
    if WidgetExt::display(window).is::<gdk4_x11::X11Display>() || !gtk4_layer_shell::is_supported() {
        return false;
    }
    window.init_layer_shell();
    window.set_namespace(Some("snowmeter")); // a name the desktop can use in its own rules
    window.set_layer(layer(z));
    window.set_keyboard_mode(KeyboardMode::None); // skins don't take typing
    // -1: don't move out of the way of panels, so margins count from the monitor's real edge
    window.set_exclusive_zone(-1);
    window.set_anchor(Edge::Top, true);
    window.set_anchor(Edge::Left, true);
    move_to(window, pos);
    true
}

// Moves a shown layer-shell window to the layer for `z` (!ZPos). Returns false, doing
// nothing, for any other window.
pub fn set_z_pos(window: &impl IsA<gtk4::Window>, z: ZPos) -> bool {
    let window = window.as_ref();
    if !window.is_layer_window() {
        return false;
    }
    window.set_layer(layer(z));
    true
}

// The layer for an AlwaysOnTop value. Layer windows are never mixed in with normal ones,
// only under them all (Bottom) or over them all (Top), so Normal has to be Bottom.
// The desktop values aren't Background: wallpaper programs draw there and would cover us.
fn layer(z: ZPos) -> Layer {
    match z {
        ZPos::OnDesktop | ZPos::Bottom | ZPos::Normal => Layer::Bottom,
        ZPos::OnTop => Layer::Top,
        ZPos::Topmost => Layer::Overlay,
    }
}

// Moves a layer-shell window so its top-left corner is at (x, y), counted from the primary
// monitor's top-left. The monitor is the one that holds that point (else the primary).
pub fn move_to(window: &impl IsA<gtk4::Window>, (x, y): (i32, i32)) {
    let window = window.as_ref();
    let (ox, oy) = crate::monitors::origin();
    // Back into GTK's coordinates, where the monitors' rectangles are
    let (gx, gy) = (x + ox, y + oy);
    let monitors = crate::monitors::gtk_monitors();
    let monitor = monitors
        .iter()
        .find(|m| m.geometry().contains_point(gx, gy))
        .or(monitors.first());
    let Some(monitor) = monitor else { return };
    let r = monitor.geometry();
    window.set_monitor(Some(monitor));
    window.set_margin(Edge::Left, gx - r.x());
    window.set_margin(Edge::Top, gy - r.y());
}

// Where a layer-shell window's top-left corner is, counted from the primary monitor's
// top-left. None for any other window.
pub fn position(window: &impl IsA<gtk4::Window>) -> Option<(i32, i32)> {
    let window = window.as_ref();
    if !window.is_layer_window() {
        return None;
    }
    let r = window.monitor()?.geometry();
    let (ox, oy) = crate::monitors::origin();
    Some((r.x() + window.margin(Edge::Left) - ox, r.y() + window.margin(Edge::Top) - oy))
}
