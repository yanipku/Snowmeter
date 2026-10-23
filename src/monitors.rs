use gtk4::gdk;
use gtk4::prelude::*;

// Built-in variables for the monitors: #SCREENAREAWIDTH#, #WORKAREAX@2#, #VSCREENAREAHEIGHT#, ...
// Keys are lowercase like the rest of the variable table.
//
// Rainmeter's plain #SCREENAREA...# means the skin's own monitor. Wayland doesn't tell us
// where a window is, so it means the primary one here, and the primary is the first
// monitor (Wayland has no "primary"). The work area (screen minus panels) isn't
// available from GTK4 either, so for now it is the whole screen.
//
// Returns nothing when GTK isn't started (as in `cargo test`) or there is no monitor.
pub fn screen_vars() -> Vec<(String, String)> {
    if !gtk4::is_initialized() {
        return Vec::new();
    }
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let list = display.monitors();
    let rects: Vec<gdk::Rectangle> = (0..list.n_items())
        .filter_map(|i| list.item(i).and_downcast::<gdk::Monitor>())
        .map(|m| m.geometry())
        .collect();
    let Some(primary) = rects.first() else {
        return Vec::new();
    };

    // The rectangle that covers every monitor
    let left = rects.iter().map(|r| r.x()).min().unwrap_or(0);
    let top = rects.iter().map(|r| r.y()).min().unwrap_or(0);
    let right = rects.iter().map(|r| r.x() + r.width()).max().unwrap_or(0);
    let bottom = rects.iter().map(|r| r.y() + r.height()).max().unwrap_or(0);
    let virtual_screen = gdk::Rectangle::new(left, top, right - left, bottom - top);

    let mut vars = Vec::new();
    let mut add = |name: &str, suffix: &str, r: &gdk::Rectangle| {
        let parts = [("x", r.x()), ("y", r.y()), ("width", r.width()), ("height", r.height())];
        for (part, value) in parts {
            vars.push((format!("{name}{part}{suffix}"), value.to_string()));
        }
    };
    for area in ["screenarea", "workarea"] {
        add(area, "", primary);
        add(&format!("p{area}"), "", primary);
        for (i, r) in rects.iter().enumerate() {
            add(area, &format!("@{}", i + 1), r);
        }
    }
    add("vscreenarea", "", &virtual_screen);
    vars
}
