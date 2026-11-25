# Snowmeter

A port of [Rainmeter](https://www.rainmeter.net/) to Linux. Rainmeter is the Windows program that shows skins (small widgets) on the desktop.
The goal is for existing Rainmeter skins to keep working, with good performance and security.

Written in Rust (edition 2024), with GTK4 and Cairo for the window and drawing.

## Status

Early development: skins are started from the terminal.
The full checklist, compared with Rainmeter for Windows, is in [ROADMAP.md](ROADMAP.md).

- Overall estimate: **15–20%**
- Main app only (without the plugins that come with Rainmeter): **20–25%**
- Roadmap checklist: 92 / 174 ≈ **53%**
- Translations: 0 / 39 = **0%**

- Compatibility with each kind of desktop (the desktop-dependent items in the roadmap):
  - X11: 5 / 9 ≈ **56%**
  - Wayland with layer-shell (KDE, sway, Hyprland, labwc, …): 7 / 9 ≈ **78%**
  - GNOME (Wayland): 4 / 8 = **50%** (runs through XWayland; first tests on Ubuntu 25.04)

## What works

- [x] Skin files: `[Variables]`, `@Include`, `MeterStyle`, built-in variables, section variables, formulas, `DynamicVariables`, `LocalFont`
- [x] Measures: `CPU`, `PhysicalMemory`, `Time`, `Calc`
- [x] Meters: `String`, `Bar`, `Image`, `Line`, `Histogram`
- [x] Transparent, borderless window that can be dragged with the left button
- [x] Mouse actions and the first bangs: `!ShowMeter`, `!HideMeter`, `!ToggleMeter`, `!SetVariable`, `!Update`, `!Redraw`, `!Refresh`, `!ActivateConfig`, `!DeactivateConfig`, `!ToggleConfig`, `!ZPos`, `!Draggable`, `!ClickThrough`, `!KeepOnScreen`, `!SnapEdges`, `!ShowBlur`, `!HideBlur`, `!ToggleBlur`, `!AddBlur`, `!RemoveBlur` (bangs can also be aimed at other skins)
- [x] Several skins at once (give several on the command line); with no skin given, last time's skins are loaded
- [x] Skin positions are remembered on X11, and on Wayland desktops with layer-shell (KDE, sway, labwc, …)

- [x] Per-skin window settings: `AlwaysOnTop`, `Draggable`, `ClickThrough`, `KeepOnScreen`, `SnapEdges`, `SavePosition`, `AnchorX`/`AnchorY`, and the skin's own `Default…` settings for its first load
- [x] Blur behind the skin (`Blur`, `BlurRegion`) on KDE Plasma, on X11 and Wayland

## Not yet

- [ ] Remembered positions on GNOME, and the other skin settings; right-click menu, tray icon
- [ ] Number formatting (`NumOfDecimals`, `AutoScale`)
- [ ] Disk, network and other measures; plugins; Lua scripts
- [ ] Skin manager window
- [ ] Installing `.rmskin` packages

## Differences from Rainmeter

Skins are meant to work as they do on Windows. Where Snowmeter does something differently, by choice or because Linux works differently, it's listed here, grouped like the roadmap.

### Skin files and variables

- `#WORKAREA…#` and `[$…]` work-area values are the whole screen for now.
- Monitor numbers (`@N`) follow GTK's order, which can differ from Windows'.
- `[$SkinVisible]` is always 1. On GNOME, `[$SkinX]`/`[$SkinY]` are where the skin was put, not where it is now.

### Window and app

- Settings live in `~/Rainmeter/Rainmeter.ini` if it exists, otherwise `~/Snowmeter/Snowmeter.ini`.
- `AlwaysOnTop`: X11 has no separate "topmost" or "on desktop" level, so 2 acts as 1 and -2 as -1. Wayland (layer-shell) has no "normal" level: -2, -1 and 0 all sit at the bottom, 1 is on top, 2 is above everything.
- On X11 and GNOME, `SnapEdges` and `KeepOnScreen` apply when the drag ends, so the skin jumps into place, and Ctrl/Shift can't turn snapping off. On Wayland they work during the drag, as on Windows.
- Right-click menu: the skin name isn't bold (GTK's menus have no markup); Edit skin opens the `.ini` with the desktop's default app (`ConfigEditor=` not yet); there's no Restart item; the program submenu is called "Snowmeter".
- A skin's `Default…` settings: on the config's very first load, options read once at load that use `[$SkinX]`/`[$SkinY]` see 0,0 instead of the default position (it's right from the first update on, and on every later load).
- `!SkinMenu` opens at the mouse when it's over that skin, otherwise at the skin's top-left corner (Wayland doesn't tell a program where the mouse is elsewhere).
- The log is printed to the terminal (stderr), in Rainmeter's log-file format. Control characters are shown as `?`. Debug lines always print (Rainmeter needs `Debug=1`).

### [Rainmeter] section

- `Update=` and `!SetUpdate` are at least 50 ms (Windows allows about 10).
- `Update=… | Invisible N` is read but not used yet, and a skin covered by other windows doesn't count as invisible (GTK can't tell).
- `AccurateText=0`: the extra space per character is measured but not drawn, until meter `AntiAlias` exists.
- `BackgroundMode=0` with an image that can't be loaded logs an error and shows no background (Rainmeter makes a 0×0 window).
- `Blur` only works on KDE Plasma (X11 and Wayland), where KWin does the blur. Other desktops (GNOME, sway, Hyprland, labwc, …) give programs no way to ask for blur, so nothing happens there. On GNOME, the "Blur my Shell" extension can blur whole skin windows: install it with Extension Manager (on Ubuntu 25.04 the apt package `gnome-shell-extension-blur-my-shell` doesn't work), then in its Applications settings add Snowmeter's windows to the list. It blurs the whole skin window while it isn't focused, and stops when you click the skin; it ignores `Blur` and `BlurRegion` (even `Blur=0`). On Windows, `Blur` only blurs on Vista and 7: since Windows 8 the area turns see-through but isn't blurred, so on KDE a skin can look blurrier than it does on Windows 10/11.
- `BlurRegion=2,…` (rounded rectangle) with no corner radius is drawn as a plain rectangle, with a warning in the log; Rainmeter blurs nothing there. Shapes Rainmeter ignores (zero width or height, unknown type) are ignored here too, but with a warning.
- Rounded corners and ellipses in `BlurRegion` can differ from Windows by a pixel.
- A number in a blur shape that is a broken formula counts as 0, like Rainmeter, but without Rainmeter's `Formula:` error in the log.

### Interaction

- Mouse over/leave: the whole window rectangle counts as the skin (no test for transparent pixels).
- A `ClickThrough` skin gets no hover actions, scroll actions or tooltips.
- `ToolTipType=1` (balloon) is drawn as a normal tooltip.
- `!WriteKeyValue`: symlinks are followed before the folder check; line breaks in a name or value are refused; new lines use the file's own line endings; a new key goes after comments right under the section's last key, but before comments that follow a blank line (Rainmeter puts it after all of them).

### Measures

- `InvertMeasure`: a disabled inverted measure reads 0 (Rainmeter gives MaxValue + MinValue).

### Meters

- `BevelType` always uses the half-pixel offset, until `AntiAlias` exists.

## Building

You need Rust and the GTK4 development files. On Debian, Ubuntu or Kali:

```
sudo apt install libgtk-4-dev libgtk4-layer-shell-dev build-essential
cargo build --release
```

Run the tests with `cargo test`.

## Usage

```
snowmeter [options] <skin>
```

- `<skin>`: a path to an `.ini` file, or a name inside the skins folder, such as `Demo/Clock/Clock.ini`
- `--list`: show the skins that were found
- `--skins-dir FOLDER`: use this skins folder for this run only
- `--set-skins-dir FOLDER`: remember this skins folder for future runs
- `--help`: show all options

Transparency needs a compositor (most desktops have one).

## Folders and settings

- Skins live in their own folder, separate from the program.
- The settings file is `~/Rainmeter/Rainmeter.ini` if it exists, otherwise `~/Snowmeter/Snowmeter.ini` (created on the first run).
- The skins folder is chosen in this order:
  1. `--skins-dir`
  2. the `RAINMETER_SKINS` environment variable
  3. `SkinPath=` in the settings file
  4. `Skins` inside the settings folder

## Credits

Based on the behaviour of the official [Rainmeter source](https://github.com/rainmeter/rainmeter). Snowmeter is a separate project, not made by the Rainmeter team.
