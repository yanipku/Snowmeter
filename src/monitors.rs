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
//
// Positions count from the primary's top-left corner, as on Windows, so the primary is
// always at 0,0 and a monitor left of it has a negative X.
pub fn screen_vars() -> Vec<(String, String)> {
    let (ox, oy) = origin();
    let rects: Vec<gdk::Rectangle> = gtk_rects()
        .iter()
        .map(|r| gdk::Rectangle::new(r.x() - ox, r.y() - oy, r.width(), r.height()))
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

// Where the primary monitor's top-left corner is, in GTK's coordinates.
// Windows counts positions from there, so we subtract it everywhere.
// (0, 0) when GTK isn't started or there is no monitor.
pub fn origin() -> (i32, i32) {
    gtk_rects().first().map_or((0, 0), |r| (r.x(), r.y()))
}

// Every monitor's rectangle as GTK gives it (top-left of the whole desktop = 0,0).
// Empty when GTK isn't started (as in `cargo test`) or there is no display.
fn gtk_rects() -> Vec<gdk::Rectangle> {
    if !gtk4::is_initialized() {
        return Vec::new();
    }
    let Some(display) = gdk::Display::default() else {
        return Vec::new();
    };
    let list = display.monitors();
    (0..list.n_items())
        .filter_map(|i| list.item(i).and_downcast::<gdk::Monitor>())
        .map(|m| m.geometry())
        .collect()
}
