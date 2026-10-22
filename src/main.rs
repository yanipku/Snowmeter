mod action;
mod clock;
mod config;
mod cpu;
mod ini;
mod measure;
mod memory;
mod meter;
mod mouse;
mod paths;
mod skin;
mod textfile;
mod calc;
mod formula;
mod sectionvar;

use action::After;
use gtk4::prelude::*;
use gtk4::{gdk, glib, Application, ApplicationWindow, DrawingArea};
use mouse::{MouseEvent, Press};
use skin::Skin;
use std::{cell::Cell, cell::RefCell, env, path::Path, rc::Rc, time::Duration};

type SharedSkin = Rc<RefCell<Skin>>;

const USAGE: &str = "Usage: snowmeter [options] <skin>

  <skin>                   a path to an .ini file, or a name inside the skins
                           folder, such as Demo/Clock/Clock.ini
                           (case and \\ or / don't matter)

Options:
  --list                   show the skins that were found
  --skins-dir FOLDER       use this skins folder for this run only
  --set-skins-dir FOLDER   remember this skins folder for future runs
  --help                   show this text

The skins folder is chosen in this order:
  1. --skins-dir
  2. the RAINMETER_SKINS environment variable
  3. SkinPath= in the settings file
  4. Skins inside the settings folder

The settings file is ~/Rainmeter/Rainmeter.ini if it exists ([Rainmeter] section),
otherwise ~/Snowmeter/Snowmeter.ini ([Snowmeter] section), made on the first run.
";

fn fail(msg: &str) -> ! {
    eprintln!("{}", msg);
    std::process::exit(1);
}

fn print_skins(root: &Path, source: &str) {
    println!("Skins folder: {} (from {})", root.display(), source);
    let skins = config::list_skins(root);
    if skins.is_empty() {
        println!("  (no skins yet: put them in <Config>/<Skin>/<File>.ini)");
    }
    for s in skins {
        println!("  {}", s);
    }
}

fn build_ui(app: &Application, skin: SharedSkin) {
    let (width, height, interval) = {
        let s = skin.borrow();
        (s.width, s.height, s.update_ms)
    };

    let area = DrawingArea::builder()
        .content_width(width)
        .content_height(height)
        .build();

    let draw_skin = skin.clone();
    area.set_draw_func(move |_, cr, _w, _h| {
        let s = draw_skin.borrow();
        if let Some(bg) = &s.background {
            meter::set_color(cr, bg);
            let _ = cr.paint();
        }
        for m in &s.meters {
            m.draw(cr, &s.measures);
        }
    });

    let timer_skin = skin.clone();
    let timer_area = area.clone();
    glib::timeout_add_local(Duration::from_millis(interval), move || {
        update_skin(&timer_skin, &timer_area);
        glib::ControlFlow::Continue
    });

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Snowmeter")
        .resizable(false)
        .decorated(false) // no title bar or frame, like a Rainmeter skin
        .child(&area)
        .build();

    // Stop the theme painting a background: only what draw_func paints shows.
    // Transparent pixels need a compositor; without one they show as black.
    let css = gtk4::CssProvider::new();
    css.load_from_data("window { background: transparent; }");
    gtk4::style_context_add_provider_for_display(
        &WidgetExt::display(&window),
        &css,
        gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    // Set while the skin is being moved, so letting go of it isn't a click
    // (Rainmeter's m_Dragged). Rc<Cell>: shared by two closures, changeable by both.
    let dragged = Rc::new(Cell::new(false));

    // Drag with the left button to move the skin. The window manager does the moving,
    // which is the only way that works on Wayland as well as X11.
    let drag = gtk4::GestureDrag::new();
    drag.set_button(gdk::BUTTON_PRIMARY);
    let weak_window = window.downgrade();
    let drag_dragged = dragged.clone();
    drag.connect_drag_update(move |gesture, dx, dy| {
        // A few pixels of wobble is still a click, not a drag
        if dx.hypot(dy) < 4.0 {
            return;
        }
        let Some(window) = weak_window.upgrade() else { return };
        let Some((x, y)) = gesture.start_point() else { return };
        let (Some(device), Some(surface)) = (gesture.device(), window.surface()) else { return };
        if let Ok(toplevel) = surface.downcast::<gdk::Toplevel>() {
            drag_dragged.set(true);
            let button = gesture.current_button() as i32;
            toplevel.begin_move(&device, button, x, y, gesture.current_event_time());
        }
        // The window manager owns the pointer now; end our gesture
        gesture.reset();
    });
    window.add_controller(drag);

    // Mouse buttons: LeftMouseDownAction, RightMouseUpAction and the rest
    let click = gtk4::GestureClick::new();
    click.set_button(0); // 0 = every button
    let press_skin = skin.clone();
    let press_area = area.clone();
    let press_dragged = dragged.clone();
    click.connect_pressed(move |gesture, n_press, x, y| {
        press_dragged.set(false);
        let button = gesture.current_button();
        // As on Windows, every second press in a quick series is a double-click, and
        // without a DoubleClickAction anywhere it counts as a plain press
        let mut events = Vec::new();
        if n_press % 2 == 0 {
            events.extend(MouseEvent::from_button(button, Press::DoubleClick));
        }
        events.extend(MouseEvent::from_button(button, Press::Down));
        mouse_action(&press_skin, &press_area, x, y, &events);
    });
    let release_skin = skin.clone();
    let release_area = area.clone();
    click.connect_released(move |gesture, _, x, y| {
        if dragged.get() {
            return; // the skin was moved, not clicked
        }
        if let Some(event) = MouseEvent::from_button(gesture.current_button(), Press::Up) {
            mouse_action(&release_skin, &release_area, x, y, &[event]);
        }
    });
    window.add_controller(click);

    window.present();
}

// One update of the whole skin: measures, then meters, then the window size, then a
// repaint. Used by the Update= timer and by !Update.
fn update_skin(skin: &SharedSkin, area: &DrawingArea) {
    let new_size = {
        let mut guard = skin.borrow_mut();
        let s = &mut *guard;
        let values = measure::update_all(&mut s.measures);
        // Meters refresh their dynamic options, graphs record, and positions are worked out
        meter::update_all(&mut s.meters, &s.measures, &values);
        // DynamicWindowSize=1: the window follows the meters
        if s.dynamic_window {
            Some(skin::window_size(&s.meters, &s.measures))
        } else {
            None
        }
    };
    // Resize outside the borrow, so nothing can ask to draw while the skin is locked
    if let Some((w, h)) = new_size {
        area.set_content_width(w);
        area.set_content_height(h);
    }
    area.queue_draw();
}

// Runs the action of the first of `events` that anything at (x, y) has an action for
fn mouse_action(skin: &SharedSkin, area: &DrawingArea, x: f64, y: f64, events: &[MouseEvent]) {
    // Copy the action text out, so the read borrow ends before the bangs change the skin
    let text = {
        let s = skin.borrow();
        let found = events
            .iter()
            .find_map(|&e| s.action_at(x, y, e).map(|(name, a)| (e, s.expand_action(name, a))));
        let Some((event, text)) = found else { return };
        println!("{event:?} at ({x:.0}, {y:.0}): {text}");
        text
    }; // `s` is dropped here: the skin is no longer borrowed
    for command in action::split_bangs(&text) {
        // Borrowed for one bang at a time: !Update needs to borrow the skin itself
        let after = action::execute(&mut skin.borrow_mut(), command);
        match after {
            After::Redraw => area.queue_draw(),
            After::Update => update_skin(skin, area),
            After::Nothing => {}
        }
    }
}

fn main() -> glib::ExitCode {
    let mut skins_dir_arg: Option<String> = None;
    let mut set_dir: Option<String> = None;
    let mut list_only = false;
    let mut show_help = false;
    let mut skin_arg: Option<String> = None;

    let mut args = env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--list" | "-l" => list_only = true,
            "--help" | "-h" => show_help = true,
            "--skins-dir" => match args.next() {
                Some(dir) => skins_dir_arg = Some(dir),
                None => fail("--skins-dir needs a folder name"),
            },
            "--set-skins-dir" => match args.next() {
                Some(dir) => set_dir = Some(dir),
                None => fail("--set-skins-dir needs a folder name"),
            },
            _ => skin_arg = Some(a),
        }
    }

    // ~/Rainmeter/Rainmeter.ini if it exists, else ~/Snowmeter/Snowmeter.ini (made now if needed)
    let cfg = config::find_config();
    cfg.create_if_missing().unwrap_or_else(|e| fail(&e));

    // --set-skins-dir: save the choice and stop
    if let Some(dir) = set_dir {
        let file = config::save_skins_dir(&config::expand_home(&dir), &cfg)
            .unwrap_or_else(|e| fail(&e));
        println!("Saved. Future runs will use this skins folder (written to {})", file.display());
        std::process::exit(0);
    }

    let (skins_dir, source) = config::resolve_skins_dir(skins_dir_arg, &cfg);
    let root = config::ensure_skins_dir(&skins_dir).unwrap_or_else(|e| fail(&e));

    // No skin given (or --list / --help): show what's available and stop
    let skin_arg = match skin_arg {
        Some(s) if !list_only && !show_help => s,
        _ => {
            if !list_only {
                println!("{}", USAGE);
            }
            print_skins(&root, source);
            std::process::exit(0);
        }
    };

    let skin_file = config::resolve_skin(&skin_arg, &root).unwrap_or_else(|e| fail(&e));
    let skin = skin::load(&skin_file, &root).unwrap_or_else(|e| fail(&e));
    let skin: SharedSkin = Rc::new(RefCell::new(skin));

    let app = Application::builder()
        .application_id("org.example.snowmeter")
        .build();
    app.connect_activate(move |app| build_ui(app, skin.clone()));
    app.run_with_args::<&str>(&[])
}