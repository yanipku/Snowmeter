// Rainmeter's [$Skin…] and [$Display…] variables: the running skin's position and size,
// and the monitors. Examples: [$SkinX], [$SkinZPos], [$DisplayW], [$Display2WorkAreaH],
// [$DisplayCount], [$DisplayVirtualScreenW]. Follows ConfigParser::GetDollarSkinVariable
// and GetDollarDisplayVariable.
//
// The skin code doesn't know its window, so the window sets a SkinInfo with with_skin()
// while its skin updates, runs an action or loads, and lookup() reads it. Outside of that
// the variables are unknown, and stay as written.
use crate::position::ZPos;
use gtk4::gdk;
use gtk4::prelude::*; // brings in MonitorExt: scale_factor(), model(), connector()
use std::cell::Cell;

// What the window tells us about the skin being worked on
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinInfo {
    pub x: i32, // top-left corner, counted from the primary monitor's top-left
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub z: ZPos,
}

thread_local! {
    // The skin being worked on. GTK runs on one thread, so a thread_local is enough.
    static CURRENT: Cell<Option<SkinInfo>> = const { Cell::new(None) };
}

// Runs `f` with `info` as the skin the variables describe
pub fn with_skin<R>(info: SkinInfo, f: impl FnOnce() -> R) -> R {
    let old = CURRENT.replace(Some(info));
    let result = f();
    CURRENT.set(old);
    result
}

// The value of [$name] ("SkinX", "Display2W", …; case doesn't matter), or None if the
// name is unknown or no skin is being worked on
pub fn lookup(name: &str) -> Option<String> {
    let info = CURRENT.get()?;
    let name = name.to_ascii_lowercase();
    // The same lists as monitors::monitor_rects() and gtk_monitors(): [0] of rects covers
    // every monitor, then both list the monitors, the primary first
    let rects = crate::monitors::monitor_rects();
    let monitors = crate::monitors::gtk_monitors();
    if let Some(rest) = name.strip_prefix("skin") {
        skin_var(rest, &info, &rects, &monitors)
    } else if let Some(rest) = name.strip_prefix("display") {
        display_var(rest, &info, &rects, &monitors)
    } else {
        None
    }
}

// [$SkinX], [$SkinPhysicalW], [$SkinZPos], …
fn skin_var(rest: &str, info: &SkinInfo, rects: &[gdk::Rectangle], monitors: &[gdk::Monitor]) -> Option<String> {
    let scale = skin_monitor(info, rects).and_then(|i| monitors.get(i)).map_or(1, |m| m.scale_factor());
    let rect = gdk::Rectangle::new(info.x, info.y, info.w, info.h);
    if let Some(v) = component(rest, &rect, 1) {
        return Some(v);
    }
    match rest {
        _ if rest.starts_with("physical") => component(&rest["physical".len()..], &rect, scale),
        "zoomedw" => Some(info.w.to_string()), // no Zoom= yet: always 100%
        "zoomedh" => Some(info.h.to_string()),
        "zpos" => Some(info.z.number().to_string()),
        "dpifactor" => Some(scale.to_string()),
        "zoomfactor" => Some("1".to_string()),
        "visible" => Some("1".to_string()), // we can't tell if other windows cover it
        _ => None,
    }
}

