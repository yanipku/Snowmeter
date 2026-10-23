// Action strings, such as LeftMouseUpAction=[!ShowMeter A][!Redraw], and the bangs in them.
// Follows Rainmeter's CommandHandler::ExecuteCommand (Library/CommandHandler.cpp).

use crate::formula;
use crate::skin::Skin;

// What the window has to do after a bang, since bangs can't reach the window themselves
pub enum After {
    Nothing,
    Redraw, // !Redraw: paint the skin as it is now
    Update, // !Update: a full update straight away, then paint
    Refresh, // !Refresh: read the skin file again from scratch
    Config(ConfigBang), // a bang that loads or unloads skins: only the manager can do it
}

// !ActivateConfig, !DeactivateConfig, !ToggleConfig
#[derive(Debug, PartialEq)]
pub enum ConfigBang {
    Activate { config: String, file: Option<String> },
    Deactivate { config: Option<String> }, // None: this skin; "*": every skin
    Toggle { config: String, file: Option<String> },
}

// Which skin a bang runs on
#[derive(Debug, PartialEq)]
pub enum Target {
    This,           // the skin whose action it is
    Config(String), // the skin of another config, like illustro\Clock
    All,            // every running skin
}

// How many arguments each bang takes. One more is the config to run it on, and
// "*" or "" means all skins (Rainmeter's DoBang). None: a bang we don't know yet.
fn arg_count(bang: &str) -> Option<usize> {
    match bang {
        "redraw" | "update" | "refresh" => Some(0),
        "showmeter" | "hidemeter" | "togglemeter" => Some(1),
        "setvariable" => Some(2),
        _ => None,
    }
}

pub fn target(cmd: &str) -> Target {
    let Some((name, args)) = parse_bang(cmd) else { return Target::This };
    let Some(count) = arg_count(&name.to_ascii_lowercase()) else { return Target::This };
    match args.get(count).map(String::as_str) {
        None => Target::This,
        Some("" | "*") => Target::All,
        Some(config) => Target::Config(config.to_string()),
    }
}

// Runs one command from an action. Only a few bangs are supported so far;
// the others, and programs, are reported and skipped.
pub fn execute(skin: &mut Skin, cmd: &str) -> After {
    let Some((name, args)) = parse_bang(cmd) else {
        println!("Running programs isn't supported yet: {cmd}");
        return After::Nothing;
    };
    // Bang names ignore case: !hidemeter works as well as !HideMeter
    let lower = name.to_ascii_lowercase();
    match lower.as_str() {
        "showmeter" | "hidemeter" | "togglemeter" => {
            let Some(meter_name) = args.first() else {
                eprintln!("!{name}: needs the name of a meter");
                return After::Nothing;
            };
            let Some(meter) = skin.meter_mut(meter_name) else {
                eprintln!("!{name}: no meter named \"{meter_name}\"");
                return After::Nothing;
            };
            let hide = match lower.as_str() {
                "showmeter" => false,
                "hidemeter" => true,
                _ => !meter.is_hidden(), // toggle: the opposite of now
            };
            meter.set_hidden(hide);
            After::Nothing // like Rainmeter: seen at the next update or !Redraw
        }
        "setvariable" => {
            let [var, value, ..] = args.as_slice() else {
                eprintln!("!{name}: needs a variable name and a value");
                return After::Nothing;
            };
            // Variable names ignore case: the table keys are lowercase
            let var = var.to_lowercase();
            let value = &variable_value(value);
            for meter in &mut skin.meters {
                meter.set_variable(&var, value);
            }
            skin.vars.insert(var, value.clone());
            After::Nothing // like Rainmeter: seen at the next update
        }
        "redraw" => After::Redraw,
        "update" => After::Update,
        "refresh" => After::Refresh,
        "activateconfig" | "toggleconfig" => {
            let Some(config) = args.first().cloned() else {
                eprintln!("!{name}: Invalid parameters");
                return After::Nothing;
            };
            let file = args.get(1).cloned();
            if lower == "activateconfig" {
                After::Config(ConfigBang::Activate { config, file })
            } else {
                After::Config(ConfigBang::Toggle { config, file })
            }
        }
        "deactivateconfig" => {
            After::Config(ConfigBang::Deactivate { config: args.first().cloned() })
        }
        _ => {
            println!("!{name} isn't supported yet");
            After::Nothing
        }
    }
}

