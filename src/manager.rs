// The running skins: one window per config (a folder under Skins, like illustro\Clock),
// as in Rainmeter, where at most one skin of each config runs at a time.
use crate::action::{self, After, ConfigBang, Target, WindowBang};
use crate::dollarvar::SkinInfo;
use crate::position::ZPos;
use crate::config::{self, Config, SkinSettings};
use crate::meter;
use crate::mouse::{MouseEvent, Press};
use crate::skin::{self, Skin};
use gtk4::prelude::*;
use gtk4::{gdk, glib, Application, ApplicationWindow, DrawingArea};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::time::Duration;

pub struct Manager {
    app: Application,
    skins_root: PathBuf, // the skins folder
    cfg: Config,         // the settings file, where Active= is saved; its folder is #SETTINGSPATH#
    // Keys are lowercase config names. The map holds the only strong reference to each
    // window's state; the window's own callbacks hold weak ones (see SkinWindow::open).
    skins: RefCell<HashMap<String, Rc<SkinWindow>>>,
}

// Loads a skin file that has no window yet. [$SkinX] and the others see where its settings
// will put it, as in Rainmeter, which works the position out from the options as it
// reads the skin. The size isn't known before loading, so it counts as 0 × 0.
pub fn load_skin(file: &Path, skins_root: &Path, cfg: &Config) -> Result<Skin, String> {
    let folder = file.parent().unwrap_or(Path::new(""));
    let config = folder.strip_prefix(skins_root).unwrap_or(folder).to_string_lossy().replace('/', "\\");
    let settings = config::read_skin_settings(cfg, &config);
    let rects = crate::monitors::monitor_rects();
    let (x, y) = crate::position::resolve(&settings.position, (0, 0), &rects);
    let info = SkinInfo { x, y, w: 0, h: 0, z: settings.z };
    crate::dollarvar::with_skin(info, || skin::load(file, skins_root, &cfg.dir))
}

// ClickThrough: an empty input region, so the mouse goes to the window behind.
// Some(empty) = nowhere; None would mean the whole window.
fn clear_input_region(surface: &gdk::Surface) {
    surface.set_input_region(Some(&gtk4::cairo::Region::create()));
}

// Config names ignore case, and \ or / both work
fn config_key(config: &str) -> String {
    config.trim().replace('/', "\\").to_lowercase()
}

impl Manager {
    pub fn new(app: &Application, skins_root: &Path, cfg: &Config) -> Rc<Manager> {
        Rc::new(Manager {
            app: app.clone(),
            skins_root: skins_root.to_path_buf(),
            cfg: cfg.clone(),
            skins: RefCell::new(HashMap::new()),
        })
    }

    // Shows an already loaded skin in a new window, closing any other skin of its config.
    // `self: &Rc<Self>` lets the method hand the window a pointer back to the manager.
    pub fn add(self: &Rc<Self>, skin: Skin) {
        let key = config_key(&skin.config);
        // Active=N, N counting from 1. A skin opened from outside the skins folder isn't
        // in the list, so nothing is saved for it.
        let name = skin.file.file_name().map(|n| n.to_string_lossy().to_lowercase());
        let files = config::config_files(&self.skins_root, &skin.config);
        let settings = config::read_skin_settings(&self.cfg, &skin.config);
        if let Some(i) = files.iter().position(|f| Some(f.to_lowercase()) == name) {
            self.save_active(&skin.config, i + 1);
            // and, like Rainmeter, write any window setting the file doesn't have yet
            if let Err(e) = config::save_missing_settings(&self.cfg, &skin.config, &settings) {
                eprintln!("Can't save the skin's settings: {e}");
            }
        }
        let rects = crate::monitors::monitor_rects();
        let size = (skin.width, skin.height);
        let mut position = crate::position::resolve(&settings.position, size, &rects);
        if settings.keep_on_screen {
            position = crate::position::keep_on_screen(position, size, rects.get(1..).unwrap_or(&[]));
        }
        let window = SkinWindow::open(&self.app, skin, position, &settings, Rc::downgrade(self));
        // The new window opens before the old one closes, so GTK never sees zero windows
        let old = self.skins.borrow_mut().insert(key, window.clone());
        if let Some(old) = old {
            old.close();
        }
        //OnRefreshAction also runs on first load, once the skin is in the list (so bangs
        // naming its own config find it) and any old skin of its config is gone
        let action = window.skin.borrow().on_refresh.clone();
        window.run_skin_action("Rainmeter", action, &window.in_refresh_action);
    }

