use std::ffi::{CString, c_char, c_int, c_void};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use crate::paths;

// Fontconfig is the C library Cairo uses to turn a font name into a file.
// GTK already links it, so we only declare the one function we need.
#[link(name = "fontconfig")]
unsafe extern "C" {
    // Adds a font file for this program only (nothing is installed).
    // A null config means the default one, which Cairo uses.
    fn FcConfigAppFontAddFile(config: *mut c_void, file: *const c_char) -> c_int;
}

// Makes one font file usable by FontFace=. Returns false if Fontconfig rejects it.
pub fn add_font_file(path: &Path) -> bool {
    // A path with a NUL byte inside can't be passed to C
    let Ok(file) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // Safety: `file` is a valid NUL-terminated string that outlives the call,
    // and Fontconfig copies what it needs.
    unsafe { FcConfigAppFontAddFile(std::ptr::null_mut(), file.as_ptr()) != 0 }
}

// Loads a skin's fonts like Rainmeter: every file in @Resources/Fonts/,
// then LocalFont, LocalFont2, ... until one is missing.
// `get` reads an option from [Rainmeter] with variables already filled in.
pub fn load_skin_fonts(resources: &Path, base_dir: &Path, get: impl Fn(&str) -> Option<String>) {
    // The Fonts folder, matched ignoring case like Windows does
    let fonts_dir = fs::read_dir(resources).ok().and_then(|mut entries| {
        entries
            .find_map(|e| e.ok().filter(|e| e.file_name().eq_ignore_ascii_case("Fonts")))
            .map(|e| e.path())
    });
    if let Some(entries) = fonts_dir.and_then(|d| fs::read_dir(d).ok()) {
        for path in entries.filter_map(|e| e.ok()).map(|e| e.path()) {
            if path.is_file() && !add_font_file(&path) {
                eprintln!("Unable to load font: {}", path.display());
            }
        }
    }

    let mut i = 1;
    while let Some(name) = get(&if i == 1 { "localfont".to_string() } else { format!("localfont{i}") })
        .filter(|v| !v.trim().is_empty())
    {
        let loaded = paths::find_file(base_dir, &name).is_some_and(|p| add_font_file(&p));
        if !loaded {
            eprintln!("Unable to load font: {name}");
        }
        i += 1;
    }
}
