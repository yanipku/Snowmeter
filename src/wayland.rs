/*  Wayland-only code. GTK covers most of Wayland itself; this holds what it doesn't:
    - Window positions on Wayland. Wayland doesn't let a normal window choose where it goes,
      but desktop widgets and panels can use the "layer shell" protocol: the window belongs
      to one monitor, is stuck to its top and left edges, and margins say how far from them.
      So a position here is a monitor plus two margins, and we always know it, because we set it.
      GNOME has no layer shell: there, and on X11, setup() returns false and nothing changes. 
    - Blur behind the window on KDE Plasma, through KDE's blur protocol
      (org_kde_kwin_blur_manager). Other Wayland desktops don't offer it; then nothing happens.*/
use crate::position::ZPos;
use std::cell::RefCell;
use std::collections::HashMap;
use gdk4_wayland::prelude::*;
use gdk4_wayland::{WaylandDisplay, WaylandSurface};
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_compositor::WlCompositor, wl_region::WlRegion, wl_registry::WlRegistry};
use wayland_client::backend::ObjectId;
use wayland_client::{delegate_noop, Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols_plasma::blur::client::org_kde_kwin_blur::OrgKdeKwinBlur;
use wayland_protocols_plasma::blur::client::org_kde_kwin_blur_manager::OrgKdeKwinBlurManager;

/*  ----------Position-----------------
    Makes `window` a layer-shell window at (x, y), counted from the primary monitor's
    top-left like WindowX/WindowY, on the layer that matches AlwaysOnTop.
    Must run before the window is shown.
    Returns false (and does nothing) when the desktop has no layer shell. */
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

/*  Moves a shown layer-shell window to the layer for `z` (!ZPos). Returns false, doing
    nothing, for any other window. */
pub fn set_z_pos(window: &impl IsA<gtk4::Window>, z: ZPos) -> bool {
    let window = window.as_ref();
    if !window.is_layer_window() {
        return false;
    }
    window.set_layer(layer(z));
    true
}

/*  The layer for an AlwaysOnTop value. Layer windows are never mixed in with normal ones,
    only under them all (Bottom) or over them all (Top), so Normal has to be Bottom.
    The desktop values aren't Background: wallpaper programs draw there and would cover us. */
fn layer(z: ZPos) -> Layer {
    match z {
        ZPos::OnDesktop | ZPos::Bottom | ZPos::Normal => Layer::Bottom,
        ZPos::OnTop => Layer::Top,
        ZPos::Topmost => Layer::Overlay,
    }
}

/*  Moves a layer-shell window so its top-left corner is at (x, y), counted from the primary
    monitor's top-left. The monitor is the one that holds that point (else the primary). */
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

/*  Where a layer-shell window's top-left corner is, counted from the primary monitor's
    top-left. None for any other window. */
pub fn position(window: &impl IsA<gtk4::Window>) -> Option<(i32, i32)> {
    let window = window.as_ref();
    if !window.is_layer_window() {
        return None;
    }
    let r = window.monitor()?.geometry();
    let (ox, oy) = crate::monitors::origin();
    Some((r.x() + window.margin(Edge::Left) - ox, r.y() + window.margin(Edge::Top) - oy))
}

/*  -----------Blur-------------------
    What we need from the compositor, found once on first use. The queue receives the replies
    to our own requests (there are none to read, but Wayland wants a queue for them). */
struct Kwin {
    conn: Connection,
    _queue: EventQueue<Replies>,
    qh: QueueHandle<Replies>,
    manager: OrgKdeKwinBlurManager,
    compositor: WlCompositor,
    blurs: RefCell<HashMap<ObjectId, OrgKdeKwinBlur>>,
}

thread_local! {
    // None: this desktop has no KDE Blur. GTK runs on one thread, so thread_local is enough.
    static KWIN: std::cell::OnceCell<Option<Kwin>> = const { std::cell::OnceCell::new() };
}

/*  The rectangles [x, y, w, h], in window pixels, as for x11::set_blur: an empty list
    means the whole window, and None turns blur off. Does nothing on x11. */
pub fn set_blur(window: &impl IsA<gtk4::Window>, rects: Option<&[[i32; 4]]>) {
    let window = window.as_ref();
    let Some(surface) = window.surface().and_then(|s| s.downcast::<WaylandSurface>().ok()) else { return };
    let Some(wl_surface) = surface.wl_surface() else { return };
    KWIN.with(|kwin| {
        let Some(kwin) = kwin.get_or_init(|| connect(&surface)) else { return };
        match rects {
            None => {
                kwin.manager.unset(&wl_surface);
                if let Some(blur) = kwin.blurs.borrow_mut().remove(&wl_surface.id()) {
                    blur.release();
                }
            }
            Some(rects) => {
                // This window's blur object, made the first time
                let mut blurs = kwin.blurs.borrow_mut();
                let blur = blurs
                    .entry(wl_surface.id())
                    .or_insert_with(|| kwin.manager.create(&wl_surface, &kwin.qh, ()));
                if rects.is_empty() {
                    blur.set_region(None); // no region: the whole window
                } else {
                    let region = kwin.compositor.create_region(&kwin.qh, ());
                    for &[x, y, w, h] in rects {
                        region.add(x, y, w, h);
                    }
                    blur.set_region(Some(&region));
                    region.destroy(); // done with it; the blur keeps its own copy
                }
                blur.commit();
            }
        }
        let _ = kwin.conn.flush();
    });
    // The compositor applies it with the window's next frame
    window.queue_draw();
}

// Asks the compositor for its blur manager. None if it has none (GNOME, labwc, sway, ...).
fn connect(surface: &WaylandSurface) -> Option<Kwin> {
    let display = surface.display().downcast::<WaylandDisplay>().ok()?;
    // GTK's own connection to the compositor, shared with it
    let conn = Connection::from_backend(display.wl_display()?.backend().upgrade()?);
    let (globals, queue) = registry_queue_init::<Replies>(&conn).ok()?;
    let qh = queue.handle();
    let manager = globals.bind(&qh, 1..=1, ()).ok()?;
    let compositor = globals.bind(&qh, 1..=4, ()).ok()?;
    Some(Kwin { conn, _queue: queue, qh, manager, compositor, blurs: RefCell::default() })
}

/*  Wayland asks for a handler for each kind of object we create. None of ours send us
    anything we need, so they all ignore it. */
struct Replies;

impl Dispatch<WlRegistry, GlobalListContents> for Replies {
    fn event(_: &mut Self, _: &WlRegistry, _: <WlRegistry as Proxy>::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}
delegate_noop!(Replies: ignore WlCompositor);
delegate_noop!(Replies: ignore WlRegion);
delegate_noop!(Replies: ignore OrgKdeKwinBlurManager);
delegate_noop!(Replies: ignore OrgKdeKwinBlur);