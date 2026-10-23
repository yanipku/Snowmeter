// Window positions on X11. GTK4 has no way to read or set where a window is, and on
// Wayland no program may, so this asks the X server directly. Every function quietly
// does nothing on Wayland, or when libX11 can't be loaded.
// GTK has deprecated its X11 backend (4.18), so all of it is kept in this one file.
use gdk4_x11::x11::xlib;
use gdk4_x11::{X11Display, X11Surface};
use gtk4::prelude::*;

thread_local! {
    // libX11, loaded the first time it's needed. GTK runs on one thread, so a
    // thread_local is enough; None if the library isn't there.
    static XLIB: Option<xlib::Xlib> = xlib::Xlib::open().ok();
}

// The X connection and X window behind a GTK window, if it is an X11 window
fn x11_handles(window: &impl IsA<gtk4::Window>) -> Option<(*mut xlib::Display, xlib::Window)> {
    surface_handles(&window.as_ref().surface()?)
}

// The same, starting from the window's GDK surface
fn surface_handles(surface: &gdk4_x11::gdk::Surface) -> Option<(*mut xlib::Display, xlib::Window)> {
    let surface = surface.clone().downcast::<X11Surface>().ok()?;
    let display = surface.display().downcast::<X11Display>().ok()?;
    // Safety: the pointer belongs to GTK's open X connection, which stays open while the
    // window exists. We only pass it to Xlib, never free it.
    let xdisplay = unsafe { display.xdisplay() };
    Some((xdisplay, surface.xid()))
}

// The screen position of the window's top-left corner, counted from the primary
// monitor's top-left as on Windows (see monitors::origin)
pub fn position(window: &impl IsA<gtk4::Window>) -> Option<(i32, i32)> {
    let (display, xwindow) = x11_handles(window)?;
    XLIB.with(|x| {
        let x = x.as_ref()?;
        let (mut sx, mut sy, mut child) = (0, 0, 0);
        // Safety: display and xwindow are valid (see x11_handles); the out-pointers are
        // our own locals. Converts the window's (0, 0) into root-window (screen) coordinates.
        let ok = unsafe {
            let root = (x.XDefaultRootWindow)(display);
            (x.XTranslateCoordinates)(display, xwindow, root, 0, 0, &mut sx, &mut sy, &mut child)
        };
        let (ox, oy) = crate::monitors::origin();
        (ok != 0).then_some((sx - ox, sy - oy))
    })
}

// Calls `moved` each time the window is moved or resized. X sends a ConfigureNotify event
// for that; during a drag many arrive each second, so the caller should wait for them to stop.
pub fn on_moved(window: &impl IsA<gtk4::Window>, moved: impl Fn() + 'static) {
    let moved = std::rc::Rc::new(moved);
    window.as_ref().connect_realize(move |w| {
        let Some(surface) = w.surface() else { return };
        let Some((_, xwindow)) = surface_handles(&surface) else { return };
        let Ok(display) = surface.display().downcast::<X11Display>() else { return };
        let moved = moved.clone();
        // Safety: GTK passes a valid event for the length of the call, and we only read it.
        // XEvent is a C union: the `type_` field says which of its members is filled in.
        let id = unsafe {
            display.connect_xevent(move |_, event| {
                let event = &*event;
                if event.type_ == xlib::ConfigureNotify && event.configure.window == xwindow {
                    moved();
                }
                gtk4::glib::Propagation::Proceed // let GTK handle the event as usual
            })
        };
        // The listener is on the display, which outlives the window: remove it with the window
        let id = std::cell::Cell::new(Some(id));
        w.connect_unrealize(move |_| {
            if let Some(id) = id.take() {
                display.disconnect(id);
            }
        });
    });
}

// Moves the window to (x, y) once it is really on screen. The window manager places a
// new window as it appears, so a move made before that is undone; GTK sets the surface's
// "mapped" property when the X server says the window is up and placed.
// (x, y) count from the primary monitor's top-left, as position() returns them.
pub fn move_when_shown(window: &impl IsA<gtk4::Window>, x: i32, y: i32) {
    let window = window.as_ref();
    // The surface exists from "realize" on, which comes just before the window is shown
    window.connect_realize(move |w| {
        let Some(surface) = w.surface() else { return };
        let done = std::cell::Cell::new(false);
        surface.connect_mapped_notify(move |s| {
            // Only the first time: later the user may have dragged it elsewhere
            if s.is_mapped() && !done.replace(true) {
                if let Some((display, xwindow)) = surface_handles(s) {
                    let (ox, oy) = crate::monitors::origin();
                    move_xwindow(display, xwindow, x + ox, y + oy);
                }
            }
        });
    });
}

fn move_xwindow(display: *mut xlib::Display, xwindow: xlib::Window, x: i32, y: i32) {
    XLIB.with(|lib| {
        let Some(lib) = lib.as_ref() else { return };
        // Safety: as in position(). XFlush sends the request now instead of waiting.
        unsafe {
            (lib.XMoveWindow)(display, xwindow, x, y);
            (lib.XFlush)(display);
        }
    });
}
