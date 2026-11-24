# Roadmap

This list compares our engine with Rainmeter. The reference is the official source in `../rainmeter_code` (`Library/`).
Tick an item `[x]` when it's done. 🟡 in the text means the item is done in part.

Where the desktop matters, an item has a box for each of three kinds of desktop. ❌ means it can't be done there.
The goal is for Snowmeter to work on all three.
- **X11**: any X11 session with a window manager (tested in Xephyr with openbox).
- **Wayland (layer-shell)**: Wayland desktops with the layer-shell protocol: KDE, sway, Hyprland, labwc, … (tested in labwc). Skins are layer windows, which we place and drag ourselves.
- **GNOME (Wayland)**: no layer-shell, so Snowmeter restarts itself on X11 through XWayland and uses the X11 code. Without XWayland, skins are normal windows that GNOME places. Untested. (GNOME on X11 counts as X11.)

At startup Snowmeter picks Wayland with layer-shell first, then X11 through XWayland, then plain Wayland windows, and prints which one it uses. A `GDK_BACKEND` set by the user always wins. (In WSLg, X11 windows get a Windows drop shadow; that is WSLg, not Snowmeter.)

## Progress compared with Rainmeter for Windows

- Overall estimate: **15–20%**
- Main app only (without the plugins that come with Rainmeter): **20–25%**
- Roadmap checklist: 91 / 173 ≈ **53%**
- Translations: 0 / 39 = **0%** (the Language Support list; not part of the checklist)

- Compatibility with each kind of desktop (only the desktop boxes count):
  - X11: 5 / 8 ≈ **63%**
  - Wayland (layer-shell): 7 / 8 ≈ **88%**
  - GNOME (Wayland): 4 / 7 ≈ **57%**

## Next up
1. [ ] The skin's own defaults: `[Rainmeter]` `DefaultWindowX`, `DefaultAlwaysOnTop`, … (used when the settings file has nothing yet)
2. [ ] Skin setting `AlphaValue` (the whole skin's transparency)
3. [ ] Skin setting `FadeDuration`
4. [ ] Skin setting `OnHover` (`HideOnMouseOver`): hide or fade when the mouse is over the skin
5. [ ] Keep the anchor in place when `DynamicWindowSize` resizes the skin
6. [ ] Skin setting `LoadOrder`
7. [ ] Skin setting `AutoSelectScreen`
8. [ ] Skin setting `StartHidden` (with the `!Show` / `!Hide` / `!Toggle` skin bangs); also use the `Update=… | Invisible N` rate while hidden (+15 min)
9. [ ] Always on top / on the desktop / normal (Wayland needs compositor support)
10. [ ] Skins that fail to load: message box + deactivate, as Rainmeter (added 2026-11-16)
11. [ ] Startup checks: message box for no skins / settings file not writable
12. [ ] Only one Snowmeter running (a second start hands its skins or bangs to the running copy)
13. [ ] Quiet start on GNOME: skip the layer-shell check there, so gtk4-layer-shell doesn't print "your Wayland compositor does not support the Layer Shell protocol" before we fall back to X11 (task, added 2026-11-19)
14. [ ] GNOME with Ubuntu's desktop icons (DING): when its window is found at startup, `AlwaysOnTop=-1`/`-2` act like `0` (normal), so the skin stays clickable instead of hiding under the icons window (added 2026-11-19)
15. [ ] X11 and GNOME: our own drag, so `KeepOnScreen` / `SnapEdges` work during the drag and Ctrl/Shift can turn snapping off (added 2026-11-19)
16. [ ] Choose Snowmeter's license (GPL-2.0-or-later suggested, so Rainmeter's GPL-2.0 translations in `Languages/` can be shipped; they stay out of git until then) (task, added 2026-11-19)
17. [ ] `IfAbove` / `IfBelow` / `IfEqual` actions
18. [ ] `IfCondition` / `IfTrueAction` / `IfFalseAction` (after `IfAbove`/`IfBelow`/`IfEqual`, sharing its helpers)
19. [ ] `IfMatch` / `IfMatchAction` / `IfNotMatchAction`
20. [ ] Measure `UpdateDivider`
---

