mod action;
mod calc;
mod clock;
mod config;
mod cpu;
mod dollarvar;
mod fonts;
mod formula;
mod ini;
mod manager;
mod measure;
mod memory;
mod meter;
mod monitors;
mod mouse;
mod paths;
mod position;
mod sectionvar;
mod skin;
mod textfile;
mod wlpos;
mod x11pos;

use gtk4::prelude::*;
use gtk4::{glib, Application};
use std::{
    cell::Cell,
    env,
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};

const USAGE: &str = "Usage: snowmeter [options] <skin> [<skin> ...]

  <skin>                   a path to an .ini file, or a name inside the skins
                           folder, such as Demo/Clock/Clock.ini
                           (case and \\ or / don't matter). Give several to
                           run them together; one skin per config folder.
                           With none, the skins that ran last time come back.

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

// Backend order: Wayland with layer-shell, else X11 through XWayland (where x11pos.rs can
// place windows and keep them above/below), else plain Wayland windows. GTK can't switch
// backend once started, so for X11 the program restarts itself with GDK_BACKEND=x11.
// GDK_BACKEND set by the user always wins (and also stops the restart from looping).
fn fall_back_to_xwayland() {
    if env::var_os("GDK_BACKEND").is_some() || env::var_os("DISPLAY").is_none() {
        return;
    }
    let Some(display) = gtk4::gdk::Display::default() else { return };
    // is_supported() prints a CRITICAL warning when asked on X11, so don't ask there
    if display.is::<gdk4_x11::X11Display>() || gtk4_layer_shell::is_supported() {
        return;
    }
    let exe = env::current_exe().unwrap_or_else(|e| fail(&format!("Can't find Snowmeter itself: {e}")));
    // exec() only returns if starting the new copy failed
    let err = Command::new(exe).args(env::args_os().skip(1)).env("GDK_BACKEND", "x11").exec();
    eprintln!("Can't restart with X11 ({err}); skins will be plain Wayland windows");
}

// For the startup message
fn backend_name() -> &'static str {
    match gtk4::gdk::Display::default() {
        Some(d) if d.is::<gdk4_x11::X11Display>() => "X11",
        Some(_) if gtk4_layer_shell::is_supported() => "Wayland (layer-shell)",
        Some(_) => "Wayland (plain windows)",
        None => "none",
    }
}

fn main() -> glib::ExitCode {
    let mut skins_dir_arg: Option<String> = None;
    let mut set_dir: Option<String> = None;
    let mut list_only = false;
    let mut show_help = false;
    let mut skin_args: Vec<String> = Vec::new();

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
            _ => skin_args.push(a),
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

    // The skins to run: the ones named on the command line, else the ones that were
    // running last time (Active= in the settings file)
    let files: Vec<(String, Result<PathBuf, String>)> = if skin_args.is_empty() {
        config::active_skins(&cfg, &root)
            .into_iter()
            .map(|path| (path.display().to_string(), Ok(path)))
            .collect()
    } else {
        skin_args.iter().map(|arg| (arg.clone(), config::resolve_skin(arg, &root))).collect()
    };

    // Nothing to run (or --list / --help): show what's available and stop
    if files.is_empty() || list_only || show_help {
        if !list_only {
            println!("{}", USAGE);
        }
        print_skins(&root, source);
        std::process::exit(0);
    }

    // Start GTK now, so loading the skins can read the monitors (#SCREENAREAWIDTH# and others)
    gtk4::init().unwrap_or_else(|e| fail(&format!("Can't start GTK: {e}")));
    fall_back_to_xwayland();
    println!("Backend: {}", backend_name());
    // A skin with an error is reported and skipped, as Rainmeter does; the others still run
    let mut skins = Vec::new();
    for (name, file) in files {
        match file.and_then(|file| manager::load_skin(&file, &root, &cfg)) {
            Ok(skin) => skins.push(skin),
            Err(e) => eprintln!("{name}: {e}"),
        }
    }
    if skins.is_empty() {
        fail("No skin could be loaded");
    }

    let app = Application::builder()
        .application_id("org.example.snowmeter")
        .build();
    let manager = manager::Manager::new(&app, &root, &cfg);
    // activate runs when the app starts. GTK wants a callback that may run more than
    // once, so the skins sit in a Cell and take() hands them over the first time only
    // (later calls get an empty list).
    let first_skins = Cell::new(skins);
    app.connect_activate(move |_| {
        for skin in first_skins.take() {
            manager.add(skin);
        }
    });
    app.run_with_args::<&str>(&[])
}
