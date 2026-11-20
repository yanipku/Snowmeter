use crate::log::{log, Level};

/*  The program's own text (menus now, dialogs later), looked up by the section and key
    names of Rainmeter's language files, e.g. [SkinMenu] RefreshSkin=Refresh skin.
    English is build in, word for word from Rainmer's en.ini A language file chosen in the settings will override it later (manage window, settings tab). */
const ENGLISH: &[(&str, &str, &str)] = &[
    ("SnowmeterMenu", "Manage", "Manage"),
    ("SnowmeterMenu", "About", "About"),
    ("SnowmeterMenu", "Help", "Help"),
    ("SnowmeterMenu", "Skins", "Skins"),
    ("SnowmeterMenu", "NoSkins", "No skins available"),
    ("SnowmeterMenu", "OpenFolder", "Open folder"),
    ("SnowmeterMenu", "DisableDragging", "Disable dragging"),
    ("SnowmeterMenu", "Favorites", "Favorites"),
    ("SnowmeterMenu", "NoFavorites", "No favorites defined"),
    ("SnowmeterMenu", "Layouts", "Layouts"),
    ("SnowmeterMenu", "NoLayouts", "No layouts available"),
    ("SnowmeterMenu", "EditSettings", "Edit settings"),
    ("SnowmeterMenu", "RefreshAll", "Refresh all"),
    ("SnowmeterMenu", "Exit", "Exit"),
    ("SkinMenu", "RefreshSkin", "Refresh skin"),
    ("SkinMenu", "UnloadSkin", "Unload skin"),
    ("SkinMenu", "CustomSkinActions", "Custom skin actions"),
    ("SkinMenu", "Variants", "Variants"),
    ("SkinMenu", "EditSkin", "Edit skin"),
    ("SkinMenu", "Settings", "Settings"),
    ("SkinMenu", "Position", "Position"),
    ("SkinMenu", "DisplayMonitor", "Display monitor"),
    ("SkinMenu", "StayTopmost", "Stay topmost"),
    ("SkinMenu", "Topmost", "Topmost"),
    ("SkinMenu", "Normal", "Normal"),
    ("SkinMenu", "Bottom", "Bottom"),
    ("SkinMenu", "OnDesktop", "On desktop"),
    ("SkinMenu", "FromRight", "From right"),
    ("SkinMenu", "FromBottom", "From bottom"),
    ("SkinMenu", "XAsPercentage", "X as percentage"),
    ("SkinMenu", "YAsPercentage", "Y as percentage"),
    ("SkinMenu", "Zoom", "Zoom"),
    ("SkinMenu", "Transparency", "Transparency"),
    ("SkinMenu", "OnHover", "On hover"),
    ("SkinMenu", "ClickThrough", "Click through"),
    ("SkinMenu", "Draggable", "Draggable"),
    ("SkinMenu", "KeepOnScreen", "Keep on screen"),
    ("SkinMenu", "SavePosition", "Save position"),
    ("SkinMenu", "SnapToEdges", "Snap to edges"),
    ("SkinMenu", "Favorite", "Favorite"),
    ("DebugDialog", "Debug", "Debug")
];

/*  The text for [section] key, in any case as in an .ini file. For now always English.
    A missing key give the key itself, so the menu still shows something, and a Debug
    line names it. A String, because text from a language file is made at run time. */
pub fn tr(section: &str, key: &str) -> String {
    let found = ENGLISH
        .iter()
        .find(|(s, k, _)| s.eq_ignore_ascii_case(section) && k.eq_ignore_ascii_case(key));
    match found {
        Some((_, _, text)) => text.to_string(),
        None => {
            log(Level::Debug, "", &format!("No text for [{section}] {key}"));
            key.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_is_found_by_section_and_key() {
        assert_eq!(tr("SkinMenu", "RefreshSkin"), "Refresh skin");
        assert_eq!(tr("skinmenu", "REFRESHSKIN"), "Refresh skin", "any case, as in an .ini file");
        assert_eq!(tr("SkinMenu", "NoSuchKey"), "NoSuchKey", "a missing key shows itself");
        assert_eq!(tr("RainmeterMenu", "RefreshSkin"), "RefreshSkin", "the section counts too");
    }
}