// [$DisplayX], [$Display2Name], [$DisplayDevice1DpiFactor], [$DisplayWorkAreaPhysicalH],
// [$DisplayCount], [$DisplayVirtualScreenW], …
fn display_var(rest: &str, info: &SkinInfo, rects: &[gdk::Rectangle], monitors: &[gdk::Monitor]) -> Option<String> {
    let mut rest = rest;
    let mut index = take_number(&mut rest);
    // "Device" numbers are Windows' display adapters; here they are the same as displays
    if index.is_none() {
        if let Some(r) = rest.strip_prefix("device") {
            rest = r;
            index = take_number(&mut rest);
        }
    }
    if index.is_none() && rest == "count" {
        return Some(monitors.len().to_string());
    }
    // The virtual screen covers every monitor, so it takes no number
    if index.is_none() {
        if let Some(r) = rest.strip_prefix("virtualscreen") {
            // Monitors may have different scales, so Physical gives the same numbers
            let all = rects.first().copied().unwrap_or(gdk::Rectangle::new(0, 0, 0, 0));
            return component(r.strip_prefix("physical").unwrap_or(r), &all, 1);
        }
    }

    // A numbered monitor, else the skin's own one. One that doesn't exist reads as zeros,
    // as in Rainmeter.
    let number = match index {
        Some(n) => (n >= 1 && n <= monitors.len()).then_some(n),
        None => skin_monitor(info, rects),
    };
    let rect = number.and_then(|n| rects.get(n)).copied().unwrap_or(gdk::Rectangle::new(0, 0, 0, 0));
    let monitor = number.and_then(|n| monitors.get(n - 1));
    let scale = monitor.map_or(1, |m| m.scale_factor());

    match rest {
        "name" => return Some(monitor.and_then(monitor_name).unwrap_or_default()),
        "number" => return Some(number.unwrap_or(0).to_string()),
        "dpifactor" => return Some(scale.to_string()),
        _ => {}
    }
    // GTK4 has no work area (the screen minus panels), so it is the whole screen for now
    let rest = rest.strip_prefix("workarea").unwrap_or(rest);
    match rest.strip_prefix("physical") {
        Some(r) => component(r, &rect, scale),
        None => component(rest, &rect, 1),
    }
}

// "x", "y", "w" or "h" of `rect`, times `scale` (the monitor's scale factor, for the
// Physical variables). Anything else: None.
fn component(rest: &str, rect: &gdk::Rectangle, scale: i32) -> Option<String> {
    let v = match rest {
        "x" => rect.x(),
        "y" => rect.y(),
        "w" => rect.width(),
        "h" => rect.height(),
        _ => return None,
    };
    Some((v * scale).to_string())
}

// The number (1 = primary) of the monitor the skin's centre is on, else the primary
fn skin_monitor(info: &SkinInfo, rects: &[gdk::Rectangle]) -> Option<usize> {
    if rects.len() < 2 {
        return None;
    }
    let (cx, cy) = (info.x + info.w / 2, info.y + info.h / 2);
    let found = rects[1..].iter().position(|r| r.contains_point(cx, cy));
    Some(found.map_or(1, |i| i + 1))
}

// A monitor's name: its model if known ("DELL U2720Q"), else its connector ("HDMI-1")
fn monitor_name(m: &gdk::Monitor) -> Option<String> {
    m.model().or_else(|| m.connector()).map(|s| s.to_string())
}

// Takes the whole number at the start of `rest`, if there is one
fn take_number(rest: &mut &str) -> Option<usize> {
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    let n = rest[..digits].parse().ok()?;
    *rest = &rest[digits..];
    Some(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_outside_a_skin() {
        assert_eq!(lookup("SkinX"), None);
    }

    #[test]
    fn skin_variables() {
        // No GTK in tests, so no monitors: the skin's own values still work
        let info = SkinInfo { x: 120, y: 80, w: 300, h: 200, z: ZPos::OnTop };
        with_skin(info, || {
            assert_eq!(lookup("SkinX").as_deref(), Some("120"));
            assert_eq!(lookup("skiny").as_deref(), Some("80"));
            assert_eq!(lookup("SkinW").as_deref(), Some("300"));
            assert_eq!(lookup("SkinH").as_deref(), Some("200"));
            assert_eq!(lookup("SkinPhysicalW").as_deref(), Some("300"));
            assert_eq!(lookup("SkinZPos").as_deref(), Some("1"));
            assert_eq!(lookup("SkinWobble"), None);
            assert_eq!(lookup("DisplayCount").as_deref(), Some("0"));
            assert_eq!(lookup("Display3W").as_deref(), Some("0")); // no such monitor: zeros
        });
        assert_eq!(lookup("SkinX"), None); // gone again afterwards
    }

    #[test]
    fn numbers_in_names() {
        let mut rest = "12workareah";
        assert_eq!(take_number(&mut rest), Some(12));
        assert_eq!(rest, "workareah");
        let mut rest = "name";
        assert_eq!(take_number(&mut rest), None);
        assert_eq!(rest, "name");
    }
}