    // The running skin of a config
    fn find(&self, config: &str) -> Option<Rc<SkinWindow>> {
        self.skins.borrow().get(&config_key(config)).cloned()
    }

    // Loads a skin of `config`. Without a file, the config's first .ini (Rainmeter
    // would use the one its settings remember; we don't save settings yet).
    // If that very file already runs, its window is only brought to the front.
    fn activate(self: &Rc<Self>, config: &str, file: Option<&str>) -> Result<(), String> {
        let file = match file {
            Some(f) => f.to_string(),
            None => self.first_file(config).ok_or_else(|| format!("\"{config}\" does not exist"))?,
        };
        let path = config::find_in_skins(&format!("{config}/{file}"), &self.skins_root)
            .map_err(|_| format!("\"{config}\\{file}\" does not exist"))?;
        if let Some(running) = self.find(config) {
            if running.skin.borrow().file == path {
                running.window.present();
                return Ok(());
            }
        }
        let skin = load_skin(&path, &self.skins_root, &self.cfg)?;
        self.add(skin);
        Ok(())
    }

    // The first .ini directly inside a config folder, by name
    fn first_file(&self, config: &str) -> Option<String> {
        config::config_files(&self.skins_root, config).into_iter().next()
    }

    // Remembers in the settings file which skin of a config runs (0 = none).
    // A failure is only reported: the skin itself keeps running.
    fn save_active(&self, config: &str, index: usize) {
        if let Err(e) = config::save_active(&self.cfg, config, index) {
            eprintln!("Can't save the loaded skins: {e}");
        }
    }

    // Remembers where a config's window is (WindowX=, WindowY=), keeping the form the
    // options had (20R stays counted from the right, @2 stays on monitor 2, ...)
    // `size` is the skin's, for AnchorX/AnchorY.
    fn save_position(&self, config: &str, pos: (i32, i32), size: (i32, i32)) {
        let opts = config::read_position(&self.cfg, config);
        let rects = crate::monitors::monitor_rects();
        let (x, y) = crate::position::to_options(&opts, pos, size, &rects);
        if let Err(e) = config::save_position(&self.cfg, config, &x, &y) {
            eprintln!("Can't save the skin's position: {e}");
        }
    }

    // Closes a config's skin, if it runs. Dropping the last Rc frees everything; when no
    // window is left, GTK ends the program (later the tray icon will keep it running).
    fn deactivate(&self, config: &str) {
        let removed = self.skins.borrow_mut().remove(&config_key(config));
        if let Some(sw) = removed {
            // The section name from the skin itself, with its real capitalization
            let name = sw.skin.borrow().config.clone();
            self.save_active(&name, 0);
            sw.close();
        }
    }

    // !ActivateConfig, !DeactivateConfig, !ToggleConfig. `this` is the config whose action ran.
    fn run_config_bang(self: &Rc<Self>, bang: ConfigBang, this: &str) {
        let result = match bang {
            ConfigBang::Activate { config, file } => self.activate(&config, file.as_deref()),
            ConfigBang::Toggle { config, file } => {
                if self.find(&config).is_some() {
                    self.deactivate(&config);
                    Ok(())
                } else {
                    self.activate(&config, file.as_deref())
                }
            }
            ConfigBang::Deactivate { config: None } => {
                self.deactivate(this);
                Ok(())
            }
            ConfigBang::Deactivate { config: Some(config) } if config == "*" => {
                let keys: Vec<String> = self.skins.borrow().keys().cloned().collect();
                for key in keys {
                    self.deactivate(&key);
                }
                Ok(())
            }
            ConfigBang::Deactivate { config: Some(config) } => {
                if self.find(&config).is_some() {
                    self.deactivate(&config);
                    Ok(())
                } else if self.first_file(&config).is_none() {
                    Err(format!("\"{config}\" does not exist"))
                } else {
                    Ok(()) // exists but isn't running: nothing to do, as in Rainmeter
                }
            }
        };
        if let Err(e) = result {
            eprintln!("Config bang: {e}");
        }
    }

