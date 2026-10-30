# Snowmeter

A port of [Rainmeter](https://www.rainmeter.net/) to Linux. Rainmeter is the Windows program that shows skins (small widgets) on the desktop.
The goal is for existing Rainmeter skins to keep working, with good performance and security.

Written in Rust (edition 2024), with GTK4 and Cairo for the window and drawing.

## Status

Early development: skins are started from the terminal.
The full checklist, compared with Rainmeter for Windows, is in [ROADMAP.md](ROADMAP.md).

- Overall estimate: **15–20%**
- Main app only (without the plugins that come with Rainmeter): **20–25%**
- Roadmap checklist: 64 / 155 ≈ **41%**

- Compatibility with each kind of desktop (the desktop-dependent items in the roadmap):
  - X11: 4 / 7 ≈ **57%**
  - Wayland with layer-shell (KDE, sway, Hyprland, labwc, …): 6 / 7 ≈ **86%**
  - GNOME (Wayland): 0 / 7 = **0%** (runs through XWayland, untested)

## What works

- [x] Skin files: `[Variables]`, `@Include`, `MeterStyle`, built-in variables, section variables, formulas, `DynamicVariables`, `LocalFont`
- [x] Measures: `CPU`, `PhysicalMemory`, `Time`, `Calc`
- [x] Meters: `String`, `Bar`, `Image`, `Line`, `Histogram`
- [x] Transparent, borderless window that can be dragged with the left button
- [x] Mouse actions and the first bangs: `!ShowMeter`, `!HideMeter`, `!ToggleMeter`, `!SetVariable`, `!Update`, `!Redraw`, `!Refresh`, `!ActivateConfig`, `!DeactivateConfig`, `!ToggleConfig`, `!ZPos`, `!Draggable`, `!ClickThrough`, `!KeepOnScreen`, `!SnapEdges` (bangs can also be aimed at other skins)
- [x] Several skins at once (give several on the command line); with no skin given, last time's skins are loaded
- [x] Skin positions are remembered on X11, and on Wayland desktops with layer-shell (KDE, sway, labwc, …)

- [x] Per-skin window settings: `AlwaysOnTop`, `Draggable`, `ClickThrough`, `KeepOnScreen`, `SnapEdges`, `AnchorX`/`AnchorY`

## Not yet

- [ ] Remembered positions on GNOME, and the other skin settings; right-click menu, tray icon
- [ ] Number formatting (`NumOfDecimals`, `AutoScale`)
- [ ] Disk, network and other measures; plugins; Lua scripts
- [ ] Skin manager window
- [ ] Installing `.rmskin` packages

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