## Skin files and variables
- [x] INI parsing, case-insensitive (UTF-8, UTF-16 and Windows-1252; 4 MB cap)
- [x] `@Include` (with checks for loops, nesting depth and files outside the skin folder)
- [x] `[Variables]`, including variables inside variables
- [x] Nested variables `[#Var]` (in `[Variables]`, meter and measure options, and mouse actions)
- [x] Built-in variables: `#@#`, `#CURRENTPATH#`, `#ROOTCONFIGPATH#`, `#SKINSPATH#`, `#CURRENTFILE#`, `#CURRENTSECTION#`
- [x] More built-in variables: `#CURRENTCONFIG#`, `#SCREENAREAWIDTH#`/`HEIGHT#`, `#WORKAREA…#`, `#PROGRAMPATH#`, `#SETTINGSPATH#` (🟡 work area = whole screen for now; monitor numbers follow GTK, not Windows; positions count from the primary monitor, as on Windows, but a primary that is not the leftmost is untested — needs real multi-monitor Linux)
- [x] `MeterStyle` (chained and multiple styles)
- [x] Faster style lookup with a HashMap
- [x] Section variables: `[Measure]`, `[Measure:MaxValue]`, `[Meter:X/Y/W/H/XW/YH]`
- [x] More section variables: `[Measure:%]`, `[Measure:/1000,2]`, `[Measure:MaxValue:/1024,1]`, `:TimeStamp`, `:EscapeRegExp`, `:EncodeUrl`, nested `[&Measure]` (in options and mouse actions)
- [x] Character references `[\1234]` and `[\x20AC]`
- [x] `[$Skin…]` and `[$Display…]` variables (skin position and size, monitors) 🟡 work area = whole screen; `[$SkinVisible]` is always 1; measures with `DynamicVariables=1` don't see new values (as with `!SetVariable`); on GNOME `[$SkinX]`/`[$SkinY]` are where the skin was put, not where it is
- [x] `DynamicVariables=1` (meters and measures)
- [x] The formula language (operators, functions, `pi`/`e`, `0b`/`0o`/hex numbers)

## Window and app
- [x] Command-line launcher (`--list`, `--skins-dir`, `--set-skins-dir`)
- [x] GTK4 window that redraws on the `Update` timer
- [x] Transparent window with no border (needs a compositor on X11)
- [x] Drag the skin around
  - [x] X11: the window manager moves it (`begin_move`)
  - [x] Wayland (layer-shell): we move it, by changing its margins
  - [x] GNOME: through XWayland, as X11 (tested on Ubuntu 25.04, Raspberry Pi 5)
- [x] Remember each skin's position (Rainmeter.ini): `Active=` (no skin on the command line = last time's skins); `WindowX`/`WindowY` with Rainmeter's `%`/`R`/`B`/`@N` forms, saved in the same form (500 ms after a move,  and on close/refresh), and `AnchorX`/`AnchorY`, all four also as a `(formula)` before the modifiers, such as `(1920/4)R` (worked out once when read, as Rainmeter's `ParseFormulaWithModifiers`; a broken one logs `Formula:` and reads as 0). As in Rainmeter, `!Refresh` reads the skin's section again and applies it (position with the new size, `AlwaysOnTop` if changed, `Draggable`, `ClickThrough`, `KeepOnScreen`, `SnapEdges`, `SavePosition`). The other skin settings are the items below
  - [x] X11: read and set through Xlib (`x11.rs`)
  - [x] Wayland (layer-shell): monitor + margins (`wayland.rs`)
  - [x] GNOME: through XWayland, as X11. GNOME's window manager (Mutter) places a new window itself once it takes it over, after our first move, so the saved position is set again at that point (`when_managed`). Tested on Ubuntu 25.04 (Raspberry Pi 5). Long-term plan in Notes and ideas at the end
- [x] Skin setting `SavePosition` (0 = moves aren't saved: a drag or `!Move` still moves the skin, but `WindowX`/`WindowY` keep the old place; not written in when missing, as Rainmeter). Right-click ▸ Settings ▸ Save position switches it, writes `SavePosition=`, and turning it on saves where the skin is now. `!Refresh` sends an unsaved skin back to its saved place, as Rainmeter
- [ ] The skin's own defaults: `[Rainmeter]` `DefaultWindowX`, `DefaultAlwaysOnTop`, … (used when the settings file has nothing yet)
- [ ] Skin setting `AlphaValue` (the whole skin's transparency)
- [ ] Skin setting `FadeDuration`
- [ ] Skin setting `OnHover` (`HideOnMouseOver`): hide or fade when the mouse is over the skin
- [ ] Keep the anchor in place when `DynamicWindowSize` resizes the skin
- [ ] Skin setting `LoadOrder`
- [ ] Skin setting `AutoSelectScreen`
- [ ] Skin setting `StartHidden` (with the `!Show` / `!Hide` / `!Toggle` skin bangs); also use the `Update=… | Invisible N` rate while hidden
- [x] Several skins at once (several on the command line; one with an error is skipped)
- [x] Load, unload and refresh skins
- [ ] Skins that fail to load (Skin.cpp `ReadSkin`): a message box with the `[Message]` text (`NoMetersInSkin`, `UnableToRefreshSkin`, `NewVersionRequired` for `SyntaxVersion` > 2 or a newer `AppVersion`), then the skin is 
        deactivated (`Active=0`), on refresh too. Switching variants closes the old skin first, so a broken variant leaves none running. Today a skin with no meters loads as an empty 1×1 window, and a failed refresh keeps the old skin. Same message box for: `!ActivateConfig` on a missing file (`UnableToActivateSkin`; ours only logs it), Refresh all finding a running skin's file or folder gone (deactivated, `UnableToRefreshSkin`), and an `@Include` loop (`IncludeInfiniteLoop`; ours logs and skips it)
- [ ] Startup checks with a message box, as Rainmeter: no skins in the skins folder (`NoAvailableSkins`, with the path), and a settings file that can't be written (`SettingsNotWritable` + `SettingsReadOnly`, once at startup; today 
        each failed save only logs an error). Not needed: Rainmeter's settings-file encoding fix (it rewrites the file as UTF-16; we keep each file's own encoding)