    // Every running skin. A copy of the list, so the map isn't borrowed while bangs run.
    fn all(&self) -> Vec<Rc<SkinWindow>> {
        self.skins.borrow().values().cloned().collect()
    }
}

// One running skin and its window
struct SkinWindow {
    me: Weak<SkinWindow>, // a weak pointer to itself, for the timer it starts
    manager: Weak<Manager>, // to reach other skins, for bangs like [!Refresh "Config"]
    skin: RefCell<Skin>,
    window: ApplicationWindow,
    area: DrawingArea,
    // The ticket of the running Update timer, kept so it can be stopped and restarted
    timer: RefCell<Option<glib::SourceId>>,
    // Set while the skin is being moved, so letting go of it isn't a click (Rainmeter's m_Dragged)
    dragged: Cell<bool>,
    // Draggable= (a Cell, so a bang can change it later)
    draggable: Cell<bool>,
    // KeepOnScreen=, SnapEdges=, ClickThrough= and AlwaysOnTop=
    keep_on_screen: Cell<bool>,
    snap_edges: Cell<bool>,
    click_through: Cell<bool>,
    z: Cell<ZPos>,
    // Set when the last left press ran a LeftMouseDownAction: as in Rainmeter, that press
    // doesn't start a drag
    press_ran_action: Cell<bool>,
    // Set while OnUpdateAction / OnRefreshAction runs, so one that !Update / !Refresh
    // doesn't start itself again forever
    in_update_action: Cell<bool>,
    in_refresh_action: Cell<bool>,
    // The short wait after a move before saving the position (see position_changed)
    save_timer: RefCell<Option<glib::SourceId>>,
    // The position last saved or loaded, so a resize alone doesn't write the file
    saved_position: Cell<Option<(i32, i32)>>,
}

