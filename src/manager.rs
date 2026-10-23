// The running skins: one window per config (a folder under Skins, like illustro\Clock),
// as in Rainmeter, where at most one skin of each config runs at a time.
use crate::action::{self, After, ConfigBang, Target};
use crate::config;
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
    skins_root: PathBuf,   // the skins folder
    settings_dir: PathBuf, // for #SETTINGSPATH# in skins loaded later
    // Keys are lowercase config names. The map holds the only strong reference to each
    // window's state; the window's own callbacks hold weak ones (see SkinWindow::open).
    skins: RefCell<HashMap<String, Rc<SkinWindow>>>,
}

// Config names ignore case, and \ or / both work
fn config_key(config: &str) -> String {
    config.trim().replace('/', "\\").to_lowercase()
}

impl Manager {
    pub fn new(app: &Application, skins_root: &Path, settings_dir: &Path) -> Rc<Manager> {
        Rc::new(Manager {
            app: app.clone(),
            skins_root: skins_root.to_path_buf(),
            settings_dir: settings_dir.to_path_buf(),
            skins: RefCell::new(HashMap::new()),
        })
    }

    // Shows an already loaded skin in a new window, closing any other skin of its config.
    // `self: &Rc<Self>` lets the method hand the window a pointer back to the manager.
    pub fn add(self: &Rc<Self>, skin: Skin) {
        let key = config_key(&skin.config);
        let window = SkinWindow::open(&self.app, skin, Rc::downgrade(self));
        // The new window opens before the old one closes, so GTK never sees zero windows
        let old = self.skins.borrow_mut().insert(key, window);
        if let Some(old) = old {
            old.close();
        }
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
        let skin = skin::load(&path, &self.skins_root, &self.settings_dir)?;
        self.add(skin);
        Ok(())
    }

    // The first .ini directly inside a config folder, by name
    fn first_file(&self, config: &str) -> Option<String> {
        let wanted = config_key(config);
        config::list_skins(&self.skins_root).into_iter().find_map(|rel| {
            let rel = rel.replace('/', "\\");
            let (folder, file) = rel.rsplit_once('\\')?;
            (folder.to_lowercase() == wanted).then(|| file.to_string())
        })
    }

    // Closes a config's skin, if it runs. Dropping the last Rc frees everything; when no
    // window is left, GTK ends the program (later the tray icon will keep it running).
    fn deactivate(&self, config: &str) {
        let removed = self.skins.borrow_mut().remove(&config_key(config));
        if let Some(sw) = removed {
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
}

impl SkinWindow {
    // Rc::new_cyclic hands us a Weak pointer to the SkinWindow before it exists, so the
    // callbacks can be built with it. They hold Weak, not Rc: the window owns its callbacks,
    // so a strong pointer back would be a loop that is never freed.
    fn open(app: &Application, skin: Skin, manager: Weak<Manager>) -> Rc<SkinWindow> {
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
            area.set_draw_func(move |_, cr, _w, _h| {
                let Some(sw) = draw_me.upgrade() else { return };
                let s = sw.skin.borrow();
                if let Some(bg) = &s.background {
                    meter::set_color(cr, bg);
                    let _ = cr.paint();
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
            }
        });
        sw.start_timer();

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
            // A few pixels of wobble is still a click, not a drag
            if dx.hypot(dy) < 4.0 {
                return;
            }
            let (Some(window), Some(sw)) = (weak_window.upgrade(), drag_me.upgrade()) else { return };
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

        window.present();
        sw
    }

    // Stops the timer and closes the window
    fn close(&self) {
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
        let new_size = {
            let mut guard = self.skin.borrow_mut();
            let s = &mut *guard;
            let values = crate::measure::update_all(&mut s.measures);
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
            self.area.set_content_width(w);
            self.area.set_content_height(h);
        }
        self.area.queue_draw();
    }

    // !Refresh: reads the skin file again and puts the new skin in place of the old one.
    // If the file now has an error, the old skin keeps running.
    // The timer restarts, since Update= may have changed.
    fn refresh(&self) {
        let (file, root, settings) = {
            let s = self.skin.borrow();
            (s.file.clone(), s.skins_root.clone(), s.settings_dir.clone())
        };
        let fresh = match skin::load(&file, &root, &settings) {
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
    }

    // Runs the action of the first of `events` that anything at (x, y) has an action for
    fn mouse_action(&self, x: f64, y: f64, events: &[MouseEvent]) {
        // Copy the action text out, so the read borrow ends before the bangs change the skin
        let text = {
            let s = self.skin.borrow();
            let found = events
                .iter()
                .find_map(|&e| s.action_at(x, y, e).map(|(name, a)| (e, s.expand_action(name, a))));
            let Some((event, text)) = found else { return };
            println!("{event:?} at ({x:.0}, {y:.0}): {text}");
            text
        }; // `s` is dropped here: the skin is no longer borrowed
        let (Some(me), Some(manager)) = (self.me.upgrade(), self.manager.upgrade()) else { return };
        // Like Rainmeter, refreshes wait until the rest of the action has run
        let mut to_refresh: Vec<Rc<SkinWindow>> = Vec::new();
        let mut config_bangs: Vec<ConfigBang> = Vec::new();
        for command in action::split_bangs(&text) {
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
            glib::idle_add_local_once(move || {
                for bang in config_bangs {
                    manager.run_config_bang(bang, &this);
                }
            });
        }
    }

    // Runs one bang on this skin. Redraw and Update are done here; Refresh and the
    // config bangs are handed back, for the caller to do when the action is finished.
    fn run(&self, command: &str) -> After {
        // Borrowed for this bang only: !Update needs to borrow the skin itself
        let after = action::execute(&mut self.skin.borrow_mut(), command);
        match after {
            After::Redraw => self.area.queue_draw(),
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
