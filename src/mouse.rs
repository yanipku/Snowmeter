// Mouse actions such as LeftMouseUpAction, on meters and on [Rainmeter].
// Follows Rainmeter's Library/Mouse.cpp.

use crate::ini::{self, Section};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseEvent {
    LeftUp,
    LeftDown,
    LeftDoubleClick,
    RightUp,
    RightDown,
    RightDoubleClick,
    MiddleUp,
    MiddleDown,
    MiddleDoubleClick,
}

impl MouseEvent {
    pub const ALL: [MouseEvent; 9] = [
        MouseEvent::LeftUp,
        MouseEvent::LeftDown,
        MouseEvent::LeftDoubleClick,
        MouseEvent::RightUp,
        MouseEvent::RightDown,
        MouseEvent::RightDoubleClick,
        MouseEvent::MiddleUp,
        MouseEvent::MiddleDown,
        MouseEvent::MiddleDoubleClick,
    ];

    // The event for a GTK button number (1 left, 2 middle, 3 right) and kind of press.
    // None for other buttons, such as the side buttons.
    pub fn from_button(button: u32, press: Press) -> Option<MouseEvent> {
        use MouseEvent::*;
        Some(match (button, press) {
            (1, Press::Down) => LeftDown,
            (1, Press::Up) => LeftUp,
            (1, Press::DoubleClick) => LeftDoubleClick,
            (2, Press::Down) => MiddleDown,
            (2, Press::Up) => MiddleUp,
            (2, Press::DoubleClick) => MiddleDoubleClick,
            (3, Press::Down) => RightDown,
            (3, Press::Up) => RightUp,
            (3, Press::DoubleClick) => RightDoubleClick,
            _ => return None,
        })
    }

    // The skin option for this event, in lowercase like every option key
    pub fn option(self) -> &'static str {
        match self {
            MouseEvent::LeftUp => "leftmouseupaction",
            MouseEvent::LeftDown => "leftmousedownaction",
            MouseEvent::LeftDoubleClick => "leftmousedoubleclickaction",
            MouseEvent::RightUp => "rightmouseupaction",
            MouseEvent::RightDown => "rightmousedownaction",
            MouseEvent::RightDoubleClick => "rightmousedoubleclickaction",
            MouseEvent::MiddleUp => "middlemouseupaction",
            MouseEvent::MiddleDown => "middlemousedownaction",
            MouseEvent::MiddleDoubleClick => "middlemousedoubleclickaction",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Press {
    Down,
    Up,
    DoubleClick,
}

// The mouse actions of one section: a meter, or [Rainmeter] for the whole skin
#[derive(Default)]
pub struct MouseActions {
    actions: Vec<(MouseEvent, String)>, // only the events that have an action
}

impl MouseActions {
    // #Variables# are replaced now (at load, or on every update with DynamicVariables=1),
    // but section variables such as [MeasureCPU] and [#Var] are kept as text and filled in
    // when the action runs, so a click sees the values of that moment (Skin::expand_action).
    // So `section` must hold the options before section variables are expanded.
    // This is Rainmeter's ReadActionString: sectionVariables = false.
    pub fn read(section: &Section, vars: &HashMap<String, String>) -> MouseActions {
        let actions = MouseEvent::ALL
            .iter()
            .filter_map(|&event| {
                let raw = section.options.get(event.option())?;
                let text = ini::resolve(raw, vars);
                // An empty action is no action: the click goes to whatever is below
                (!text.trim().is_empty()).then_some((event, text))
            })
            .collect();
        MouseActions { actions }
    }

    pub fn get(&self, event: MouseEvent) -> Option<&str> {
        self.actions.iter().find(|(e, _)| *e == event).map(|(_, a)| a.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_are_read() {
        let mut options = HashMap::new();
        options.insert("leftmouseupaction".to_string(), "[!SetVariable C #Red#][!Log [MeasureCPU]][!Log [#Red]]".to_string());
        options.insert("rightmouseupaction".to_string(), "  ".to_string());
        let mut vars = HashMap::new();
        vars.insert("red".to_string(), "255,0,0".to_string());

        let mouse = MouseActions::read(&Section { name: "M".to_string(), options }, &vars);
        // #Var# is replaced, section variables ([Measure], [#Var]) are kept for the click
        assert_eq!(
            mouse.get(MouseEvent::LeftUp),
            Some("[!SetVariable C 255,0,0][!Log [MeasureCPU]][!Log [#Red]]")
        );
        assert_eq!(mouse.get(MouseEvent::RightUp), None, "an empty action is no action");
        assert_eq!(mouse.get(MouseEvent::MiddleUp), None);
    }

    #[test]
    fn gtk_buttons_become_events() {
        assert_eq!(MouseEvent::from_button(1, Press::Up), Some(MouseEvent::LeftUp));
        assert_eq!(MouseEvent::from_button(3, Press::DoubleClick), Some(MouseEvent::RightDoubleClick));
        assert_eq!(MouseEvent::from_button(8, Press::Down), None, "side buttons aren't supported yet");
    }
}
