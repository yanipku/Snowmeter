// Window positions on X11. GTK4 has no way to read or set where a window is, and on
// Wayland no program may, so this asks the X server directly. Every function quietly
// does nothing on Wayland, or when libX11 can't be loaded.
// GTK has deprecated its X11 backend (4.18), so all of it is kept in this one file.
use crate::position::ZPos;
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

// Moves a window that is already on screen to (x, y), counted from the primary monitor's
// top-left. Does nothing on Wayland.
pub fn move_to(window: &impl IsA<gtk4::Window>, (x, y): (i32, i32)) {
    let Some((display, xwindow)) = x11_handles(window) else { return };
    let (ox, oy) = crate::monitors::origin();
    move_xwindow(display, xwindow, x + ox, y + oy);
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

// Moves the window to (x, y) once it is really on screen.
// (x, y) count from the primary monitor's top-left, as position() returns them.
pub fn move_when_shown(window: &impl IsA<gtk4::Window>, x: i32, y: i32) {
    when_shown(window, move |display, xwindow| {
        let (ox, oy) = crate::monitors::origin();
        move_xwindow(display, xwindow, x + ox, y + oy);
    });
}

// Once the window manager has the window, asks it to keep it above or below other
// windows (AlwaysOnTop), and to leave it out of the taskbar and the workspace switcher,
// as Rainmeter skins are on Windows. X11 has no "topmost" or "on desktop" level of its
// own, so Topmost is the same as OnTop, and OnDesktop the same as Bottom.
pub fn set_z_pos_when_managed(window: &impl IsA<gtk4::Window>, z: ZPos) {
    when_managed(window, move |display, xwindow| {
        if let Some(level) = level_state(z) {
            change_states(display, xwindow, ADD, level, None);
        }
        change_states(display, xwindow, ADD, "_NET_WM_STATE_SKIP_TASKBAR", Some("_NET_WM_STATE_SKIP_PAGER"));
    });
}

// Moves a shown window from AlwaysOnTop `old` to `new` (!ZPos): takes the old state off,
// then puts the new one on
pub fn change_z_pos(window: &impl IsA<gtk4::Window>, old: ZPos, new: ZPos) {
    let Some((display, xwindow)) = x11_handles(window) else { return };
    if let Some(state) = level_state(old) {
        change_states(display, xwindow, REMOVE, state, None);
    }
    if let Some(state) = level_state(new) {
        change_states(display, xwindow, ADD, state, None);
    }
}

// The _NET_WM_STATE for an AlwaysOnTop value, if any
fn level_state(z: ZPos) -> Option<&'static str> {
    match z {
        ZPos::OnTop | ZPos::Topmost => Some("_NET_WM_STATE_ABOVE"),
        ZPos::Bottom | ZPos::OnDesktop => Some("_NET_WM_STATE_BELOW"),
        ZPos::Normal => None,
    }
}

// What change_states asks for (EWMH's _NET_WM_STATE_REMOVE and _NET_WM_STATE_ADD)
const REMOVE: std::ffi::c_long = 0;
const ADD: std::ffi::c_long = 1;

// Runs `f` once, when the window is really on screen. The window manager sets a new
// window up as it appears, so a move made before that can be undone;
// GTK sets the surface's "mapped" property when it asks for the window to be shown.
fn when_shown(window: &impl IsA<gtk4::Window>, f: impl Fn(*mut xlib::Display, xlib::Window) + 'static) {
    let f = std::rc::Rc::new(f);
    // The surface exists from "realize" on, which comes just before the window is shown
    window.as_ref().connect_realize(move |w| {
        let Some(surface) = w.surface() else { return };
        let done = std::cell::Cell::new(false);
        let f = f.clone();
        surface.connect_mapped_notify(move |s| {
            // Only the first time: later the user may have dragged it elsewhere
            if s.is_mapped() && !done.replace(true) {
                if let Some((display, xwindow)) = surface_handles(s) {
                    f(display, xwindow);
                }
            }
        });
    });
}

