/*  The running skins: one window per config (a folder under Skins, like illustro\Clock),
    as in Rainmeter, where at most one skin of each config runs at a time. */
use crate::action::{self, After, ConfigBang, Pick, Target, WindowBang};
use crate::dollarvar::SkinInfo;
use crate::position::ZPos;
use crate::config::{self, Config, SkinSettings};
use crate::lang::tr;
use crate::meter;
use crate::mouse::{MouseEvent, Press};
use crate::skin::{self, ContextItem, Skin};
use crate::log::{log, Level};
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib, Application, ApplicationWindow, DrawingArea};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::{Rc, Weak};
use std::time::Duration;

pub struct Manager {
    app: Application,
    skins_root: PathBuf, // the skins folder
    cfg: Config,         /* the settings file, where Active= is saved; its folder is #SETTINGSPATH#
                            Keys are lowercase config names. The map holds the only strong reference to each
                            window's state; the window's own callbacks hold weak ones (see SkinWindow::open). */
    skins: RefCell<HashMap<String, Rc<SkinWindow>>>,
}

/*  Loads a skin file that has no window yet. [$SkinX] and the others see where its settings
    will put it, as in Rainmeter, which works the position out from the options as it
    reads the skin. The size isn't known before loading, so it counts as 0 × 0. */
pub fn load_skin(file: &Path, skins_root: &Path, cfg: &Config) -> Result<Skin, String> {
    let folder = file.parent().unwrap_or(Path::new(""));
    let config = folder.strip_prefix(skins_root).unwrap_or(folder).to_string_lossy().replace('/', "\\");
    let settings = config::read_skin_settings(cfg, &config);
    let rects = crate::monitors::monitor_rects();
    let (x, y) = crate::position::resolve(&settings.position, (0, 0), &rects);
    let info = SkinInfo { x, y, w: 0, h: 0, z: settings.z };
    let source = skin::log_source(&config, file);
    crate::log::with_source(&source, || crate::dollarvar::with_skin(info, || skin::load(file, skins_root, &cfg.dir)))
}

/*  ClickThrough: an empty input region, so the mouse goes to the window behind.
    Some(empty) = nowhere; None would mean the whole window. */
fn clear_input_region(surface: &gdk::Surface) {
    surface.set_input_region(Some(&gtk4::cairo::Region::create()));
}

// Config names ignore case, and \ or / both work
fn config_key(config: &str) -> String {
    config.trim().replace('/', "\\").to_lowercase()
}

/*  Opens a file or folder with the desktop's default program for it, as Windows'
    ShellExecute does for Rainmeter. GIO asks the desktop which program to use (xdg-mime)
    and starts it directly, never through a shell. a free function, because the program 
    menu (Edit settings, Open folder) has no skin window to ask. */
fn launch_default(path: &Path) -> Result<(), glib::Error> {
    let uri = gio::File::for_path(path).uri();
    let context = gdk::Display::default().map(|d| d.app_launch_context());
    gio::AppInfo::launch_default_for_uri(&uri, context.as_ref())
}

impl Manager {
    pub fn new(app: &Application, skins_root: &Path, cfg: &Config) -> Rc<Manager> {
        let manager = Rc::new(Manager {
            app: app.clone(),
            skins_root: skins_root.to_path_buf(),
            cfg: cfg.clone(),
            skins: RefCell::new(HashMap::new()),
        });
        manager.add_app_actions();
        manager
    }

    /*  The program menu's commands ("app.exit" and the others), set up for the whole
        program. Gtk looks "app." actions up on the application, so every skin's menu finds
        them. They hold a weak ppointer, like the window callbacks do. */
    fn add_app_actions(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        let add = |name: &str, run: fn(&Rc<Manager>)| {
            let me = me.clone();
            let action = gio::SimpleAction::new(name, None);
            action.connect_activate(move |_, _| {
                let me = me.clone();
                /*  Runs once GTK is idle, as Rainmeter post there as messages: the menu that
                    was clicked finishes closing before any skin goer away. */
                glib::idle_add_local_once(move || {
                    if let Some(manager) = me.upgrade() {
                        run(&manager)
                    }
                });
            });
            self.app.add_action(&action);
        };
        add("exit", |m| m.quit());
        add("refreshall", |m| {
            for sw in m.all() {
                sw.refresh();
            }
        });
        add("editsettings", |m| m.open_default(&m.cfg.file));
        add("openskinsfolder", |m| m.open_default(&m.skins_root));
        // Item that don't work yet name this disable action, so GTK greys them out
        let later = gio::SimpleAction::new("later", None);
        later.set_enabled(false);
        self.app.add_action(&later);
    }

    // Launch_default for the program menu; there's no skin to name in an error
    fn open_default(&self, path: &Path) {
        if let Err(e) = launch_default(path) {
            log(Level::Error, "", &format!("Unable to open{}: {e}", path.display()));
        }
    }

    /*  Shows an already loaded skin in a new window, closing any other skin of its config.
        `self: &Rc<Self>` lets the method hand the window a pointer back to the manager. */
    pub fn add(self: &Rc<Self>, mut skin: Skin) {
        let key = config_key(&skin.config);
        /*  Active=N, N counting from 1. A skin opened from outside the skins folder isn't
            in the list, so nothing is saved for it. */
        let name = skin.file.file_name().map(|n| n.to_string_lossy().to_lowercase());
        let files = config::config_files(&self.skins_root, &skin.config);
        let index = files.iter().position(|f| Some(f.to_lowercase()) == name);
        /*  A config loaded for the first time as no settings yet. Asked before Active= is
            written, since that makes the section.Only skins with Default... keys read the file. */
        let first_run = index.is_some() && !skin.defaults.is_empty() && !config::has_settings(&self.cfg, &skin.config);
        if let Some(i) = index {
            self.save_active(&skin.config, i + 1);
        }
        /*  Then, on a first load, the skin's own Default... settings, below Active= as in 
        Rainmeter, so the settings read just below already see them */
        if first_run
            && let Err(e) = config::save_options(&self.cfg, &skin.config, &skin.defaults)
        {
            log(Level::Error, &skin.source, &format!("Can't save the skin's default settings: {e}"));
        }
        let settings = config::read_skin_settings(&self.cfg, &skin.config);
        skin.groups.add(&settings.group);
        // and, like Rainmeter, write any window settings the file doesn't have yet
        if index.is_some()
            && let Err(e) = config::save_missing_settings(&self.cfg, &skin.config, &settings) 
        {
            log(Level::Error, &skin.source, &format!("Can't save the skin's settings: {e}"));
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
        // The first update, at load as in Rainmeter, before OnRefreshAction (with Update=-1 it's the only one)
        window.update();
        window.apply_blur();    // Blur=1, right after the first update, as in Rainmeter
        /*  OnRefreshAction also runs on first load, once the skin is in the list (so bangs
            naming its own config find it) and any old skin of its config is gone */
        let action = window.skin.borrow().on_refresh.clone();
        window.run_skin_action("Rainmeter", action, &window.in_refresh_action);
    }

    // The running skin of a config
    fn find(&self, config: &str) -> Option<Rc<SkinWindow>> {
        self.skins.borrow().get(&config_key(config)).cloned()
    }

    /*  Loads a skin of `config`. Without a file, the config's first .ini (Rainmeter
        would use the one its settings remember; we don't save settings yet).
        If that very file already runs, its window is only brought to the front. */
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

    /*  Remembers in the settings file which skin of a config runs (0 = none).
        A failure is only reported: the skin itself keeps running. */
    fn save_active(&self, config: &str, index: usize) {
        if let Err(e) = config::save_active(&self.cfg, config, index) {
            log(Level::Error, "", &format!("Can't save the loaded skins: {e}"));
        }
    }

    /*  Remembers where a config's window is (WindowX=, WindowY=), keeping the form the
        options had (20R stays counted from the right, @2 stays on monitor 2, ...)
        `size` is the skin's, for AnchorX/AnchorY. */
    fn save_position(&self, config: &str, pos: (i32, i32), size: (i32, i32)) {
        let opts = config::read_position(&self.cfg, config);
        let rects = crate::monitors::monitor_rects();
        let (x, y) = crate::position::to_options(&opts, pos, size, &rects);
        if let Err(e) = config::save_position(&self.cfg, config, &x, &y) {
            log(Level::Error, "", &format!("Can't save the skin's position: {e}"));
        }
    }

    /*  Closes a config's skin, if it runs. Dropping the last Rc frees everything; when no
        window is left, GTK ends the program (later the tray icon will keep it running). */
    fn deactivate(&self, config: &str) {
        let removed = self.skins.borrow_mut().remove(&config_key(config));
        if let Some(sw) = removed {
            // The section name from the skin itself, with its real capitalization
            let name = sw.skin.borrow().config.clone();
            self.save_active(&name, 0);
            sw.close();
        }
    }

    /*  Quitting (Ctrl + C, SIGTERM, ...): closes every skin, then ends the program. As in 
        Rainmeter's DeleteAllSkins, each skin leaves the list just before it closes, so its
        OnCloseAction can still reach the others. Active= is kept, so the same skins come
        back next time. */
    pub fn quit(&self) {
        loop {
            // The first config by name; the borrow ends with this line
            let first = self.skins.borrow().keys().min().cloned();
            let Some(key) = first else { break };
            let removed = self.skins.borrow_mut().remove(&key);
            if let Some(sw) = removed {
                sw.close();
            }
        }
        // quit() ignores app.hold(), so config bangs from an OnCloseAction don't run 
        self.app.quit();
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
            ConfigBang::DeactivateGroup { group } => {
                // "" is every skin, as Rainmeter's GetSkinsByLoadOrder
                let members = if group.is_empty() { self.all() } else {self.in_group(&group) };
                for sw in members {
                    let config = sw.skin.borrow().config.clone();
                    self.deactivate(&config);
                }
                Ok(())
            }
        };
        if let Err(e) = result {
            log(Level::Error, "", &format!("Config bang: {e}"));
        }
    }

