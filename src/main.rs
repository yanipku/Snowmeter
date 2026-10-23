mod action;
mod clock;
mod config;
mod cpu;
mod fonts;
mod ini;
mod measure;
mod memory;
mod monitors;
mod manager;
mod meter;
mod mouse;
mod paths;
mod skin;
mod textfile;
mod calc;
mod formula;
mod sectionvar;

use gtk4::prelude::*;
use gtk4::{glib, Application};
use std::{cell::Cell, env, path::Path};

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
    // Start GTK now, so loading the skin can read the monitors (#SCREENAREAWIDTH# and others)
    gtk4::init().unwrap_or_else(|e| fail(&format!("Can't start GTK: {e}")));
    let skin = skin::load(&skin_file, &root, &cfg.dir).unwrap_or_else(|e| fail(&e));

    let app = Application::builder()
        .application_id("org.example.snowmeter")
        .build();
    let manager = manager::Manager::new(&app, &root, &cfg.dir);
    // activate runs when the app starts. GTK wants a callback that may run more than
    // once, so the skin sits in a Cell and take() hands it over the first time only.
    let first_skin = Cell::new(Some(skin));
    app.connect_activate(move |_| {
        if let Some(skin) = first_skin.take() {
            manager.add(skin);
        }
    });
    app.run_with_args::<&str>(&[])
}