// Splits an action into the commands it runs, in order: "[!A][!B]" gives "!A" and "!B".
// Brackets inside a bang, such as [MeasureCPU] or [#Var], don't end it, and """text"""
// can hold anything, brackets included. Text between the bracketed bangs is ignored.
// An action that doesn't start with [ is a single command: a bang ("!Refresh") or a program.
//
// The returned commands are slices of `action`, so nothing is copied.
pub fn split_bangs(action: &str) -> Vec<&str> {
    let action = action.trim_start();
    if !action.starts_with('[') {
        return if action.is_empty() { Vec::new() } else { vec![action] };
    }

    let bytes = action.as_bytes();
    let mut out = Vec::new();
    // Signed on purpose: as in Rainmeter, a stray ] makes it negative, and the
    // bangs after it never run. Skins written for Rainmeter rely on the same result.
    let mut depth: i32 = 0;
    let mut start: Option<usize> = None;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'[' => {
                if depth == 0 {
                    start = Some(i + 1);
                }
                depth += 1;
            }
            b']' => {
                depth -= 1;
                if let (0, Some(s)) = (depth, start) {
                    let cmd = action[s..i].trim_start();
                    if !cmd.is_empty() {
                        out.push(cmd);
                    }
                }
            }
            b'"' if action[i..].starts_with(r#"""""#) => {
                // Jump past the closing """ (if there is none, only the opening one)
                i += 3;
                if let Some(p) = action[i..].find(r#"""""#) {
                    i += p + 3;
                }
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out
}

const TRIPLE: &str = r#"""""#;

// Splits one bang into its name and arguments: !SetVariable Color "255, 0, 0" gives
// ("SetVariable", ["Color", "255, 0, 0"]). None if the command isn't a bang.
// Section variables in the arguments ([Measure], [#Var]) are left for the bang to expand.
pub fn parse_bang(cmd: &str) -> Option<(&str, Vec<String>)> {
    let body = cmd.trim_start().strip_prefix('!')?;
    // Old skins write !RainmeterShowMeter for !ShowMeter
    let body = match body.get(..9) {
        Some(p) if p.eq_ignore_ascii_case("rainmeter") => &body[9..],
        _ => body,
    };
    Some(match body.split_once(' ') {
        Some((name, rest)) => (name, parse_args(rest)),
        None => (body, Vec::new()),
    })
}

// The arguments of a bang, as Rainmeter's CommandHandler::ParseString reads them:
//   word      up to the next space; any " inside is removed
//   "a b"     up to the next "
//   """a"b""" up to the next """ followed by a space (or else the last """); quotes stay
// An unclosed quote takes the rest of the text.
fn parse_args(text: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut rest = text;
    while let Some(p) = rest.find(|c| c != ' ') {
        rest = &rest[p..];
        // (the argument, whether its quotes are removed, the text after it)
        let (arg, strip, after) = if let Some(r) = rest.strip_prefix(TRIPLE) {
            match r.find(r#"""" "#) {
                Some(e) => (&r[..e], false, &r[e + 4..]),
                None => match r.rfind(TRIPLE) {
                    Some(e) => (&r[..e], false, &r[e + 3..]),
                    None => (r, false, ""),
                },
            }
        } else if let Some(r) = rest.strip_prefix('"') {
            match r.find('"') {
                Some(e) => (&r[..e], true, &r[e + 1..]),
                None => (r, true, ""),
            }
        } else {
            match rest.find(' ') {
                Some(e) => (&rest[..e], true, &rest[e + 1..]),
                None => (rest, true, ""),
            }
        };
        args.push(if strip { arg.replace('"', "") } else { arg.to_string() });
        rest = after;
    }
    // Rainmeter quirk: text made only of spaces becomes one argument
    if args.is_empty() && !rest.is_empty() {
        args.push(rest.to_string());
    }
    args
}

// The value !SetVariable stores. Like Rainmeter's Skin::SetVariable, a value in
// parentheses is a formula: its result is written with 5 decimals, trailing zeros
// removed ("3.50000" -> "3.5", "2.00000" -> "2"). Anything else, or a formula with
// an error, is stored as written.
fn variable_value(value: &str) -> String {
    if !(value.starts_with('(') && value.ends_with(')')) {
        return value.to_string();
    }
    match formula::parse_number(value) {
        Ok(n) => {
            let text = format!("{n:.5}");
            let text = text.trim_end_matches('0').trim_end_matches('.');
            if text == "-0" { "0".to_string() } else { text.to_string() }
        }
        Err(e) => {
            eprintln!("!SetVariable: formula {value}: {e}");
            value.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_bangs() {
        let bang = |cmd: &str| parse_bang(cmd).map(|(n, a)| (n.to_ascii_lowercase(), a));
        // Parsed the way execute() reads them
        assert_eq!(
            bang(r#"!ActivateConfig "illustro\Clock" "Clock.ini""#),
            Some(("activateconfig".to_string(), vec![r"illustro\Clock".to_string(), "Clock.ini".to_string()]))
        );
        assert_eq!(target(r#"!DeactivateConfig "illustro\Clock""#), Target::This);
    }

    #[test]
    fn bang_targets() {
        let config = |c: &str| Target::Config(c.to_string());
        assert_eq!(target("!Refresh"), Target::This);
        assert_eq!(target(r#"!Refresh "illustro\Clock""#), config(r"illustro\Clock"));
        assert_eq!(target("!Refresh *"), Target::All);
        assert_eq!(target(r#"!Refresh """#), Target::All);
        assert_eq!(target("!ShowMeter Graph"), Target::This);
        assert_eq!(target("!ShowMeter Graph Other"), config("Other"));
        assert_eq!(target("!SetVariable N 1"), Target::This);
        assert_eq!(target("!SetVariable N 1 *"), Target::All);
        assert_eq!(target("!RainmeterHideMeter Graph Other"), config("Other"));
        // Bangs we don't know yet, and programs, stay on this skin
        assert_eq!(target("!Log hi Other"), Target::This);
        assert_eq!(target("notepad.exe"), Target::This);
    }

    #[test]
    fn set_variable_values() {
        assert_eq!(variable_value("(1+2)"), "3");
        assert_eq!(variable_value("(7/2)"), "3.5");
        assert_eq!(variable_value("(1/3)"), "0.33333");
        assert_eq!(variable_value("Hello"), "Hello");
        assert_eq!(variable_value("(1+"), "(1+"); // not a formula: no closing )
    }

    #[test]
    fn bangs_are_split() {
        assert_eq!(split_bangs("[!ShowMeter A][!Redraw]"), ["!ShowMeter A", "!Redraw"]);
        // Leading spaces are dropped, text between bangs is ignored
        assert_eq!(split_bangs("  [ !A] junk [!B]"), ["!A", "!B"]);
    }

    #[test]
    fn nested_brackets_stay_in_their_bang() {
        assert_eq!(
            split_bangs("[!SetOption Text Text [MeasureCPU]%][!Redraw]"),
            ["!SetOption Text Text [MeasureCPU]%", "!Redraw"]
        );
    }

    #[test]
    fn triple_quotes_hide_brackets() {
        assert_eq!(
            split_bangs(r#"[!Log """a]b[c"""][!Redraw]"#),
            [r#"!Log """a]b[c""""#, "!Redraw"]
        );
    }

    #[test]
    fn single_commands_and_mistakes() {
        assert_eq!(split_bangs("!Refresh"), ["!Refresh"]);
        assert_eq!(split_bangs(r#""https://rainmeter.net""#), [r#""https://rainmeter.net""#]);
        assert!(split_bangs("   ").is_empty());
        assert!(split_bangs("[!A").is_empty(), "an unclosed bang never runs");
        assert!(split_bangs("[]").is_empty());
        // A stray ] stops everything after it, as in Rainmeter
        assert_eq!(split_bangs("[!A]][!B]"), ["!A"]);
    }

    #[test]
    fn bang_names_and_arguments() {
        let bang = |s| parse_bang(s).expect("should be a bang");
        assert_eq!(bang(r#"!SetVariable Color "255, 0, 0""#), ("SetVariable", vec!["Color".into(), "255, 0, 0".into()]));
        assert_eq!(bang("  !RainmeterShowMeter A"), ("ShowMeter", vec!["A".into()]));
        assert_eq!(bang("!Redraw"), ("Redraw", vec![]));
        assert_eq!(parse_bang(r#""https://rainmeter.net""#), None);
    }

    #[test]
    fn arguments_follow_rainmeter_quoting() {
        assert_eq!(parse_args(r#"a"b  "c d"e"#), ["ab", "c d", "e"]);
        assert_eq!(parse_args(r#""""say "hi" now""" next"#), [r#"say "hi" now"#, "next"]);
        assert_eq!(parse_args(r#"x """[a]""""#), ["x", "[a]"]);
        assert_eq!(parse_args(r#"Text "unclosed text"#), ["Text", "unclosed text"]);
        assert_eq!(parse_args("  "), ["  "]);
        assert!(parse_args("").is_empty());
    }
}