    // Every running skin. A copy of the list, so the map isn't borrowed while bangs run.
    fn all(&self) -> Vec<Rc<SkinWindow>> {
        let skins = self.skins.borrow();
        let mut keys: Vec<&String> = skins.keys().collect();
        keys.sort();
        keys.into_iter().map(|k| skins[k].clone()).collect()
    }

    // the running skins in a group (the skin's Group= or the settings file's), in the same order
    fn in_group(&self, group: &str) -> Vec<Rc<SkinWindow>> {
        self.all().into_iter().filter(|sw| sw.skin.borrow().groups.matches(group)).collect()
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
    // SavePosition=: off, moves still happen but WindowX/WindowY aren't written
    save_position: Cell<bool>,
    click_through: Cell<bool>,
    z: Cell<ZPos>,
    /*  Set when the last left press ran a LeftMouseDownAction: as in Rainmeter, that press
        doesn't start a drag */
    press_ran_action: Cell<bool>,
    /*  Set while OnUpdateAction / OnRefreshAction runs, so one that !Update / !Refresh
        doesn't start itself again forever */
    in_update_action: Cell<bool>,
    in_refresh_action: Cell<bool>,
    // Counts refreshes, so the mouse_move loop can stop when an action refreshed the skin
    refreshes: Cell<u32>,
    // Where the mouse is in the window (None = outside), for scroll events, which carry on position
    pointer: Cell<Option<(f64, f64)>>,
    // our last answer to GTK's tooltip question was "show one", so updates refresh its text
    tip_shown: Cell<bool>,
    // The short wait after a move before saving the position (see position_changed)
    save_timer: RefCell<Option<glib::SourceId>>,
    // The position last saved or loaded, so a resize alone doesn't write the file
    saved_position: Cell<Option<(i32, i32)>>,
    // The right-click menu while it's open (None = closed); mouse actions wait until it closes
    menu: RefCell<Option<gtk4::PopoverMenu>>,
    // The focus state the skin was last told about (OnFocusAction / OnUnfocusAction)
    focused: Cell<bool>,
}

impl SkinWindow {
    /*  Rc::new_cyclic hands us a Weak pointer to the SkinWindow before it exists, so the
        callbacks can be built with it. They hold Weak, not Rc: the window owns its callbacks,
        so a strong pointer back would be a loop that is never freed.
        `position` is where WindowX/WindowY put the window. */
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
                    bg.draw(cr,w as f64, h as f64);
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
                save_position: Cell::new(settings.save_position),
                click_through: Cell::new(settings.click_through),
                z: Cell::new(settings.z),
                press_ran_action: Cell::new(false),
                in_update_action: Cell::new(false),
                in_refresh_action: Cell::new(false),
                refreshes: Cell::new(0),
                pointer: Cell::new(None),
                tip_shown: Cell::new(false),
                save_timer: RefCell::new(None),
                saved_position: Cell::new(Some(position)),
                menu: RefCell::new(None),
                focused: Cell::new(false),
            }
        });
        sw.start_timer();

        // Save the position when the window has been moved (X11 only)
        let moved_me = Rc::downgrade(&sw);
        crate::x11::on_moved(&window, move || {
            if let Some(sw) = moved_me.upgrade() {
                sw.position_changed();
            }
        });

        /*  Stop the theme painting a background: only what draw_func paints shows.
            Transparent pixels need a compositor; without one they show as black. */
        let css = gtk4::CssProvider::new();
        // Tooltip icon in theme's colours (symbolic icon are painted in the css colour)
        css.load_from_data(
            "window { background: transparent; }
            .snow-dialog-information, .snow-dialog-question { color: @accent_color; }
            .snow-dialog-warning { color: @warning_color; }
            .snow-dialog-error { color: @error_color; }
            .snow-security-high { color: @success_color; }",
        );
        gtk4::style_context_add_provider_for_display(
            &WidgetExt::display(&window),
            &css,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        /*  Our colors for a theme that doesn't name these: at FALLBACK priority, below the 
            them, so the theme's own @warning_color and friends win when it has them */
        let fallback = gtk4::CssProvider::new();
        fallback.load_from_data(
            "@define-color accent_color #1088c8;
             @define-color warning_color #f6d32d;
             @define-color error_color #e01b24;
             @define-color success_color #018306;",
        );
        gtk4::style_context_add_provider_for_display(
            &WidgetExt::display(&window), 
            &fallback, 
        gtk4::STYLE_PROVIDER_PRIORITY_FALLBACK,
        );

        /*  Drag with the left button to move the skin. The window manager does the moving,
            which is the only way that works on Wayland as well as X11. */
        let drag = gtk4::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        let weak_window = window.downgrade();
        let drag_me = Rc::downgrade(&sw);
        drag.connect_drag_update(move |gesture, dx, dy| {
            let (Some(window), Some(sw)) = (weak_window.upgrade(), drag_me.upgrade()) else { return };
            if !sw.draggable.get() || sw.press_ran_action.get() || sw.menu.borrow().is_some() {
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
            /*  A layer-shell window (Wayland) can't be moved by the window manager: we move
                it. dx, dy count from the press point inside the window; once the window has
                moved that much, the mouse is over the same point again, so each update only
                adds the newest movement. */
            if let Some((wx, wy)) = crate::wayland::position(&window) {
                sw.dragged.set(true);
                // Holding Ctrl or Shift turns snapping off, as in Rainmeter
                let held = gesture.current_event_state();
                let snapping = !held.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::SHIFT_MASK);
                let target = sw.adjust_move((wx + dx as i32, wy + dy as i32), snapping);
                crate::wayland::move_to(&window, target);
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
            /*  As on Windows, every second press in a quick series is a double-click, and
                without a DoubleClickAction anywhere it counts as a plain press */
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
            let Some(event) = MouseEvent::from_button(gesture.current_button(), Press::Up) else { return };
            /*  As in Rainmeter, a right-click opens the menu when no RightMouseUpAction ran,
                and always when Ctrl is held. Ctrl also skips the click's own action, as in Rainmeter. */
            let ctrl = gesture.current_event_state().contains(gdk::ModifierType::CONTROL_MASK);
            let ran = !ctrl && sw.mouse_action(x, y, &[event]);
            if event == MouseEvent::RightUp && !ran {
                sw.show_menu(x, y);
            }
        });
        window.add_controller(click);

        /*  The right-click menu's command. GTK menu items don't call our functions: each
            names an action ("skin.refresh"), which GTK looks up in the action groups of the 
            window the menu opened in. Ours run the same bangs a skin could. */
        let actions = gio::SimpleActionGroup::new();
        for (name, bang) in [("refresh", "!Refresh"), ("unload", "!DeactivateConfig")] {
            let action = gio::SimpleAction::new(name, None);
            let menu_me = Rc::downgrade(&sw);
            action.connect_activate(move |_, _| {
                if let Some(sw) = menu_me.upgrade() {
                    sw.run_action(bang);
                }
            });
            actions.add_action(&action);
        }
        /*  The skin's name opens its folder in the file manager, as in Rainmeter. GIO asks the 
            desktop which program opens folders (xdg-mine: inode/directory) and starts it directly,
            never through a shell, so odd characters in a path can't run anything. */
        let folder = gio::SimpleAction::new("openfolder", None);
        let folder_me = Rc::downgrade(&sw);
        folder.connect_activate(move |_, _| {
            let Some(sw) = folder_me.upgrade() else { return };
            let folder = sw.skin.borrow().file.parent().map(Path::to_path_buf); //borrow ends here
            if let Some(folder) = folder {
                sw.open_default(&folder);
            }
        });
        actions.add_action(&folder);
        // Edit skin opens the skin's .ini in the desktop's default program for text files
        let edit = gio::SimpleAction::new("edit", None);
        let edit_me = Rc::downgrade(&sw);
        edit.connect_activate(move |_, _| {
            let Some(sw) = edit_me.upgrade() else { return };
            let file = sw.skin.borrow().file.clone(); // borrow ends here
            sw.open_default(&file);
        });
        actions.add_action(&edit);
        /*  The custom items (ContextTitle/ContextAction). They all share one action; the menu item
            sends its number along (the "target"). the action text is read only now, at the click,
            as in Rainmeter, so it uses the Variables of this moment. */
        let context = gio::SimpleAction::new("context", Some(glib::VariantTy::INT32));
        let context_me = Rc::downgrade(&sw);
        context.connect_activate(move |_, target| {
            let Some(sw) = context_me.upgrade() else { return };
            let Some(n) = target.and_then(|t| t.get::<i32>()) else { return };
            let action = sw.skin.borrow().context_action(n as usize); // borrow ends here, before run_action
            if let Some(action) = action {
                sw.run_action(&action);
            }
        });
        actions.add_action(&context);
        window.insert_action_group("skin", Some(&actions));

        /*  GTK's rule: a widget with a popover child must give it its size each time the widget
            is laid out (gtk_popover_present). Buttons do it themselves; a drawing area doesn't, 
            so without this the menu keeps the too-small size GTK guessed when it first opened. */
        let present_me = Rc::downgrade(&sw);
        area.connect_resize(move |_, _, _| {
            let Some(sw) = present_me.upgrade() else { return };
            if let Some(menu) = sw.menu.borrow().as_ref() {
                menu.present();
            }
        });

        /*  MouseOverAction / MouseLeaveAction. GTK tells us when the mouse comes onto the 
            window (enter), moves orver it (motion) and goes off it (leave). with ClickThrough
            there's no input region, so none of there arrive: no hover (unlike Wondows + Ctrl). */
        let motion = gtk4::EventControllerMotion::new();
        let enter_me = Rc::downgrade(&sw);
        motion.connect_enter(move |_, x, y| {
            if let Some(sw) = enter_me.upgrade() {
                sw.move_actions(Some((x, y)));
            }
        });
        let motion_me = Rc::downgrade(&sw);
        motion.connect_motion(move |_, x, y| {
            if let Some(sw) = motion_me.upgrade() {
                sw.move_actions(Some((x, y)));
            }
        });
        let leave_me = Rc::downgrade(&sw);
        motion.connect_leave(move |_| {
            if let Some(sw) = leave_me.upgrade() {
                sw.move_actions(None);
            }
        });
        window.add_controller(motion);

        /*  MouseScrollUpAction and the other three. DISCRETE makes GTK add up a touchpad's
            small moves into whole steps, like a wheel's notches, so each step runs one 
            action, as each WM_MOUSEWHEEL does in Rainmeter. A scroll event has no position,
            so we use the last one the motion controller saw. */
        let scroll = gtk4::EventControllerScroll::new(
            gtk4::EventControllerScrollFlags::BOTH_AXES | gtk4::EventControllerScrollFlags::DISCRETE,
        );
        let scroll_me = Rc::downgrade(&sw);
        scroll.connect_scroll(move |_, dx, dy| {
            if let Some(sw) = scroll_me.upgrade() {
                if let (Some((x,y)), Some(event)) = (sw.pointer.get(), MouseEvent::from_scroll(dx, dy)) {
                    sw.mouse_action(x, y, &[event]);
                }
            }
            glib::Propagation::Stop
        });
        window.add_controller(scroll);

        /*  Tooltips (ToolTipText). GTK asks "what tooltip here?" when the mouse rests on the 
            skin; we answer with the top meter's, and give its box as the tip area, so GTK asks
            again once the mouse leaves that meter. Return false means no tooltip here. */
        area.set_has_tooltip(true);
        let tip_me = Rc::downgrade(&sw);
        area.connect_query_tooltip(move |widget, x, y, _keyboard, tooltip| {
            let Some(sw) = tip_me.upgrade() else { return false };
            let s = sw.skin.borrow();
            let found = meter::tooltip_at(&s.meters, &s.measures, x as f64, y as f64, s.tooltip_hidden);
            sw.tip_shown.set(found.is_some());
            let Some(tip) = found else {
                return false;
            };
            // Our own widget instead of GTK's plain label, laid out like a windows tooltip
            tooltip.set_custom(Some(&crate::tooltip::build(&tip, widget)));
            let r = tip.area;
            tooltip.set_tip_area(&gdk::Rectangle::new(r.x.floor() as i32, r.y.floor() as i32, r.w.ceil() as i32, r.h.ceil() as i32));
            true
        });

        // Put the window in place: with the layer shell on Wayland, else by asking the X server
        if !crate::wayland::setup(&window, position, settings.z) {
            crate::x11::move_when_shown(&window, position.0, position.1);
            crate::x11::set_z_pos_when_managed(&window, settings.z);
        }

        /*  ClickThrough=1: an empty input region (the part of the window that gets the
            mouse), so every click goes to the window behind. Works on X11 and Wayland.
            Unlike on Windows, holding Ctrl doesn't make it clickable: with no input region
            we never learn that the mouse is over the skin.
            GTK sets the input region itself each time it lays the window out, so ours is set
            again after every layout (our handler is connected after GTK's, so it runs last).
            It checks the setting each time, since !ClickThrough can change it. */
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
          when it gains or loses keyboard focus */
        let focus_me = Rc::downgrade(&sw);
        window.connect_is_active_notify(move |_| {
            if let Some(sw) = focus_me.upgrade() {
                sw.focus_changed();
            }
        });
        window.present();
        sw
    }

    /*  !ZPos, !Draggable, !ClickThrough, !KeepOnScreen, !SnapEdges: changes the setting
        straight away and saves it, as Rainmeter does */
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
                if !crate::wayland::set_z_pos(&self.window, z) {
                    crate::x11::change_z_pos(&self.window, old, z);
                }
                ("AlwaysOnTop", z.number().to_string())
            }
            WindowBang::Draggable(n) => ("Draggable", bit(set(&self.draggable, n))),
            WindowBang::KeepOnScreen(n) => ("KeepOnScreen", bit(set(&self.keep_on_screen, n))),
            WindowBang::SnapEdges(n) => ("SnapEdges", bit(set(&self.snap_edges, n))),
            WindowBang::ClickThrough(n) => {
                let on = action::flag(n, self.click_through.get());
                self.set_click_through(on);
                ("ClickThrough", bit(on))
            }
            WindowBang::Move(x, y) => return self.move_bang((x, y)),
        };
        let Some(manager) = self.manager.upgrade() else { return };
        let config = self.skin.borrow().config.clone();
        if let Err(e) = config::save_setting(&manager.cfg, &config, key, &value) {
            self.log(Level::Error, &format!("Can't save {key}: {e}"));
        }
    }

    /*  Sets ClickThrough= on the window: an empty input region (every click goes to the
        window behinde), or None for the whole window again. Used by !ClickThrough and
        !Refresh; the realize handler in open() keeps it after GTK's layouts. */
    fn set_click_through(&self, on: bool) {
        self.click_through.set(on);
        let Some(surface) = self.window.surface() else { return };
        if on {
            clear_input_region(&surface);
        } else {
            surface.set_input_region(None); // None: the whole window again
        }
    }

    /*  Put the window settings read from the settings file into effect, as Rainmeter's
        Refresh does after reading them again. (At load, opoen() starts with them.) */
    fn apply_settings(&self, s: &SkinSettings) {
        self.draggable.set(s.draggable);
        self.keep_on_screen.set(s.keep_on_screen);
        self.snap_edges.set(s.snap_edges);
        self.save_position.set(s.save_position);
        self.set_click_through(s.click_through);

        // The layer' only when it changed (as Rainmeter: oldZPos != m_WindowZPosition)
        let old = self.z.replace(s.z);
        if old != s.z && !crate::wayland::set_z_pos(&self.window, s.z) {
            crate::x11::change_z_pos(&self.window, old, s.z);
        }

        /*  The saved position, work out with the skin's new size (R/B and anchors depend
            on it), then kept on screen. With SavePosition=0 this is how the skin goes back
            to its saved place, as in Rainmeter. */
        let size = self.size();
        let rects = crate::monitors::monitor_rects();
        let mut pos = crate::position::resolve(&s.position, size, &rects);
        if s.keep_on_screen {
            pos = crate::position::keep_on_screen(pos, size, rects.get(1..).unwrap_or(&[]));
        }
        self.move_window(pos);
        // so the move X11 reports back isn't saved again (see move_bang)
        self.saved_position.set(Some(pos));
    }

    //Move the window: layer-shell window on wayland, else X11 (only one does anything)
    fn move_window(&self, pos: (i32, i32)) {
        if crate::wayland::position(&self.window).is_some() {
            crate::wayland::move_to(&self.window, pos);
        } else {
            crate::x11::move_to(&self.window, pos);
        } 
    }

    /*  !Move X Y: puts the skin's top-left corner at (x, y), counted from the primary
        monitor, and saves WindowX/WindowY as plain numbers, as Rainmeter does. KeepOnScreen
        applies, but snapping doesn't (Rainmeter snaps only while dragging). */
    fn move_bang(&self, pos: (i32, i32)) {
        let Some(manager) = self.manager.upgrade() else { return };
        let config = self.skin.borrow().config.clone();
        let size = self.size();
        let opts = crate::position::moved(&config::read_position(&manager.cfg, &config), pos, size);
        let mut pos = crate::position::resolve(&opts, size, &crate::monitors::monitor_rects());
        pos = self.adjust_move(pos, false);
        self.move_window(pos);
        /*  The move makes X11 report a new position: this stops that from saving it again
            in the old form */
        self.saved_position.set(Some(pos));
        if self.save_position.get()
            && let Err(e) = config::save_position(&manager.cfg, &config, &opts.x, &opts.y) 
        {
            self.log(Level::Error, &format!("Can't save the skin's position: {e}"));
        }
    }

    /*  The window moved or changed size. During a drag this comes many times a second,
        so each call restarts a 500 ms wait, and the position is saved once it stops moving. */
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

    /*  If a position save is still waiting, does it now. Like Rainmeter's
        WriteDeferredOptions, this runs before the skin closes or refreshes. */
    fn flush_position(&self) {
        let pending = self.save_timer.borrow_mut().take();
        if let Some(timer) = pending {
            timer.remove();
            self.save_position_now();
        }
    }

    /*  Where a window being moved to `pos` really goes: snapped to the edges of other skins
        and of the screen (SnapEdges=1, and `snapping`), then kept on screen (KeepOnScreen=1) */
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

    /*  What [$Skin…] variables see: where the window is now (where it was put, if we
        can't tell, as on GNOME), its size and AlwaysOnTop */
    fn info(&self) -> SkinInfo {
        let (w, h) = self.size();
        let pos = crate::wayland::position(&self.window).or_else(|| crate::x11::position(&self.window));
        let (x, y) = pos.or(self.saved_position.get()).unwrap_or((0, 0));
        SkinInfo { x, y, w, h, z: self.z.get() }
    }

    /*  Runs 'f' as this skin's work: [$Skin] variables describe this window, and
        messages from log::skin_log() name this skin */
    fn as_skin<R>(&self, f: impl FnOnce() -> R) -> R {
        let source = self.skin.borrow().source.clone();
        crate::log::with_source(&source, || crate::dollarvar::with_skin(self.info(), f))
    }

    // log a message with this skin as source
    fn log(&self, level: Level, message: &str) {
        log(level, &self.skin.borrow().source, message);
    }

    // The skin's width and height
    fn size(&self) -> (i32, i32) {
        let s = self.skin.borrow();
        (s.width, s.height)
    }

    // Where the window is and how big, if we can tell (X11, or Wayland with layer shell)
    fn rect(&self) -> Option<gdk::Rectangle> {
        let (x, y) = crate::wayland::position(&self.window).or_else(|| crate::x11::position(&self.window))?;
        let (w, h) = self.size();
        Some(gdk::Rectangle::new(x, y, w, h))
    }

    // Saves where the window is, unless that's already saved
    fn save_position_now(&self) {
        let pos = crate::wayland::position(&self.window).or_else(|| crate::x11::position(&self.window));
        let Some(mut pos) = pos else { return };
        // On X11 the window manager did the moving, so SnapEdges and KeepOnScreen can only
        // put the window right now that it has stopped (on Wayland the drag already did)
        let adjusted = self.adjust_move(pos, true);
        if adjusted != pos {
            crate::x11::move_to(&self.window, adjusted);
            pos = adjusted;
        }
        if self.saved_position.get() == Some(pos) {
            return; // only resized (DynamicWindowSize), or moved back to where it was
        }
        self.saved_position.set(Some(pos));
        if !self.save_position.get() {
            return; // SavePosition=0: the skin moved, but the file keeps the old place
        }
        let Some(manager) = self.manager.upgrade() else { return };
        let (config, size) = {
            let s = self.skin.borrow();
            (s.config.clone(), (s.width, s.height))
        };
        manager.save_position(&config, pos, size);
    }

    /*  The menu's "Save position": writes SavePosition=0/1. Turning it on also saves where
        the skin is now, as Rainmeter's SetSavePosition does. */
    fn set_save_position(&self, on: bool) {
        self.save_position.set(on);
        let Some(manager) = self.manager.upgrade() else { return };
        let config = self.skin.borrow().config.clone();
        if let Err(e) = config::save_setting(&manager.cfg, &config, "SavePosition", if on { "1" } else { "0" }) {
            self.log(Level::Error, &format!("Can't save SavePosition: {e}"));
        }
        // saved_position kept followinf the window, even while nothing was written
        if on && let Some(pos) = self.saved_position.get() {
            manager.save_position(&config, pos, self.size());
        }
    }

    // Stops the timers and closes the window
    fn close(&self) {
        /*  OnCloseAction, while the skin is still whole (Rainmeter runs it before tearing
            the skin down). A config bang in it waits for GTK, so it can't close us twice */
        let action = self.skin.borrow().on_close.clone();
        self.run_skin_action("Rainmeter", action, &Cell::new(false));
        self.flush_position();
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        /*  An open menu goes first: GTK wants a widget's children taken off (unparented)
            before the widget itself is freed */
        if let Some(menu) = self.menu.take() {
            menu.unparent();
        }
        crate::wayland::set_blur(&self.window, None); // frees KWin's blur object
        self.window.close();
    }

    /*  (Re)starts the Update timer with the skin's current Update= value. The old timer,
        if any, is stopped first. The timer holds a Weak pointer, and stops itself once
        the skin is gone. */
    fn start_timer(&self) {
        if let Some(old) = self.timer.borrow_mut().take() {
            old.remove();
        }
        let Some(interval) = self.skin.borrow().update_ms else {
            return; // !setUpdate -1: the old timer is gone and no new one starts
        };
        let me = self.me.clone();
        let id = glib::timeout_add_local(Duration::from_millis(interval), move || {
            let Some(sw) = me.upgrade() else { return glib::ControlFlow::Break };
            sw.update();
            glib::ControlFlow::Continue
        });
        *self.timer.borrow_mut() = Some(id);
    }

    /*  One update of the whole skin: measures, then meters, then the window size, then a
        repaint. Used by the Update= timer and by !Update. */
    fn update(&self) {
        let mut values = crate::measure::current_values(&self.skin.borrow().measures);
        self.update_measures(&mut values, &Pick::All);
        self.update_meters(&values, &Pick::All);
        self.fit_window();
        self.area.queue_draw();
        // A tooltip already showing bet the new measure values (only while the mouse is on the skin)
        if self.tip_shown.get() && self.pointer.get().is_some() {
            self.area.trigger_tooltip_query();
        }
        // OnUpdateAction run last, after the redraw, as in Rainmeter
        let action = self.skin.borrow().on_update.clone();
        self.run_skin_action("Rainmeter", action, &self.in_update_action);
    }

    /*  Updates the measure 'pick' names, in order, as Rainmeter's UpdateMeasure: all, the 
        first with that name (any case), or every one in a group. Each one's OnUpdateAction
        run right after it, with the skin let go, so the action can change the measures 
        below. 'values' get the new values, for the meter. Return whther any was picked. */
    fn update_measures(&self, values: &mut crate::measure::Values, pick: &Pick) -> bool {
        let mut found = false;
        let mut i = 0;
        loop {
            // [%Skin] and the others, in dynamic options, describe this skin
            let step = self.as_skin(|| {
                let mut s = self.skin.borrow_mut();
                let m = s.measures.get_mut(i)?; // None: past the last one (or a !Refresh shrank the list)
                let picked = match pick {
                    Pick::All => true,
                    Pick::Name(name) => m.name().eq_ignore_ascii_case(name),
                    Pick::Group(group) => m.switch_mut().is_some_and(|sw| sw.in_group(group)),
                };
                if !picked {
                    return Some(None); // not picked: left as it is
                }
                found = true;
                let actions = crate::measure::update_one(m, values);
                Some(Some((m.name().to_string(), actions)))
            });
            let Some(step) = step else { break };
            if let Some((section, actions)) = step {
                for action in actions {
                    self.run_skin_action(&section, Some(action), &self.in_update_action);
                }
            }
            if found && matches!(pick, Pick::Name(_)) {
                break; // a name picks one measure: done
            }
            i += 1;
        }
        found
    }

    /*  The same for meters, as Rainmeter's UpdateMeter. The meters not picked are only placed
        agin: in Rainmeter a relative X/Y follows the meter before it at draw time, so a meter
        after one that change size must move. Positions start from where every meter is now,
        for [Meter:X] references to meter further down. */
    fn update_meters(&self, values: &crate::measure::Values, pick: &Pick) -> bool {
        let mut found = false;
        let mut layout = {
            let s = self.skin.borrow();
            meter::Layout::seeded(&s.meters, &s.measures)
        };
        let mut i = 0;
        loop {
            let step = self.as_skin(|| {
                let mut guard = self.skin.borrow_mut();
                let s = &mut *guard;
                let m = s.meters.get_mut(i)?;
                let picked = match pick {
                    Pick::All => true,
                    Pick::Name(name) => m.name().eq_ignore_ascii_case(name),
                    Pick::Group(group) => m.in_group(group),
                };
                if !picked {
                    layout.place(m, &s.measures);
                    return Some(None);
                }
                found = true;
                // Dynamic options read again, graph history, then the meter is place
                let action = meter::update_one(m, &s.measures, values, &mut layout);
                Some(Some((m.name().to_string(), action)))
            });
            let Some(step) = step else { break };
            if let Some((section, action)) = step {
                self.run_skin_action(&section, action, &self.in_update_action);
            }
            i += 1;
        }
        found
    }

    /*  DynamicWindowSize=1: the window follows the meters. Used after an update and by !Redraw,
        since Rainmeter's redraw resize the window when !UpdateMeter changed a meter. */
    fn fit_window(&self) {
        let new_size = self.as_skin(|| {
            let s = self.skin.borrow();
            s.dynamic_window.then(|| skin::window_size(&s.meters, &s.measures, s.fixed_size, s.background.as_ref(), s.background_margins))
        });
        // Resize outside the borrow, so nothig can ask to draw while the skin is locked
        if let Some((w, h)) = new_size {
            self.area.set_content_width(w);
            self.area.set_content_height(h);
        }
    }

    /*  !Refresh: reads the skin file again and puts the new skin in place of the old one.
        If the file now has an error, the old skin keeps running.
        The timer restarts, since Update= may have changed. */ 
    fn refresh(&self) {
        self.flush_position();
        let (file, root, settings) = {
            let s = self.skin.borrow();
            (s.file.clone(), s.skins_root.clone(), s.settings_dir.clone())
        };
        let mut fresh = match self.as_skin(|| skin::load(&file, &root, &settings)) {
            Ok(fresh) => fresh,
            Err(e) => {
                self.log(Level::Error, &format!("!Refresh: {e}"));
                return;
            }
        };
        // Rainmeter.ini is read again too: its Group= joins the skin's own, as in Rainmeter
        let window_settings = self.manager.upgrade().map(|m| config::read_skin_settings(&m.cfg, &fresh.config));
        if let Some(ws) = &window_settings {
            fresh.groups.add(&ws.group);
        }
        let (w, h) = (fresh.width, fresh.height);
        *self.skin.borrow_mut() = fresh; // the old skin is dropped here
        self.refreshes.set(self.refreshes.get().wrapping_add(1));
        self.area.set_content_width(w);
        self.area.set_content_height(h);
        if let Some(ws) = &window_settings {
            self.apply_settings(ws);
        }
        self.update(); // the first skin's first update, as at load (update() also asks for the redraw)
        self.apply_blur(); 
        self.start_timer();
        // OnRefreshAction: the skin is loaded again and its timer runs
        let action = self.skin.borrow().on_refresh.clone();
        self.run_skin_action("Rainmeter", action, &self.in_refresh_action);
    }

    /*  Blur=1: tells the compositor which part of the window to blur, or to stop.
        Kwin's property on x11, KDE's blur protocol on Wayland */
    fn apply_blur(&self) {
        let rects = self.skin.borrow().blur_rects();
        crate::x11::set_blur(&self.window, rects.as_deref());
        crate::wayland::set_blur(&self.window, rects.as_deref());
    }

    /*  OnFocusAction / OnUnfocusAction, only when the focus really changed. While the right-click 
        menu is open, GTK moves the focus between the skin and the menu's own windows; Windows
        doesn't, so those changes are skipped, and the state is checked again when the menu closes. */
    fn focus_changed(&self) {
        if self.menu.borrow().is_some() {
            return;
        }
        let active = self.window.is_active();
        // replace() stores the new state and give back the old one
        if self.focused.replace(active) == active {
            return;
        }
        let action = {
            let skin = self.skin.borrow();
            if active { skin.on_focus.clone() } else { skin.on_unfocus.clone() }
        };
        self.run_skin_action("Rainmeter", action, &Cell::new(false));
    }

    /*  MouseOverAction / MouseLeaveAction after the mouse moved to 'pos' in the window (None = 
        it left the window). As in Rainmeter's OnMouseMove, every leave runs first, then every
        over, one action at a time, checking again after each one. */
    fn move_actions(&self, pos: Option<(f64, f64)>) {
        if self.menu.borrow().is_some() {
            return; // the menu has the mouse now
        }
        let start = self.refreshes.get();
        for step in [MouseEvent::Leave, MouseEvent::Over] {
            loop {
                self.pointer.set(pos);  // remembered for scroll actions
                // the borrow ends with this line, before the bangs change the skin
                let Some((name, raw)) = self.skin.borrow_mut().next_move_action(pos, step) else { break };
                // [#Var], [&Measure] and [$Skin...] are filled in now, as for clicks
                let text = {
                    let s = self.skin.borrow();
                    self.as_skin(|| s.expand_action(&name, &raw))
                };
                self.log(Level::Debug, &format!("{step:?} on {name}: {text}"));
                self.run_action(&text);
                /*  An action that refreshed the skin starts it again with every flag off: stop
                    here, or MouseOverAction=[!Refresh] would run forever. the next mouse move
                    carries on, as after Rainmeter's delayed refresh. */
                if self.refreshes.get() != start {
                    return;
                }
            }
        }
    }

    /*  Runs the action of the first of `events` that anything at (x, y) has an action for
        Returns whether one ran (none = a right-click opens the menu). */
    fn mouse_action(&self, x: f64, y: f64, events: &[MouseEvent]) -> bool {
        if self.menu.borrow().is_some() {
            return false; // the menu has the mouse
        }
        // Copy the action text out, so the read borrow ends before the bangs change the skin
        let text = {
            let s = self.skin.borrow();
            let expand = |name: &str, a: &str| self.as_skin(|| s.expand_action(name, a));
            let found = events.iter().find_map(|&e| s.action_at(x, y, e).map(|(name, a)| (e, expand(name, a))));
            let Some((event, text)) = found else { return false };
            self.log(Level::Debug, &format!("{event:?} at ({x:.0}, {y:.0}): {text}"));
            text
        }; // `s` is dropped here: the skin is no longer borrowed
        self.run_action(&text);
        true
    }

    /*  Open a file or folder with the desktop's default program for it, as Windows'
        ShellExectue does for Rainmeter. Used by the skin's name and, next, by Edit skin. */
    fn open_default(&self, path: &Path) {
        if let Err(e) = launch_default(path) {
            self.log(Level::Error, &format!("Unable to open {}: {e}", path.display()));
        }
    }

    /*  the right-click menu, opend at (x, y) like Rainmeter's skin menu. For now: the skin's
        name and two commands; the rest comes in later steps. A GTK menu has two parts: a
        gio::Menu (the list of labels, each naming an action such as "skin.refresh") and a 
        PopoverMenu (the widget that show it). Each section gets a line between it and the next. */
