/*  The log: messages from skins (!Log), and later from Snowmeter itself.
    for now it prints to the terminal in Rainmeter's log-file format:
        NOTE (21:05:03.120) illustro\Clock\Clock.ini: text
    The Manage window's Log tab will show the same lines once it exists. */

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Level {
    Error,
    Warning,
    Notice,
    Debug,
}

impl Level {
    // The type argument of !Log, in any case. None: not one of the four.
    pub fn parse(name: &str) -> Option<Level> {
        match name.to_ascii_lowercase().as_str() {
            "error" => Some(Level::Error),
            "warning" => Some(Level::Warning),
            "notice" => Some(Level::Notice),
            "debug" => Some(Level::Debug),
            _ => None,
        }
    }

    // the 4-letter tag Rainmeter writes in its log file
    fn tag(self) -> &'static str {
        match self {
            Level::Error => "ERRO",
            Level::Warning => "WARN",
            Level::Notice => "NOTE",
            Level::Debug => "DBUG",
        }
    }
}

pub fn log(level: Level, source: &str, message: &str) {
    let time = chrono::Local::now().format("%H:%M:%S%.3f");
    eprintln!("{} ({time}) {}: {}", level.tag(), safe(source), safe(message));
}

/*  Control characters become '?', so a skin can't send escape code to the terminal
    (they can move the cursor, change colours or even retitle the window) */
fn safe(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { '?' } else { c }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_and_safe_text() {
        assert_eq!(Level::parse("WARNING"), Some(Level::Warning));
        assert_eq!(Level::parse("debug"), Some(Level::Debug));
        assert_eq!(Level::parse("Info"), None);
        assert_eq!(safe("a\x1b[31mb\n"), "a?[31mb?");
    }
}