// Runs `f` once, when the window manager has taken the window over. "Mapped" (above) is
// too early for that: the window manager then rewrites the window's states and drops
// what we asked for. The X11 rules (ICCCM) say a window manager puts a WM_STATE property
// on each window it takes over, and X tells us when a property changes, so we wait for that.
// With no window manager, WM_STATE never comes and `f` never runs.
fn when_managed(window: &impl IsA<gtk4::Window>, f: impl Fn(*mut xlib::Display, xlib::Window) + 'static) {
    let f = std::rc::Rc::new(f);
    window.as_ref().connect_realize(move |w| {
        let Some(surface) = w.surface() else { return };
        let Some((xdisplay, xwindow)) = surface_handles(&surface) else { return };
        let Ok(display) = surface.display().downcast::<X11Display>() else { return };
        let Some(wm_state) = intern_atom(xdisplay, "WM_STATE") else { return };
        let f = f.clone();
        let done = std::cell::Cell::new(false);
        // Safety: as in on_moved. `property` is the member filled in for PropertyNotify.
        let id = unsafe {
            display.connect_xevent(move |_, event| {
                let event = &*event;
                if event.type_ == xlib::PropertyNotify
                    && event.property.window == xwindow
                    && event.property.atom == wm_state
                    && event.property.state == xlib::PropertyNewValue
                    && !done.replace(true)
                {
                    f(xdisplay, xwindow);
                }
                gtk4::glib::Propagation::Proceed
            })
        };
        let id = std::cell::Cell::new(Some(id));
        w.connect_unrealize(move |_| {
            if let Some(id) = id.take() {
                display.disconnect(id);
            }
        });
    });
}

// The X server's number (atom) for a name. None if libX11 couldn't be loaded.
fn intern_atom(display: *mut xlib::Display, name: &str) -> Option<xlib::Atom> {
    XLIB.with(|lib| {
        let lib = lib.as_ref()?;
        // Our names are constants with no NUL in them, so CString::new can't fail
        let name = std::ffi::CString::new(name).unwrap();
        // Safety: display is valid (see x11_handles); name is a NUL-terminated string
        // that lives until the call returns. 0 = create the atom if it doesn't exist yet.
        Some(unsafe { (lib.XInternAtom)(display, name.as_ptr(), 0) })
    })
}

// Asks the window manager to add (ADD) or take off (REMOVE) one or two states of a managed window.
// A program may not change that property itself once the window is managed: the
// standard (EWMH) says to send the window manager a ClientMessage on the root window.
fn change_states(display: *mut xlib::Display, xwindow: xlib::Window, how: std::ffi::c_long, first: &str, second: Option<&str>) {
    let atom = |name: &str| intern_atom(display, name).unwrap_or(0) as std::ffi::c_long;
    let mut message = xlib::XClientMessageEvent {
        type_: xlib::ClientMessage,
        serial: 0,
        send_event: 1,
        display,
        window: xwindow,
        message_type: atom("_NET_WM_STATE") as xlib::Atom,
        format: 32, // the data is five 32-bit numbers ("longs")
        data: xlib::ClientMessageData::new(),
    };
    let data = message.data.as_longs_mut();
    data[0] = how;
    data[1] = atom(first);
    data[2] = second.map_or(0, atom);
    data[3] = 1; // the request comes from a normal program
    let mut event = xlib::XEvent::from(message);
    XLIB.with(|lib| {
        let Some(lib) = lib.as_ref() else { return };
        // Safety: as in position(); the event is our own local, which XSendEvent only reads.
        unsafe {
            let root = (lib.XDefaultRootWindow)(display);
            let mask = xlib::SubstructureRedirectMask | xlib::SubstructureNotifyMask;
            (lib.XSendEvent)(display, root, 0, mask, &mut event);
            (lib.XFlush)(display);
        }
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