fn show_menu(&self, x: f64, y: f64) {
    let name = self.skin.borrow().config.clone();
    let custom = self.skin.borrow().context_items(); // title read now, as in Rainmeter

    let title = gio::Menu::new();
    title.append(Some(&menu_label(&name)), Some("skin.openfolder")); 
    let commands = gio::Menu::new();
    commands.append(Some(&tr("SkinMenu", "EditSkin")), Some("skin.edit"));
    commands.append(Some(&tr("SkinMenu", "RefreshSkin")), Some("skin.refresh"));
    let unload = gio::Menu::new();
    unload.append(Some(&tr("SkinMenu", "UnloadSkin")), Some("skin.unload"));
    let model = gio::Menu::new();
    model.append_section(None, &title);
    let variants = gio::Menu::new();
    variants.append_submenu(Some(&tr("SkinMenu", "Variants")), &self.variants_menu());
    let tree = self.skins_menu(&variants); // also adds the skin's root folder below Variants
    model.append_section(None, &variants);
    append_custom(&model, &custom);
    let settings = gio::Menu::new();
    settings.append_submenu(Some(&tr("SkinMenu", "Settings")), &self.settings_menu());
    model.append_section(None, &settings);
    model.append_section(None, &commands);
    let program = gio::Menu::new();
    program.append_submenu(Some("Snowmeter"), &program_menu(tree));
    model.append_section(None, &program);
    model.append_section(None, &unload);

    self.popup(&model, x, y);
    }

    /*  Shows a menu at (x, y) on the skin: the right-click menu, and later !SkinMenu
        and !SkinCustomMenu. Only one menu can be open at a time. */
    fn popup(&self, model: &gio::Menu, x: f64, y: f64) {
        if self.menu.borrow().is_some() {
            return; // one at a time
        }
        // As in Rainmeter, the hover ends first (MouseLeaveAvtion run), since the menu takes the mouse
        self.move_actions(None);
        self.area.set_has_tooltip(false); // no tooltip while the menu is open, as in Rainmeter

        let popover = gtk4::PopoverMenu::from_model_full(model, gtk4::PopoverMenuFlags::NESTED);
        popover.set_parent(&self.area);
        // Like a Windows menu: no arrow, top-left corner at the mouse
        popover.set_has_arrow(false);
        popover.set_halign(gtk4::Align::Start);
        popover.set_position(gtk4::PositionType::Bottom);
        popover.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        /*  Once it close, take it off the skin and let mouse action work again. This waits
            until GTK is idle: the clicked item's action may run after "closed", and GTK finds
            the action through the popover's parent, so the popover must still be attached. */
        let me = self.me.clone();
        popover.connect_closed(move |_| {
            let me = me.clone();
            glib::idle_add_local_once(move || {
                let Some(sw) = me.upgrade() else { return };
                if let Some(menu) = sw.menu.take() {
                    menu.unparent();
                }
                sw.area.set_has_tooltip(true);
                sw.focus_changed(); // the focus may have moved while the menu was open
            });
        });
        self.menu.replace(Some(popover.clone())); // stored first, so the resize handler already finds it
        popover.popup();
    }

    /*  !SkinCustomMenu: a menu of only the skin's ContextTitle items, as Rainmeter's 
        ShowSkinCustomContextMenu: no heading, and no submenu however there are.
        A skin without custom items show nothing. */
    fn custom_menu(&self, x: f64, y: f64) {
        let items = self.skin.borrow().context_items(); //titles read now, as in the full menu
        let model = custom_sections(&items);
        if model.n_items() > 0 {
            self.popup(&model, x, y);
        }
    }

    /*  The Settings submenu, as Rainmeter's Create SkinSettingsMenu. Its actions ("set.")
        are made fresh each time the menu opens, from the values of this moment, so GTK
        draws the right check marks and radio dot. A click runs the same bang a skin could. */
    fn settings_menu(&self) -> gio::Menu {
        let actions = gio::SimpleActionGroup::new();

        /*  position: one action whose state is the zpos number. Each item carries its own
            number (its target), and GTK puts the radio dot on the one that match the state. */
        let zpos = gio::SimpleAction::new_stateful("zpos", Some(glib::VariantTy::INT32), &self.z.get().number().to_variant());
        let me = self.me.clone();
        zpos.connect_activate(move |_, target| {
            let (Some(sw), Some(n)) = (me.upgrade(), target.and_then(|t| t.get::<i32>())) else { return };
            sw.run_action(&format!("!ZPos {n}"));
        });
        actions.add_action(&zpos);

        // on/off settings: a true/false state give a check mark; a click toggles (-1)
        let toggles = [
            ("clickthrough", "ClickThrough", self.click_through.get()),
            ("draggable", "Draggable", self.draggable.get()),
            ("keeponscreen", "KeepOnScreen", self.keep_on_screen.get()),
            ("snapedges", "SnapEdges", self.snap_edges.get()),
        ];
        for (name, bang, on) in toggles {
            let action = gio::SimpleAction::new_stateful(name, None, &on.to_variant());
            let me = self.me.clone();
            action.connect_activate(move |_, _| {
                if let Some(sw) = me.upgrade() {
                    sw.run_action(&format!("!{bang} -1"));
                }
            });
            actions.add_action(&action);
        }

        //SavePosition has no bang, so its item calls the methode straight away
        let save = gio::SimpleAction::new_stateful("saveposition", None, &self.save_position.get().to_variant());
        let me = self.me.clone();
        save.connect_activate(move |_, _| {
            if let Some(sw) = me.upgrade() {
                sw.set_save_position(!sw.save_position.get());
            }
        });
        actions.add_action(&save);

        // Not supported yet: an action that exist but is disabled, so GTK grays its items
        let disabled = gio::SimpleAction::new("later", None);
        disabled.set_enabled(false);
        actions.add_action(&disabled);
        self.area.insert_action_group("set", Some(&actions)); // replaces the last menu's group

        let t = |key: &str| tr("SkinMenu", key);
        let later = "set.later";

        // Position ▸: the monitor (later), the five window layers, the X/Y options (later)
        let monitor = gio::Menu::new();
        monitor.append(Some(&t("DisplayMonitor")), Some(later));
        let layers = gio::Menu::new();
        for (key, n) in [("StayTopmost", 2), ("Topmost", 1), ("Normal", 0), ("Bottom", -1), ("OnDesktop", -2)] {
            let item = gio::MenuItem::new(Some(&t(key)), None);
            item.set_action_and_target_value(Some("set.zpos"), Some(&n.to_variant()));
            layers.append_item(&item);
        }
        let axes = gio::Menu::new();
        for key in ["FromRight", "FromBottom", "XasPercentage", "YasPercentage"] {
            axes.append(Some(&t(key)), Some(later));
        }
        let position = gio::Menu::new();
        position.append_section(None, &monitor);
        position.append_section(None, &layers);
        position.append_section(None, &axes);

        // The three groups of Rainmeter's with a line between them
        let size = gio::Menu::new();
        size.append_submenu(Some(&t("Position")), &position);
        size.append(Some(&t("Zoom")), Some(later));
        let look = gio::Menu::new();
        look.append(Some(&t("Transparncy")), Some(later));
        look.append(Some(&t("OnHover")), Some(later));
        let flags = gio::Menu::new();
        for (key, action) in [
            ("ClickThrough", "set.clickthrough"),
            ("draggable", "set.draggable"),
            ("KeepOnScreen", "set.keeponscreen"),
            ("SavePosition", "set.saveposition"),
            ("SnapToEdges", "set.snapedges"),
            ("Favorite", later),
        ] {
            flags.append(Some(&t(key)), Some(action));
        }
        let menu = gio::Menu::new();
        menu.append_section(None, &size);
        menu.append_section(None, &look);
        menu.append_section(None, &flags);
        menu
    }

    /*  The Variants submenu: every .ini in the skin's folder, with the running one checked.
        As in Rainmeter (ToggleSkin), picking another file loads it in place of this one, and
        picking the checked one unloads the skin. Made fresh each time, like settings_menu. */
    fn variants_menu(&self) -> gio::Menu {
        let (config, running, root) = {
            let s = self.skin.borrow();
            let file = s.file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            (s.config.clone(), file, s.skins_root.clone())
        }; // borrow ends here

        let menu = gio::Menu::new();
        for file in config::config_files(&root, &config) {
            let item = gio::MenuItem::new(Some(&menu_label(&file)), None);
            item.set_action_and_target_value(Some("var.pick"), Some(&file.to_variant()));
            menu.append_item(&item);
        }

        let actions = gio::SimpleActionGroup::new();
        actions.add_action(&self.config_action("pick", config, running));
        self.area.insert_action_group("var", Some(&actions));
        menu
    }

    /*  One radio action for the .ini files of one config folder. Each item sends its file
        name (its target) and GTK puts the dot on the item whose name equals the action's
        state: the running file, or "" when none runs. As in Rainmeter (ToggleSkin), picking
        the running file unloads the skin; picking another loads it in its place. */
    fn config_action(&self, name: &str, config: String, running: String) -> gio::SimpleAction {
        let action = gio::SimpleAction::new_stateful(name, Some(glib::VariantTy::STRING), &running.to_variant());
        let me = self.me.clone();
        action.connect_activate(move |_, target| {
            let Some(sw) = me.upgrade() else { return };
            let Some(file) = target.and_then(|t| t.get::<String>()) else { return };
            if config.contains('"') || file.contains('"') {
                // A bang can't quote a name with " in it (Windows forbids them in file names)
                sw.log(Level::Error, &format!("Can't load {config}\\{file}: the name has a \" in it"));
            } else if file == running {
                sw.run_action(&format!("!DeactivateConfig \"{config}\""));
            } else {
                sw.run_action(&format!("!activateConfig \"{config}\" \"{file}\""));
            }
        });
        action
    }

    // the running .ini file name, e.g. "Clock.ini"
    fn file_name(&self) -> String {
        let s = self.skin.borrow();
        s.file.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /*  The Skins list of the program menu: the whole skins folder as tree, as Rainmeter's
        CreateSkinsMenuRecursive. Each folder with .ini files gets its own radio action
        (c0, c1, ...) in a fresh "tree" group, so every running skin has its dot. The skin's
        root folder is also added to 'variants', below Variants ▸, as in Rainmeter. */
    fn skins_menu(&self, variants: &gio::Menu) -> gio::Menu {
        let root = self.skin.borrow().skins_root.clone();
        let current = config_key(&self.skin.borrow().config); // "illustro\clock": lowercase, \ only
        let top = current.split('\\').next().unwrap_or_default(); // its root folder: "illustro"
        let menu = gio::Menu::new();
        let actions = gio::SimpleActionGroup::new();
        if let Some(manager) = self.manager.upgrade() {
            for folder in config::skin_tree(&root) {
                if folder.name.to_lowercase() == top {
                    self.add_folder(variants, &folder, &folder.name, &manager, &actions);
                }
                self.add_folder(&menu, &folder, &folder.name, &manager, &actions);
            }
        }
        self.area.insert_action_group("tree", Some(&actions));
        menu
    }

    /*  adds one folder to 'parent' as a submenu: its .ini files first, then a line, then
        its subfolders, as in Rainmeter. 'config' is the folder's path below the skins
        folder ("illustro\Clock"), the config its files load as. it calls itself for each
        subfolder (recursion), so any depth works. */
    fn add_folder(&self, parent: &gio::Menu, folder: &config::SkinFolder, config: &str, manager: &Manager, actions: &gio::SimpleActionGroup) {
        let submenu = gio::Menu::new();
        if !folder.files.is_empty() {
            let name = format!("c{}", actions.list_actions().len()); // a new name per folder
            let running = manager.find(config).map(|sw| sw.file_name()).unwrap_or_default();
            let files = gio::Menu::new();
            for file in &folder.files {
                let item = gio::MenuItem::new(Some(&menu_label(file)), None);
                item.set_action_and_target_value(Some(&format!("tree.{name}")), Some(&file.to_variant()));
                files.append_item(&item);
            }
            submenu.append_section(None, &files);
            actions.add_action(&self.config_action(&name, config.to_string(), running));
        }
        if !folder.folders.is_empty() {
            let folders = gio::Menu::new();
            for sub in &folder.folders {
                self.add_folder(&folders, sub, &format!("{config}\\{}", sub.name), manager, actions);
            }
            submenu.append_section(None, &folders);
        }
        parent.append_submenu(Some(&menu_label(&folder.name)), &submenu);
    }

    /*  Runs an action: its bangs one after another, then any !Refresh, then (once GTK is free)
        the congif bangs, as Rainmeter does. Used by mouse actions and the [Rainmeter]
        actions (OnUpdateAction and others). */
    fn run_action(&self, text: &str) {
        let (Some(me), Some(manager)) = (self.me.upgrade(), self.manager.upgrade()) else { return };
        let source = self.skin.borrow().source.clone(); // target errors name the skin whose action it is
        // Like Rainmeter, refreshes wait until the rest of the action has run
        let mut to_refresh: Vec<Rc<SkinWindow>> = Vec::new();
        let mut config_bangs: Vec<ConfigBang> = Vec::new();
        for command in action::split_bangs(text) {
            let targets = match crate::log::with_source(&source, || action::target(command)) {
                Target::This => vec![me.clone()],
                Target::All => manager.all(),
                Target::Group(group) => manager.in_group(&group),
                Target::Nowhere => Vec::new(),
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
        /*  Loading and closing skins waits until GTK is done with this click, since a skin
            may close its own window. They run in the order the action gave them. */
        if !config_bangs.is_empty() {
            let this = self.skin.borrow().config.clone();
            /*  Keep the program alive until the bang have run: closiong the last windows
                would otherwise make GTK quit before a new skin can open */
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
        let text = {
            let s = self.skin.borrow();
            self.as_skin(|| s.expand_action(section, &raw))
        };
        busy.set(true);
        self.run_action(&text);
        busy.set(false);
    }

    /*  Runs one bang on this skin. Redraw and Update are done here; Refresh and the
        config bangs are handed back, for the caller to do when the action is finished. */
    fn run(&self, command: &str) -> After {
        // Borrowed for this bang only: !Update needs to borrow the skin itself
        let source = self.skin.borrow().source.clone(); // bang errors name the skin the bang runs on
        let after = crate::log::with_source(&source, || action::execute(&mut self.skin.borrow_mut(), command));
        match after {
            After::Redraw => {
                self.fit_window();
                self.area.queue_draw();
            }
            After::UpdateMeter(pick) => {
                let values = crate::measure::current_values(&self.skin.borrow().measures);
                if !self.update_meters(&values, &pick) && let Pick::Name(name) = &pick {
                    self.log(Level::Error, &format!("!UpdateMeter: [{name}] not found"));
                }
            }
            After::UpdateMeasure(pick) => {
                let mut values = crate::measure::current_values(&self.skin.borrow().measures);
                if !self.update_measures(&mut values, &pick) && let Pick::Name(name) = &pick {
                    self.log(Level::Error, &format!("!UpdateMeasure: [{name}] not found"));
                }
            }
            After::Menu(menu) => {
                // At the mouse when it's over this skin, else at the skin's top-left corner
                let (x, y) = self.pointer.get().unwrap_or((0.0, 0.0));
                match menu {
                    action::Menu::Skin => self.show_menu(x, y),
                    action::Menu::Custom => self.custom_menu(x, y),
                }
            }
            After::SetUpdate(ms) => {
                self.skin.borrow_mut().update_ms = ms;
                self.start_timer(); // as in Rainmeter: the next update comes a full interval later
            }
            After::Blur => self.apply_blur(),
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
            self.log(Level::Warning, &format!("{bang}: Skin \"{config}\" is not active"));
        } else {
            self.log(Level::Error, &format!("{bang}: Skin \"{config}\" does not exist"));
        }
    }
}

/*  the skin's custom items, as Rainmeter's AppendSkinCustomMenu: up to 3 go right in the menu
    under a grayed "Custom skin action" heading (separators dropped there); more go into a 
    submenu of that name, where a "---" title draws a line. GTK only draws lines between
    section, so the submenu is cut into one section per run of items. */
fn append_custom(model: &gio::Menu, items: &[ContextItem]) {
    let heading = tr("SkinMenu", "CustomSkinActions");
    if items.len() <= 3 {
        let section = gio::Menu::new();
        for item in items {
            if let ContextItem::Action { n, title } = item {
                section.append_item(&custom_item(*n, title));
            }
        }
        if section.n_items() > 0 {
            model.append_section(Some(&heading), &section); // a section's label is its grayed heading
        }
    } else {
        let submenu = custom_sections(items);
        let section = gio::Menu::new();
        section.append_submenu(Some(&heading), &submenu);
        model.append_section(None, &section);
    }
}

/*  The custom items as a menu of their own, where a "---" title draws a line. GTK only
    draws lines between section, so each run of items becomes one section. Used for the 
    "Custom skin actions" submenu and for !SkinCustomMenu. */
fn custom_sections(items: &[ContextItem]) -> gio::Menu {
    let menu = gio::Menu::new();
    let mut part = gio::Menu::new();
    for item in items {
        match item {
            ContextItem::Action { n, title } => part.append_item(&custom_item(*n, title)),
            ContextItem::Separator => {
                if part.n_items() > 0 {
                    menu.append_section(None, &part);
                    part = gio::Menu::new(); // the next run starts a new section
                }
            }
        }
    }
    if part.n_items() > 0 {
        menu.append_section(None, &part);
    }
    menu
}

/*  The program menu ("Snowmeter ▸"), as Rainmeter's ShowMenu (ContextMenu.cpp). Items
    that don't work yet name the disabled "app.later", so GTK greys them out. 'tree' is
    the Skins list, made by SkinWindow::skins_menu. */
fn program_menu(tree: gio::Menu) -> gio::Menu {
    let t = |key: &str| tr("SnowmeterMenu", key);
    let menu = gio::Menu::new();
    let windows = gio::Menu::new();
    windows.append(Some(&t("manage")), Some("app.later"));
    windows.append(Some(&tr("DebugDialog", "Debug")), Some("app.later"));
    windows.append(Some(&t("About")), Some("app.later"));
    windows.append(Some(&t("Help")), Some("app.later"));
    menu.append_section(None, &windows);

    let skins = gio::Menu::new();
    let top = gio::Menu::new();
    top.append(Some(&t("OpenFolder")), Some("app.openskinsfolder"));
    top.append(Some(&t("DisableDragging")), Some("app.later"));
    skins.append_section(None, &top);
    if tree.n_items() == 0 {
        tree.append(Some(&t("NoSkins")), Some("app.later"));
    }
    skins.append_section(None, &tree);
    let favorites = gio::Menu::new();
    favorites.append(Some(&t("NoFavorites")), Some("app.later"));
    let layouts = gio::Menu::new();
    layouts.append(Some(&t("NoLayouts")), Some("app.later"));
    let lists = gio::Menu::new();
    lists.append_submenu(Some(&t("Skins")), &skins);
    lists.append_submenu(Some(&t("Favorites")), &favorites);
    lists.append_submenu(Some(&t("Layouts")), &layouts);
    menu.append_section(None, &lists);

    let commands = gio::Menu::new();
    commands.append(Some(&t("EditSettings")), Some("app.editsettings"));
    commands.append(Some(&t("RefreshAll")), Some("app.refreshall"));
    menu.append_section(None, &commands);
    let exit = gio::Menu::new();
    exit.append(Some(&t("Exit")), Some("app.exit"));
    menu.append_section(None, &exit);
    menu
}

// One custom item: its title, and the "skin.context" action with its number as the target
fn custom_item(n: usize, title: &str) -> gio::MenuItem {
    let item = gio::MenuItem::new(Some(&menu_label(title)), None);
    item.set_action_and_target_value(Some("skin.context"), Some(&(n as i32).to_variant()));
    item
}

/*  GTK reads "_" in a menu label as a keyboard shortcut mark (_File = Alt+F) and hides it.
    "__" show a real "_", so names like My_Skin and titlres stay as written. */
fn menu_label(text: &str) -> String {
    text.replace('_', "__")
}