impl SkinWindow {
    // Rc::new_cyclic hands us a Weak pointer to the SkinWindow before it exists, so the
    // callbacks can be built with it. They hold Weak, not Rc: the window owns its callbacks,
    // so a strong pointer back would be a loop that is never freed.
    // `position` is where WindowX/WindowY put the window.
    fn open(
        app: &Application,
        skin: Skin,
        position: (i32, i32),
        settings: &SkinSettings,
        manager: Weak<Manager>,
    ) -> Rc<SkinWindow> {
        let (width, height) = (skin.width, skin.height);
        let area = DrawingArea::builder().content_width(width).content_height(height).build();
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Snowmeter")
            .resizable(false)
            .decorated(false) // no title bar or frame, like a Rainmeter skin
            .child(&area)
            .build();

        let sw = Rc::new_cyclic(|me: &Weak<SkinWindow>| {
            let draw_me = me.clone();
            area.set_draw_func(move |_, cr, w, h| {
                let Some(sw) = draw_me.upgrade() else { return };
                let s = sw.skin.borrow();
                if let Some(bg) = &s.background {
                    bg.paint(cr, 0.0, 0.0, w as f64, h as f64);
                }
                if let Some(bevel) = &s.bevel {
                    bevel.draw(cr, 0.0, 0.0, w as f64, h as f64, false);
                }
                for m in &s.meters {
                    m.draw(cr, &s.measures);
                }
            });
            SkinWindow {
                me: me.clone(),
                manager,
                skin: RefCell::new(skin),
                window: window.clone(),
                area: area.clone(),
                timer: RefCell::new(None),
                dragged: Cell::new(false),
                draggable: Cell::new(settings.draggable),
                keep_on_screen: Cell::new(settings.keep_on_screen),
                snap_edges: Cell::new(settings.snap_edges),
                click_through: Cell::new(settings.click_through),
                z: Cell::new(settings.z),
                press_ran_action: Cell::new(false),
                in_update_action: Cell::new(false),
                in_refresh_action: Cell::new(false),
                save_timer: RefCell::new(None),
                saved_position: Cell::new(Some(position)),
            }
        });
        sw.start_timer();

        // Save the position when the window has been moved (X11 only)
        let moved_me = Rc::downgrade(&sw);
        crate::x11pos::on_moved(&window, move || {
            if let Some(sw) = moved_me.upgrade() {
                sw.position_changed();
            }
        });

        // Stop the theme painting a background: only what draw_func paints shows.
        // Transparent pixels need a compositor; without one they show as black.
        let css = gtk4::CssProvider::new();
        css.load_from_data("window { background: transparent; }");
        gtk4::style_context_add_provider_for_display(
            &WidgetExt::display(&window),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        // Drag with the left button to move the skin. The window manager does the moving,
        // which is the only way that works on Wayland as well as X11.
        let drag = gtk4::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let weak_window = window.downgrade();
        let drag_me = Rc::downgrade(&sw);
        drag.connect_drag_update(move |gesture, dx, dy| {
            let (Some(window), Some(sw)) = (weak_window.upgrade(), drag_me.upgrade()) else { return };
            if !sw.draggable.get() || sw.press_ran_action.get() {
                return;
            }
            // DragMargins: a press outside the drag area never drags (clicks still work)
            let Some((px, py)) = gesture.start_point() else { return };
            let inside = {
                let s = sw.skin.borrow();
                crate::skin::in_drag_area(s.drag_margins, (s.width, s.height), px, py)
            };
            if !inside{
                return;
            }
            // A few pixels of wobble is still a click, not a drag
            if !sw.dragged.get() && dx.hypot(dy) < 4.0 {
                return;
            }
            // A layer-shell window (Wayland) can't be moved by the window manager: we move
            // it. dx, dy count from the press point inside the window; once the window has
            // moved that much, the mouse is over the same point again, so each update only
            // adds the newest movement.
            if let Some((wx, wy)) = crate::wlpos::position(&window) {
                sw.dragged.set(true);
                // Holding Ctrl or Shift turns snapping off, as in Rainmeter
                let held = gesture.current_event_state();
                let snapping = !held.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK);
                let target = sw.adjust_move((wx + dx as i32, wy + dy as i32), snapping);
                crate::wlpos::move_to(&window, target);
                sw.position_changed(); // saved once the moving stops, as on X11
                return;
            }
            let Some((x, y)) = gesture.start_point() else { return };
            let (Some(device), Some(surface)) = (gesture.device(), window.surface()) else { return };
            if let Ok(toplevel) = surface.downcast::<gdk::Toplevel>() {
                sw.dragged.set(true);
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
        let press_me = Rc::downgrade(&sw);
        click.connect_pressed(move |gesture, n_press, x, y| {
            let Some(sw) = press_me.upgrade() else { return };
            sw.dragged.set(false);
            let button = gesture.current_button();
            // As on Windows, every second press in a quick series is a double-click, and
            // without a DoubleClickAction anywhere it counts as a plain press
            let mut events = Vec::new();
            if n_press % 2 == 0 {
                events.extend(MouseEvent::from_button(button, Press::DoubleClick));
            }
            events.extend(MouseEvent::from_button(button, Press::Down));
            let ran = events.contains(&MouseEvent::LeftDown)
                && sw.skin.borrow().action_at(x, y, MouseEvent::LeftDown).is_some();
            sw.press_ran_action.set(ran);
            sw.mouse_action(x, y, &events);
        });
        let release_me = Rc::downgrade(&sw);
        click.connect_released(move |gesture, _, x, y| {
            let Some(sw) = release_me.upgrade() else { return };
            if sw.dragged.get() {
                return; // the skin was moved, not clicked
            }
            if let Some(event) = MouseEvent::from_button(gesture.current_button(), Press::Up) {
                sw.mouse_action(x, y, &[event]);
            }
        });
        window.add_controller(click);

        // Put the window in place: with the layer shell on Wayland, else by asking the X server
        if !crate::wlpos::setup(&window, position, settings.z) {
            crate::x11pos::move_when_shown(&window, position.0, position.1);
            crate::x11pos::set_z_pos_when_managed(&window, settings.z);
        }

        // ClickThrough=1: an empty input region (the part of the window that gets the
        // mouse), so every click goes to the window behind. Works on X11 and Wayland.
        // Unlike on Windows, holding Ctrl doesn't make it clickable: with no input region
        // we never learn that the mouse is over the skin.
        // GTK sets the input region itself each time it lays the window out, so ours is set
        // again after every layout (our handler is connected after GTK's, so it runs last).
        // It checks the setting each time, since !ClickThrough can change it.
        let region_me = Rc::downgrade(&sw);
        window.connect_realize(move |w| {
            let Some(surface) = w.surface() else { return };
            let me = region_me.clone();
            let clear = move |s: &gdk::Surface| {
                if me.upgrade().is_some_and(|sw| sw.click_through.get()) {
                    clear_input_region(s);
                }
            };
            clear(&surface);
            surface.connect_layout(move |s, _, _| clear(s));
        });
        /*OnFocusAction / OnUnfocusAction: GTK flips the window's "is-active" property
          when it gain or loses keyboard focus */
        let focus_me = Rc::downgrade(&sw);
        window.connect_is_active_notify(move |w| {
            let Some(sw) = focus_me.upgrade() else { return };
            let action = {
                let skin = sw.skin.borrow();
                if w.is_active() { skin.on_focus.clone() } else { skin.on_unfocus.clone() }
            };
            sw.run_skin_action("Rainmeter", action, &Cell::new(false));
        });
        window.present();
        sw
    }

    // !ZPos, !Draggable, !ClickThrough, !KeepOnScreen, !SnapEdges: changes the setting
    // straight away and saves it, as Rainmeter does
    fn apply_window_bang(&self, bang: WindowBang) {
        let bit = |b: bool| if b { "1" } else { "0" }.to_string();
        // Sets a Cell<bool> setting from the bang's 1 / 0 / -1, returning the new value
        let set = |cell: &Cell<bool>, n: i32| {
            let b = action::flag(n, cell.get());
            cell.set(b);
            b
        };
        let (key, value) = match bang {
            WindowBang::ZPos(z) => {
                let old = self.z.replace(z);
                if !crate::wlpos::set_z_pos(&self.window, z) {
                    crate::x11pos::change_z_pos(&self.window, old, z);
                }
                ("AlwaysOnTop", z.number().to_string())
            }
            WindowBang::Draggable(n) => ("Draggable", bit(set(&self.draggable, n))),
            WindowBang::KeepOnScreen(n) => ("KeepOnScreen", bit(set(&self.keep_on_screen, n))),
            WindowBang::SnapEdges(n) => ("SnapEdges", bit(set(&self.snap_edges, n))),
            WindowBang::ClickThrough(n) => {
                let on = set(&self.click_through, n);
                if let Some(surface) = self.window.surface() {
                    if on {
                        clear_input_region(&surface);
                    } else {
                        surface.set_input_region(None); // None: the whole window again
                    }
                }
                ("ClickThrough", bit(on))
            }
            WindowBang::Move(x, y) => return self.move_bang((x, y)),
        };
        let Some(manager) = self.manager.upgrade() else { return };
        let config = self.skin.borrow().config.clone();
        if let Err(e) = config::save_setting(&manager.cfg, &config, key, &value) {
            eprintln!("Can't save {key}: {e}");
        }
    }

    // !Move X Y: puts the skin's top-left corner at (x, y), counted from the primary
    // monitor, and saves WindowX/WindowY as plain numbers, as Rainmeter does. KeepOnScreen
    // applies, but snapping doesn't (Rainmeter snaps only while dragging).
    fn move_bang(&self, pos: (i32, i32)) {
        let Some(manager) = self.manager.upgrade() else { return };
        let config = self.skin.borrow().config.clone();
        let size = self.size();
        let opts = crate::position::moved(&config::read_position(&manager.cfg, &config), pos, size);
        let mut pos = crate::position::resolve(&opts, size, &crate::monitors::monitor_rects());
        pos = self.adjust_move(pos, false);
        // Only one of these does anything: layer-shell windows on Wayland, else X11
        if crate::wlpos::position(&self.window).is_some() {
            crate::wlpos::move_to(&self.window, pos);
        } else {
            crate::x11pos::move_to(&self.window, pos);
        }
        // The move makes X11 report a new position: this stops that from saving it again
        // in the old form
        self.saved_position.set(Some(pos));
        if let Err(e) = config::save_position(&manager.cfg, &config, &opts.x, &opts.y) {
            eprintln!("Can't save the skin's position: {e}");
        }
    }

    // The window moved or changed size. During a drag this comes many times a second,
    // so each call restarts a 500 ms wait, and the position is saved once it stops moving.
    fn position_changed(&self) {
        if let Some(old) = self.save_timer.borrow_mut().take() {
            old.remove();
        }
        let me = self.me.clone();
        let id = glib::timeout_add_local_once(Duration::from_millis(500), move || {
            let Some(sw) = me.upgrade() else { return };
            // This timer has run, so its id must not be removed again
            sw.save_timer.borrow_mut().take();
            sw.save_position_now();
        });
        *self.save_timer.borrow_mut() = Some(id);
    }

    // If a position save is still waiting, does it now. Like Rainmeter's
    // WriteDeferredOptions, this runs before the skin closes or refreshes.
    fn flush_position(&self) {
        let pending = self.save_timer.borrow_mut().take();
        if let Some(timer) = pending {
            timer.remove();
            self.save_position_now();
        }
    }

    // Where a window being moved to `pos` really goes: snapped to the edges of other skins
    // and of the screen (SnapEdges=1, and `snapping`), then kept on screen (KeepOnScreen=1)
    fn adjust_move(&self, mut pos: (i32, i32), snapping: bool) -> (i32, i32) {
        let size = self.size();
        let rects = crate::monitors::monitor_rects();
        let monitors = rects.get(1..).unwrap_or(&[]);
        if snapping && self.snap_edges.get() {
            let others: Vec<gdk::Rectangle> = match self.manager.upgrade() {
                // every skin but this one (compared by address)
                Some(m) => m.all().iter().filter(|o| !std::ptr::eq(Rc::as_ptr(o), self)).filter_map(|o| o.rect()).collect(),
                None => Vec::new(),
            };
            pos = crate::position::snap(pos, size, &others, monitors);
        }
        if self.keep_on_screen.get() {
            pos = crate::position::keep_on_screen(pos, size, monitors);
        }
        pos
    }

    // What [$Skin…] variables see: where the window is now (where it was put, if we
    // can't tell, as on GNOME), its size and AlwaysOnTop
    fn info(&self) -> SkinInfo {
        let (w, h) = self.size();
        let pos = crate::wlpos::position(&self.window).or_else(|| crate::x11pos::position(&self.window));
        let (x, y) = pos.or(self.saved_position.get()).unwrap_or((0, 0));
        SkinInfo { x, y, w, h, z: self.z.get() }
    }

    // The skin's width and height
    fn size(&self) -> (i32, i32) {
        let s = self.skin.borrow();
        (s.width, s.height)
    }

    // Where the window is and how big, if we can tell (X11, or Wayland with layer shell)
    fn rect(&self) -> Option<gdk::Rectangle> {
        let (x, y) = crate::wlpos::position(&self.window).or_else(|| crate::x11pos::position(&self.window))?;
        let (w, h) = self.size();
        Some(gdk::Rectangle::new(x, y, w, h))
    }

    // Saves where the window is, unless that's already saved
    fn save_position_now(&self) {
        let pos = crate::wlpos::position(&self.window).or_else(|| crate::x11pos::position(&self.window));
        let Some(mut pos) = pos else { return };
        // On X11 the window manager did the moving, so SnapEdges and KeepOnScreen can only
        // put the window right now that it has stopped (on Wayland the drag already did)
        let adjusted = self.adjust_move(pos, true);
        if adjusted != pos {
            crate::x11pos::move_to(&self.window, adjusted);
            pos = adjusted;
        }
        if self.saved_position.get() == Some(pos) {
            return; // only resized (DynamicWindowSize), or moved back to where it was
        }
        self.saved_position.set(Some(pos));
        let Some(manager) = self.manager.upgrade() else { return };
        let (config, size) = {
            let s = self.skin.borrow();
            (s.config.clone(), (s.width, s.height))
        };
        manager.save_position(&config, pos, size);
    }

    // Stops the timers and closes the window
    fn close(&self) {
        // OnCloseAction, while the skin is still whole (Rainmeter runs it before tearing
        // the skin down). A config bang in it waits for GTK, so it can't close us twice
        let action = self.skin.borrow().on_close.clone();
        self.run_skin_action("Rainmeter", action, &Cell::new(false));
        self.flush_position();
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        self.window.close();
    }

    // (Re)starts the Update timer with the skin's current Update= value. The old timer,
    // if any, is stopped first. The timer holds a Weak pointer, and stops itself once
    // the skin is gone.
    fn start_timer(&self) {
        if let Some(old) = self.timer.borrow_mut().take() {
            old.remove();
        }
        let interval = self.skin.borrow().update_ms;
        let me = self.me.clone();
        let id = glib::timeout_add_local(Duration::from_millis(interval), move || {
            let Some(sw) = me.upgrade() else { return glib::ControlFlow::Break };
            sw.update();
            glib::ControlFlow::Continue
        });
        *self.timer.borrow_mut() = Some(id);
    }

    // One update of the whole skin: measures, then meters, then the window size, then a
    // repaint. Used by the Update= timer and by !Update.
    fn update(&self) {
        /* Measures one at the time, as in Rainmeter's UpdateMeasures: each one's OnUpdateAction
           run right after it, with the skin let go, so the action can change the measure below */
        let mut values = crate::measure::current_values(&self.skin.borrow().measures);
        let mut i = 0;
        loop {
            // [$Skin] and the others, in dynamic option, describe this skin
            let step = crate::dollarvar::with_skin(self.info(), || {
                let mut s = self.skin.borrow_mut();
                let m = s.measures.get_mut(i)?;  // None: past the last one (or a !Refresh shrank the list)
                let action = crate::measure::update_one(m, &mut values);
                Some((m.name().to_string(), action))
            });
            let Some((name, action)) = step else { break };
            self.run_skin_action(&name, action, &self.in_update_action);
            i += 1;
        }
        /* Meters the same way: each one's OnUpdateAction run right after it update.
           Position start from where every meter is now, for [meter:X] references further down */
        let mut layout = {
            let s = self.skin.borrow();
            meter::Layout::seeded(&s.meters, &s.measures)
        };
        let mut i = 0;
        loop {
            let step = crate::dollarvar::with_skin(self.info(), || {
                let mut guard = self.skin.borrow_mut();
                let s = &mut *guard;
                let m = s.meters.get_mut(i)?;
                // Meters refresh their dynamic options, graph record, and the meter is place
                let action = meter::update_one(m, &s.measures, &values, &mut layout);
                Some((m.name().to_string(), action))
            });
            let Some((name, action)) = step else { break };
            self.run_skin_action(&name, action, &self.in_update_action);
            i += 1;
        }
        //DynamicWindowSize=1: the window follows the meters
        let new_size = crate::dollarvar::with_skin(self.info(), || {
            let s = self.skin.borrow();
            s.dynamic_window.then(|| skin::window_size(&s.meters, &s.measures, s.fixed_size))
        });
        // Resize outside the borrow, so nothing can ask to draw while the skin is locked
        if let Some((w, h)) = new_size {
            self.area.set_content_width(w);
            self.area.set_content_height(h);
        }
        self.area.queue_draw();
        // OnUpdateAction run last, after the redraw, as in Rainmeter
        let action = self.skin.borrow().on_update.clone();
        self.run_skin_action("Rainmeter", action, &self.in_update_action);
    }

    // !Refresh: reads the skin file again and puts the new skin in place of the old one.
    // If the file now has an error, the old skin keeps running.
    // The timer restarts, since Update= may have changed.
    fn refresh(&self) {
        self.flush_position();
        let (file, root, settings) = {
            let s = self.skin.borrow();
            (s.file.clone(), s.skins_root.clone(), s.settings_dir.clone())
        };
        let info = self.info();
        let fresh = match crate::dollarvar::with_skin(info, || skin::load(&file, &root, &settings)) {
            Ok(fresh) => fresh,
            Err(e) => {
                eprintln!("!Refresh: {e}");
                return;
            }
        };
        let (w, h) = (fresh.width, fresh.height);
        *self.skin.borrow_mut() = fresh; // the old skin is dropped here
        self.area.set_content_width(w);
        self.area.set_content_height(h);
        self.area.queue_draw();
        self.start_timer();
        // OnRefreshAction: the skin is loaded again and its timer runs
        let action = self.skin.borrow().on_refresh.clone();
        self.run_skin_action("Rainmeter", action, &self.in_refresh_action);
    }

    // Runs the action of the first of `events` that anything at (x, y) has an action for
    fn mouse_action(&self, x: f64, y: f64, events: &[MouseEvent]) {
        // [$SkinX] and the others in the action describe this skin
        let info = self.info();
        // Copy the action text out, so the read borrow ends before the bangs change the skin
        let text = {
            let s = self.skin.borrow();
            let expand = |name: &str, a: &str| crate::dollarvar::with_skin(info, || s.expand_action(name, a));
            let found = events.iter().find_map(|&e| s.action_at(x, y, e).map(|(name, a)| (e, expand(name, a))));
            let Some((event, text)) = found else { return };
            println!("{event:?} at ({x:.0}, {y:.0}): {text}");
            text
        }; // `s` is dropped here: the skin is no longer borrowed
        self.run_action(&text);
    }

    /*Runs an action: its bangs one after another, then any !Refresh, then (once GTK is free)
    the congif bangs, as Rainmeter does. Used by mouse actions and the [Rainmeter]
    actions (OnUpdateAction and others).*/
    fn run_action(&self, text: &str) {
        let (Some(me), Some(manager)) = (self.me.upgrade(), self.manager.upgrade()) else { return };
        // Like Rainmeter, refreshes wait until the rest of the action has run
        let mut to_refresh: Vec<Rc<SkinWindow>> = Vec::new();
        let mut config_bangs: Vec<ConfigBang> = Vec::new();
        for command in action::split_bangs(text) {
            let targets = match action::target(command) {
                Target::This => vec![me.clone()],
                Target::All => manager.all(),
                Target::Config(config) => match manager.find(&config) {
                    Some(other) => vec![other],
                    None => {
                        self.warn_missing(command, &config);
                        Vec::new()
                    }
                },
            };
            for sw in targets {
                match sw.run(command) {
                    // Each skin once, even if the action refreshes it twice
                    After::Refresh if !to_refresh.iter().any(|r| Rc::ptr_eq(r, &sw)) => {
                        to_refresh.push(sw);
                    }
                    After::Config(bang) => config_bangs.push(bang),
                    _ => {}
                }
            }
        }
        for sw in to_refresh {
            sw.refresh();
        }
        // Loading and closing skins waits until GTK is done with this click, since a skin
        // may close its own window. They run in the order the action gave them.
        if !config_bangs.is_empty() {
            let this = self.skin.borrow().config.clone();
            // Keep the program alive until the bang have run: closiong the last windows
            // would otherwise make GTK quit before a new skin can open
            let hold = manager.app.hold();
            glib::idle_add_local_once(move || {
                for bang in config_bangs {
                    manager.run_config_bang(bang, &this);
                }
                drop(hold);
            });
        }
    }

    /*Runs an action like OnUpdateAction or OnRefreshAction, if it's set. 'section' is where it
      come from ("Rainmeter", or a measure's name), for [#CURRENTSECTION] and the like. 'busy'
      is the guard flag: while it's set, the action is already running and isn't started again.*/
    fn run_skin_action(&self, section: &str, action: Option<String>, busy: &Cell<bool>) {
        let Some(raw) = action else { return };
        if busy.get() {
            return;
        }
        //[#Var], [&Measure] and [$Skin...] are filled in now as for mouse action
        let info = self.info();
        let text = {
            let s = self.skin.borrow();
            crate::dollarvar::with_skin(info, || s.expand_action(section, &raw))
        };
        busy.set(true);
        self.run_action(&text);
        busy.set(false);
    }

    // Runs one bang on this skin. Redraw and Update are done here; Refresh and the
    // config bangs are handed back, for the caller to do when the action is finished.
    fn run(&self, command: &str) -> After {
        // Borrowed for this bang only: !Update needs to borrow the skin itself
        let after = action::execute(&mut self.skin.borrow_mut(), command);
        match after {
            After::Redraw => self.area.queue_draw(),
            After::Window(bang) => self.apply_window_bang(bang),
            After::Update => {
                // As in Rainmeter, the next tick comes a full Update= later
                self.update();
                self.start_timer();
            }
            later => return later,
        }
        After::Nothing
    }

    // A bang named a config that isn't running: a warning if the folder exists, else an error
    fn warn_missing(&self, command: &str, config: &str) {
        let bang = command.split_whitespace().next().unwrap_or(command);
        let folder = self.skin.borrow().skins_root.join(config.replace('\\', "/"));
        if folder.is_dir() {
            eprintln!("{bang}: Skin \"{config}\" is not active");
        } else {
            eprintln!("{bang}: Skin \"{config}\" does not exist");
        }
    }
}