- [ ] Only one Snowmeter running: starting `snowmeter` again hands its skins or bangs to the running copy instead of opening a second one that fights over the same settings file (Rainmeter: `IsAlreadyRunning` + `WM_COPYDATA`, 
        which opens the Manage window; GTK's `Application` can do this through its unique D-Bus name)
- [ ] Safe start, as Rainmeter: holding Ctrl+Shift while it starts, or after repeated crashes, asks whether to restore the default skins and layout (copied without overwriting the user's own changes). Needs default skins shipped 
        with Snowmeter first
- [x] Right-click menu (as Rainmeter's ContextMenu.cpp: skin name (opens its folder), Variants, the skin's root folder, `ContextTitle`/`ContextAction` (up to 25; 3 or fewer inline under a grey heading, more in a submenu; `---` draws a line; read when the menu opens; `!SetOption Rainmeter Context…`), Settings (position radio, toggles; options we don't have yet are greyed), Edit skin, Refresh skin, Snowmeter ▸ (Skins tree, Edit settings, Refresh all, Exit; Manage, Debug, About, Help, Favorites, Layouts greyed for now), Unload skin. Opens on right-click when no RightMouseUp action ran, or with Ctrl held. Bangs `!SkinMenu [Config]` and `!SkinCustomMenu [Config]`. Text comes through `tr()`, English built in. Differences: no bold skin name (GTK's menu has no markup); Edit skin uses the default app for .ini files, not `ConfigEditor=` yet; Restart is left out; `!SkinMenu` opens at the mouse when it's over that skin, else at the skin's top-left corner, since Wayland can't tell a program where the mouse is elsewhere)
        (Notes: submenus open beside the menu, like Windows (GTK's nested popover). GTK's default sliding menu names
        each submenu page by its label, so repeated names in the Skins tree, such as Demo ▸ Demo or two Clock folders, opened
        the wrong page. In WSLg only, a menu stays on screen a moment after closing, and clicking the skin while the menu is
        open makes it slide under the skin; both are fine in Xephyr. Re-check on the Pi.)
- [ ] Tray icon
  - [ ] X11: most panels show tray icons (StatusNotifierItem)
  - [ ] Wayland (layer-shell): the same
  - [ ] GNOME: hidden unless the user installs the AppIndicator extension
- [ ] Always on top, on the desktop, normal 🟡 `AlwaysOnTop=` from Rainmeter.ini works. `!ZPos` changes it while the skin runs. Still to do: the skin's own `[Rainmeter]` defaults
  - [x] X11: `_NET_WM_STATE_ABOVE`/`BELOW`, sent once the window manager sets `WM_STATE`. No separate "topmost" or "on desktop" level (2 = 1, -2 = -1). Skins are also left out of the taskbar and pager
  - [x] Wayland (layer-shell): layers -2/-1/0 → Bottom, 1 → Top, 2 → Overlay. "Normal" can't exist: layer windows are never mixed with normal ones
  - [ ] GNOME: through XWayland, as X11 🟡 below (`-1`/`-2`) puts the skin under the transparent window of Ubuntu's desktop-icons extension (DING): it still shows, but every click goes to the icons window. With DING turned off it works. On top (`1`/`2`) and normal (`0`) work. Tested on Ubuntu 25.04. Fix in Next up
- [x] Click-through, Draggable, SnapEdges, KeepOnScreen. Missing window settings are written to the settings file when a skin loads, as Rainmeter does
  - `Draggable` (as in Rainmeter, a press that runs a `LeftMouseDownAction` doesn't drag)
    - [x] X11
    - [x] Wayland (layer-shell)
    - [x] GNOME: through XWayland (tested on Ubuntu 25.04)
  - `ClickThrough` (an empty input region). Unlike Windows, holding Ctrl doesn't make a click-through skin clickable: we can't see the mouse over it
    - [ ] X11 🟡 works without a window manager, but openbox's frame still catches the clicks (window managers that composite should be fine; untested)
    - [x] Wayland (layer-shell)
    - [x] GNOME: through XWayland (tested on Ubuntu 25.04)
  - `KeepOnScreen`, `SnapEdges` (10 px, other skins then screen edges)
    - [ ] X11 🟡 applied when the skin stops moving, so it jumps into place; Ctrl/Shift can't turn snapping off. To fix: do the drag ourselves (move the window with each mouse motion, as on Wayland) instead of handing it to the window manager with `begin_move`, so snapping and keeping on screen happen during the drag and Ctrl/Shift can turn snapping off. Fixes GNOME too (added 2026-11-19)
    - [x] Wayland (layer-shell): during the drag; Ctrl/Shift turns snapping off
    - [ ] GNOME: through XWayland 🟡 as X11: applied when the skin stops moving, so it jumps into place; Ctrl/Shift can't turn snapping off (tested on Ubuntu 25.04)

## Manage window
A normal GTK4 window (with a title bar) for running Snowmeter without the terminal, like Rainmeter's Manage dialog.
- [ ] Open it from the tray icon, the right-click menu, or by starting `snowmeter` with no skin
- [ ] Skins tab: tree of every config and .ini in the skins folder, with Load / Unload / Refresh / Edit buttons
- [ ] Skins tab: the selected skin's settings (X/Y, position, transparency, on hover, draggable, click-through, keep on screen, snap edges, load order)
- [ ] Skins tab: skin info from `[Metadata]` (Name, Author, Version, License, Information)
- [ ] Layouts tab: save and load layouts (which skins are loaded, and where)
- [ ] Settings tab: skins folder, text editor, logging
- [ ] Settings tab: language support (pick the language; the app's text read from the language files, right-to-left for Arabic and Hebrew; the languages themselves are listed under Language Support)
- [ ] Log / About window: messages and errors, loaded skins, version
- [ ] Start Snowmeter at login (an XDG autostart `.desktop` file)

## [Rainmeter] section
- [x] `Update`
- [x] `Update` details (as Skin.cpp `ReadUpdateOption`): a number or a `(formula)`; below 0 = one update at load, then none; `| Invisible N` (any case, formula allowed, below 0 = 0) is read and stored; anything else after the number logs `Invalid Update: …` and keeps it. Every skin now gets its first update at load and at `!Refresh`, before the timer and `OnRefreshAction`, as in Rainmeter (before, the first one came a full `Update=` later). Difference: the `Invisible` rate isn't used yet; it will apply while the skin is hidden by `!Hide` (Next up `StartHidden`). Windows also counts a skin covered by other windows as invisible; GTK can't tell us that (X11 `VisibilityNotify` might, later)
        else (ours silently falls back to 1000)
- [x] `DynamicWindowSize`
- [x] `BackgroundMode=2` + `SolidColor`
- [x] `SolidColor2` / `GradientAngle` (gradient background, same edge points as Rainmeter)
- [x] `Background` image, `BackgroundMode` 0/1/3/4, `BackgroundMargins` (0 = image, window takes its size; 3 = cut into 9 pieces at the margins; 4 = tiled; the margins add space around the meters in every mode, as in 
        Skin.cpp `ResizeWindow`)
- [x] `BevelType` (with `BevelColor`/`BevelColor2`; drawn only with `BackgroundMode=2`, as in Rainmeter)
- [x] `SkinWidth` / `SkinHeight`
- [x] `DragMargins` (negative left/top count from the far edge, as in Rainmeter)
- [ ] `DefaultUpdateDivider`
- [x] `OnRefreshAction`, `OnCloseAction`, `OnUpdateAction`, `OnFocusAction`/`OnUnfocusAction`
- [x] Ctrl+C / SIGTERM runs `OnCloseAction` (also SIGHUP, when the terminal closes; through GLib's main loop with `glib-unix`; every skin closes as in Rainmeter's `DeleteAllSkins`, and `Active=` is kept for the next start)
- [ ] `ContextTitle` / `ContextAction`
- [x] `Group` (from the skin and from the settings file; `A|B` any, `A&B` all; bangs `!RefreshGroup`, `!UpdateGroup`, `!RedrawGroup`, `!ZPosGroup`, `!ClickThroughGroup`, `!DraggableGroup`, `!SnapEdgesGroup`, `!KeepOnScreenGroup`, 
        `!SetVariableGroup`, `!DeactivateConfigGroup`; an empty group means every skin, as in Rainmeter)
- [x] `LocalFont`
- [x] `AccurateText` (0, the default, adds GDI+'s spacing like Rainmeter: size/6 each side, size/72 per character, size/8 below; the per-character spacing is only measured, not drawn, until meter `AntiAlias` exists)
- [x] `Blur` / `BlurRegion` (`BlurRegion2`, … up to the first empty one; shapes 1 = rectangle, 2 = rounded rectangle, 3 = ellipse, each number a formula; bangs `!ShowBlur`, `!HideBlur`, `!ToggleBlur`, `!AddBlur`, `!RemoveBlur`, with Rainmeter's quirks: with no `BlurRegion`, Add/Remove do nothing, and `!ShowBlur` on a `Blur=0` skin blurs the whole window. Differences: a rounded rectangle with no `r` is drawn as a rectangle, with a warning (Rainmeter blurs nothing); ignored shapes get a warning; round corners may differ by a pixel. On Windows 8 and later the same call makes the area see-through but no longer blurs it, so on KDE skins look as they did on Windows 7)
  - [x] X11: KWin's `_KDE_NET_WM_BLUR_BEHIND_REGION` property (`x11.rs`). Other window managers ignore it. Tested on KDE Plasma 6.3 (Raspberry Pi 5)
  - [x] Wayland (layer-shell): KDE's blur protocol, `org_kde_kwin_blur_manager` (`wayland.rs`). sway, Hyprland, labwc, … have none, so nothing happens. Tested on KDE Plasma 6.3 (Raspberry Pi 5)
  - ❌ GNOME: no way for a program to ask for blur, on Wayland or through XWayland. The "Blur my Shell" extension (install it with Extension Manager: on Ubuntu 25.04 the apt package `gnome-shell-extension-blur-my-shell` doesn't work) is only a stopgap: tested 2026-11-23 on Ubuntu 25.04 (Pi 5), it blurs the whole skin window while it isn't focused and stops when it gets the focus, whatever `Blur`/`BlurRegion` say (even `Blur=0`). Our own extension later (Notes and ideas, GNOME)

## Interaction
- [x] Mouse actions: `LeftMouseUp/Down/DoubleClickAction` (and the Right/Middle versions)
- [x] `MouseOverAction` / `MouseLeaveAction` (every meter under the mouse gets its over-action, not just the top one; leaves run before overs; a hidden meter counts as left. Differences from Windows: the whole window rectangle 
        counts as the skin, with no transparent-pixel test; a `ClickThrough` skin gets no hover; Button meter focus waits for the Button meter)
- [x] Scroll actions (`MouseScrollUpAction`, `MouseScrollDownAction`, `MouseScrollLeftAction`, `MouseScrollRightAction`; one action per wheel step, chosen like a click: top meter under the mouse, else `[Rainmeter]`; touchpad 
        moves are added up into steps. Up/down tested; left/right and touchpad untested (no hardware). A `ClickThrough` skin gets no scrolls)
- [x] `ToolTipText`, `ToolTipTitle`, `ToolTipIcon`, `ToolTipWidth`, `ToolTipType`, `ToolTipHidden` (on every meter type; the top visible meter under the mouse with its tooltip on wins; `%1`… stay live while it shows; `[Rainmeter]
        ToolTipHidden` is the default for meters. Laid out like Windows: icon + bold title, then the text wrapped at `ToolTipWidth`. Stock icons map to theme icons (`dialog-information`, …); an icon file is loaded once when the meter is built. Differences: `ToolTipType=1` (balloon) draws as a normal tooltip; a `ClickThrough` skin gets no tooltips; in WSLg it hides about 1 s after leaving the meter, to re-check on a real desktop)
- [x] Bangs: `!SetVariable` (formula values; `[#Var]` in actions is read at click time)
- [x] Bangs: `!WriteKeyValue` (Section Key Value [File], as Rainmeter: the file must already exist and be inside the skins or settings folder, `../` is refused, a read-only file gives a warning, `(formula)` values are worked out, 
        the file keeps its encoding, the skin isn't reloaded. Differences: symlinks are followed before the folder check; line breaks in a name or value are refused; new lines use the file's own line endings; a new key goes after any comment right under the section's last key, but before comments that follow a blank line (Rainmeter puts it after all of them))
- [x] Bangs: `!ShowMeter`, `!HideMeter`, `!ToggleMeter` (take effect at the next update, as in Rainmeter)
- [x] Bangs: `!ShowMeterGroup`, `!HideMeterGroup`, `!ToggleMeterGroup` (also `!ShowMeter Group=Name` and the like; a toggle flips each meter on its own; a group with no meters in it is silent, as in Rainmeter)
- [x] Bangs: `!SetOption` on meters and measures (read again at the next update, as in Rainmeter; a measure keeps its state, such as the Calc counter). On `[Rainmeter]`, only the `Context…` options (the right-click menu's custom items)
- [x] Bangs: `!SetOptionGroup` (every meter, then every measure, in the group; also `!SetOption Group=Name …`; an empty value removes the option, even the one written in the file, as in Rainmeter)
- [x] Bangs: `!Update` (restarts the Update timer), `!Redraw`
- [x] Bangs: `!SetUpdate` (changes the skin's Update rate while it runs and restarts the timer, so the next update comes a full interval later; `-1` or less = no more updates; a number or a `(formula)`, anything else = 1000, as in Rainmeter's `Bang::SetUpdate`. `!Refresh` goes back to the file's `Update=`. Difference: the rate is at least 50 ms, like `Update=`, where Windows allows about 10)
- [x] Bangs: `!Refresh` (this skin only; reloads the file, restarts the timer, keeps the old skin on an error)
- [x] Bangs: `!Log` (`Notice`/`Warning`/`Error`/`Debug`; prints to the terminal (stderr) in Rainmeter's log-file format, with control characters shown as `?` so a skin can't send escape codes; the Manage window's Log tab will 
        show the same lines later). Snowmeter's own messages use the same log too: the source is the skin (`config\file.ini`), or `config\file.ini - [Section]` for one section's option, or empty for program messages, as in Rainmeter; Debug lines (mouse-action traces) always print for now (Rainmeter needs `Debug=1`)
- [x] Bangs: `!UpdateMeter`, `!UpdateMeasure` (`*` = all; each runs its `OnUpdateAction`; no repaint until `!Redraw`, which now also resizes a `DynamicWindowSize` skin, as in Rainmeter; meters after an updated one are placed 
        again, since relative X/Y follow it. also `!UpdateMeasureGroup` / `!UpdateMeterGroup` and `Group=Name`, where `*` is a group name)
- [x] Bangs: `!EnableMeasure`, `!DisableMeasure`, `!ToggleMeasure`, `!PauseMeasure`, `!UnpauseMeasure`, `!TogglePauseMeasure` (seen at the next update; disabled = 0, paused = last value)
- [x] Bangs: `!EnableMeasureGroup`, `!DisableMeasureGroup`, `!ToggleMeasureGroup`, `!PauseMeasureGroup`, `!UnpauseMeasureGroup`, `!TogglePauseMeasureGroup` (also the newer `Group=Name` form of the plain bangs, e.g. 
        `!DisableMeasure Group=Clocks`; a group with no measures in it is silent, as in Rainmeter)
- [x] Bangs: `!CommandMeasure`, `!CommandMeasureGroup` (and `Group=Name`; the measure gets the command even when disabled or paused, as in Rainmeter; the old one-argument form `"Measure Command"` is split at the first space with 
        a warning. No measure takes commands yet: each says `Not supported` until Loop, Script or a plugin adds its own)
- [x] Bangs: `!Move` (plain pixel numbers from the primary monitor; KeepOnScreen applies, as in Rainmeter)
- [x] Bangs for the window settings: `!ZPos`, `!Draggable`, `!ClickThrough`, `!KeepOnScreen`, `!SnapEdges` (1, 0, or -1 to toggle; the new value is saved to the settings file)
- [x] Bangs: `!ActivateConfig`, `!DeactivateConfig`, `!ToggleConfig`
- [x] Bangs aimed at other skins: an extra config argument, or `*` for all (Rainmeter's DoBang rule)
- [ ] Running external programs from actions (`["program"]`)
- [ ] `IfCondition` / `IfTrueAction` / `IfFalseAction`
- [ ] `IfAbove` / `IfBelow` / `IfEqual` actions
- [ ] `IfMatch` actions
- [x] `OnChangeAction` (on measures; fires when the value or the text changed since the last update that reached the measure; the first update only records them; not while paused or disabled; also after `!UpdateMeasure`)
- [x] `OnUpdateAction` on meters and measures (every section reads it, Section.cpp)

## General measure options
- [x] `MinValue` / `MaxValue`
- [x] `InvertMeasure` (MaxValue − value + MinValue when the value is read, as Rainmeter's `GetValue`; text values such as a Time's `Format` stay as they are; also through `!SetOption`. Difference: a disabled inverted measure reads 0,  
        where Rainmeter gives MaxValue + MinValue)
- [ ] `UpdateDivider` (reverted earlier)
- [ ] `Substitute` / `RegExpSubstitute`
- [ ] `AverageSize`
- [x] `Disabled` / `Paused`
- [x] `Group` (`#Variables#` filled in; read again after `!SetOption`)

## Measures
- [x] `CPU`: total usage
- [ ] `CPU`: `Processor=N`
- [x] `PhysicalMemory`
- [ ] `Memory`
- [ ] `SwapMemory` (reverted earlier)
- [x] `Time`: `Format`, `TimeZone` 🟡
- [ ] `Time`: `TimeStamp`, `TimeStampFormat`, `DaylightSavingTime`, `FormatLocale`
- [x] `Calc`: `Formula`, `Counter`, `Random`, `LowBound`/`HighBound`, `UpdateRandom`
- [ ] `Calc`: `UniqueRandom`
- [ ] `FreeDiskSpace` (reverted earlier)
- [ ] `NetIn` / `NetOut` / `NetTotal`
- [ ] `Uptime`
- [ ] `Loop`
- [ ] `String`
- [ ] `Script` (Lua)
- [ ] Ex-plugins that make sense on Linux: `SysInfo`, `Process`, `Power`, `WifiStatus`, `FolderInfo`, `Ping`, `WebParser`, `NowPlaying`, `Audio`/`AudioLevel`, `RunCommand`, `InputText`, `FileView`, `UsageMonitor`
- [ ] Skip or find Linux replacements for: `PerfMon`, `Registry`, `iTunes`, `SpeedFan`, `CoreTemp`, `WindowMessage`, `RecycleManager`, `ResMon`

## General meter options
- [x] `X` / `Y`, absolute and relative `r`/`R` (checked against the source)
- [x] `W` / `H`
- [x] `Padding`
- [x] `SolidColor`
- [x] `Hidden`
- [x] `MeasureName`, `MeasureName2`, …
- [x] `SolidColor2` / `GradientAngle` (gradient meter background, same as the skin background)
- [x] `BevelType` / `BevelColor` / `BevelColor2` (drawn 2 px outside the meter box; `AntiAlias` not read yet, so always the half-pixel offset)
- [ ] `AntiAlias`
- [ ] `TransformationMatrix`
- [ ] `UpdateDivider`
- [x] `Group` (`#Variables#` and section variables filled in; read again on every rebuild, so `!SetOption` and `DynamicVariables` change it)
- [ ] `Container`

## String meter
- [x] `Text` with `%1 %2 …`
- [x] `FontFace`, `FontSize`, `FontColor`
- [x] `StringStyle`
- [x] `StringAlign` (all 12 values)
- [x] `Prefix` / `Postfix`
- [x] `StringCase` (rules checked against Windows' LCMapString)
- [ ] `StringEffect` / `FontEffectColor`
- [ ] `ClipString` / `ClipStringW` / `ClipStringH`
- [ ] Multi-line text (`#CRLF#`, line breaks from measures)
- [ ] `FontWeight`
- [ ] `Angle`
- [ ] `NumOfDecimals`, `AutoScale`, `Scale`, `Percentual` (reverted earlier)
- [ ] `TrailingSpaces`
- [ ] `InlineSetting` / `InlinePattern`

## Bar meter
- [x] `BarColor`, `BarOrientation`
- [x] `Flip`
- [ ] `BarImage`
- [ ] `BarBorder`

## Image meter
- [x] `ImageName`, `ImagePath`, `ImageAlpha`, `W`/`H`
- [ ] `PreserveAspectRatio`
- [ ] `Tile`
- [ ] `ScaleMargins`
- [ ] `MaskImageName`
- [ ] Image modifiers: `ImageTint`, `ImageCrop`, `ImageRotate`, `ImageFlip`, `Greyscale`, `ColorMatrix`, `UseExifOrientation` (one shared image helper, as Rainmeter's GeneralImage; also used by `BarImage`, `MaskImageName`, 
        the Histogram images, `Rotator`, `Button` and `Bitmap`, with their own prefixes such as `PrimaryImageTint`)

## Line meter
- [x] `LineCount`, `LineColor`, `Scale`, `LineWidth`, `HorizontalLines`, `AutoScale`, `GraphStart`, `GraphOrientation`, `Flip`
- [ ] `LineJoin` / `MiterLimit`
- [ ] `TransformStroke`

## Histogram meter
- [x] `PrimaryColor`, `SecondaryColor`, `BothColor`, `AutoScale`, `GraphStart`, `GraphOrientation`, `Flip`
- [ ] `PrimaryImage` / `SecondaryImage` / `BothImage`

## Missing meter types
- [ ] `Shape` (rectangles, ellipses, paths, strokes, fills)
- [ ] `Roundline`
- [ ] `Rotator`
- [ ] `Bitmap`
- [ ] `Button`
- [ ] `Svg` (newer fork only)
- [ ] `TextEdit` (newer fork only)

## Skin installer (.rmskin)
A `.rmskin` is a ZIP file with a short "RMSKIN" footer at the end and an `RMSKIN.ini` inside. Reference: `SkinInstaller/` in `../rainmeter_code`.
- [ ] Read the package: the ZIP plus its footer, safely (no paths escaping the target folder, no symlinks, size limits)
- [ ] `RMSKIN.ini`: `Name`, `Author`, `Version`, `MinimumRainmeter`
- [ ] Install `Skins/` into the skins folder, backing up a skin that's already there
- [ ] `VariableFiles`: keep the user's own values when a skin is upgraded
- [ ] `Layouts/` and `@Vault`; skip `Plugins/` (Windows DLLs) with a warning
- [ ] `LoadType` / `Load`: load the skin or layout after installing
- [ ] Installer window: show what's in the package, then Install / Cancel
- [ ] Open `.rmskin` files by double-clicking them (a `.desktop` file and a MIME type)

## Language Support
The languages in Rainmeter's Manage window (Settings tab), named as it shows them; the last three are only in the source (`Language/*.ini`), not in the released Rainmeter. Arabic and Hebrew are right-to-left. They count in the Translations line, not in the checklist.

(How the language files work: see Notes and ideas at the end.)

- [ ] Arabic - العربية (المملكة العربية السعودية)
- [ ] Bulgarian - български (България)
- [ ] Chinese (Simplified) - 中文(中国)
- [ ] Chinese (Traditional) - 中文(台灣)
- [ ] Czech - čeština (Česko)
- [ ] Danish - dansk (Danmark)
- [ ] Dutch - Nederlands (Nederland)
- [ ] English - English (United States)
- [ ] Estonian - eesti (Eesti)
- [ ] Finnish - suomi (Suomi)
- [ ] French - français (France)
- [ ] German - Deutsch (Deutschland)
- [ ] Greek - Ελληνικά (Ελλάδα)
- [ ] Hebrew - עברית (ישראל)
- [ ] Hungarian - magyar (Magyarország)
- [ ] Indonesian - Indonesia (Indonesia)
- [ ] Italian - italiano (Italia)
- [ ] Japanese - 日本語 (日本)
- [ ] Korean - 한국어(대한민국)
- [ ] Malay - Melayu (Malaysia)
- [ ] Norwegian Bokmål - norsk bokmål (Norge)
- [ ] Polish - polski (Polska)
- [ ] Portuguese - português (Brasil)
- [ ] Portuguese - português (Portugal)
- [ ] Romanian - română (România)
- [ ] Russian - русский (Россия)
- [ ] Serbian (Cyrillic) - српски (Србија и Црна Гора (Бивша))
- [ ] Serbian (Latin) - srpski (Srbija i Crna Gora (Bivša))
- [ ] Slovak - slovenčina (Slovensko)
- [ ] Slovenian - slovenščina (Slovenija)
- [ ] Spanish - español (España, alfabetización internacional)
- [ ] Swedish - svenska (Sverige)
- [ ] Thai - ไทย (ไทย)
- [ ] Turkish - Türkçe (Türkiye)
- [ ] Ukrainian - українська (Україна)
- [ ] Vietnamese - Tiếng Việt (Việt Nam)
- [ ] Croatian (source only)
- [ ] Latvian (source only)
- [ ] Lithuanian (source only)

## Notes and ideas
Plans and ideas that aren't checklist items. They don't count in any progress line.

### Language files
- How (decided 2026-11-14): unlike Rainmeter, which builds each `Language/*.ini` into a `.dll`, Snowmeter keeps them as plain `.ini` files in the same format (`[RainmeterMenu]`, `[SkinMenu]`, … with the same keys), so anyone can fix a translation or add a language with a text editor. English stays built into the program (`src/lang.rs`) as the fallback for missing files and keys. Files are looked for in the user's folder first (so a user's own edit wins), then in the program's folder. Read with the same size limits as skins; the text is only ever shown, never run. Before shipping Rainmeter's translations (GPL-2.0), Snowmeter needs a GPL-compatible license. The program menu's text is kept under `[SnowmeterMenu]` in `lang.rs`: when a language file is read, its `[RainmeterMenu]` section is read as `[SnowmeterMenu]`, so Rainmeter's files work unchanged (decided 2026-11-17).
- Idea (user, not decided yet): Read each .ini files in Language folder for "[LanguageSettings]", "LanguageName=..." for the name to show in Language: list in Settings tab if key empty or missing show file name less ".ini"
  - Suggestion (Claude, open): a middle fallback. Rainmeter's own files have no `LanguageName=` (Rainmeter asks Windows for the name of the file's language number), so a file copied from Rainmeter would show as just "fr". Order: `LanguageName=`, then a small built-in table from file name to name (the names in Language Support above), then the file name.
  - Suggestion (Claude, open): `LanguageName=` holds the full name as Rainmeter shows it, "French - Français (France)", not just "Français", so people find their language by its English or its own name.
  - Suggestion (Claude, open): right-to-left needs no new key: `[LanguageSettings]` already has `RTL=1` (Arabic, Hebrew); GTK can flip the layout with `set_default_direction`.
  - Safety: the name is only shown; cap its length and drop control characters, as for skin text.

### GNOME (Wayland)
- Plan for GNOME (no layer-shell), as Desktop Icons NG does it: one transparent, borderless, monitor-sized window per monitor (not fullscreen, which would cover the top bar), with an input region covering only the skins, so clicks elsewhere pass through, and every skin drawn inside it at its own x,y (we choose the skin positions). Paired with a small GNOME Shell extension (JavaScript) that puts each window on its monitor, keeps it on the desktop layer, and hides it from Alt+Tab, the dock and the overview: Mutter gives those powers only to extensions. Without the extension: limited mode, where skins show and can be clicked but GNOME may put the window on the wrong monitor or below the top bar, it stacks like a normal app, and it shows in Alt+Tab. Costs either way: all skins on a monitor stack together (no per-skin on top / on desktop), clicking a skin takes focus, no skin across two monitors. To keep this open, skins should draw into a target with an offset instead of assuming they own a whole window.
- Blur on GNOME (idea, 2026-11-21): only GNOME Shell can see the pixels behind a window, so blur needs an extension. The same extension as above could read each skin's blur rectangles (for example the `_KDE_NET_WM_BLUR_BEHIND_REGION` property we already set on X11) and put GNOME's own blur effect (`Shell.BlurEffect`) there, so `BlurRegion` and the blur bangs work as on KDE. Costs: JavaScript, upkeep at each GNOME release (the extension API changes often), and the user has to enable it. Estimate ~4–6 h. Until then: "Blur my Shell" (its Applications setting) blurs whole skin windows, but only while they aren't focused, and ignores `Blur`/`BlurRegion` (tested 2026-11-23). Rejected: blurring the wallpaper ourselves (wrong as soon as a window is behind the skin) and a near-solid noisy layer (doesn't blur, and looks less like Windows, where `Blur` does nothing visible since Windows 8).
