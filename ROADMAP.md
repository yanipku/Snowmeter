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

- Overall estimate: **10–15%**
- Roadmap checklist: 58 / 153 ≈ **38%**

- Compatibility with each kind of desktop (only the desktop boxes count):
  - X11: 4 / 7 ≈ **57%**
  - Wayland (layer-shell): 6 / 7 ≈ **86%**
  - GNOME (Wayland): 0 / 7 = **0%**

## Next up
1. [ ] Remember each skin's position and settings (our own Rainmeter.ini; Wayland limits window placement)
2. [ ] Always on top / on the desktop / normal (Wayland needs compositor support)
3. [ ] `[Rainmeter]` actions: `OnRefreshAction`, `OnUpdateAction`, `OnCloseAction`, `OnFocusAction`/`OnUnfocusAction`
4. [ ] The rest of the `[Rainmeter]` section: `Background` image with `BackgroundMode` 0/1/3/4 and `BackgroundMargins`, `SolidColor2`/`GradientAngle`, `BevelType`, `SkinWidth`/`SkinHeight`, `DragMargins`, `DefaultUpdateDivider`, `ContextTitle`/`ContextAction`, `Group`, `AccurateText`, `Blur`/`BlurRegion` (see "[Rainmeter] section" below)
5. [ ] The rest of "Interaction": `MouseOverAction`/`MouseLeaveAction`, scroll actions, tooltips, the other bangs (`!WriteKeyValue`, the meter-group bangs, `!SetOptionGroup`, `!UpdateMeter`/`!UpdateMeasure`, `!CommandMeasure`), `IfCondition`, `IfAbove`/`IfBelow`/`IfEqual`, `IfMatch`, `OnChangeAction` (see "Interaction" below)
6. [ ] The rest of the general measure options: `InvertMeasure`, `UpdateDivider`, `Substitute`/`RegExpSubstitute`, `AverageSize`, `Group` (see "General measure options" below)
7. [ ] The rest of the general meter options: `SolidColor2`/`GradientAngle`, `BevelType`/`BevelColor`/`BevelColor2`, `AntiAlias`, `TransformationMatrix`, `UpdateDivider`, `Group`, `Container` (see "General meter options" below)
8. [ ] Running programs from actions (safely: launched directly, never through a shell)
9. [ ] The missing measures, and the missing options of the measures we have (see "Measures" below)
10. [ ] Finish the meters we have: String, Bar, Image, Line, Histogram (see their sections below)
11. [ ] The missing meter types: `Shape`, `Roundline`, `Rotator`, `Bitmap`, `Button`, `Svg`, `TextEdit` (see "Missing meter types" below)
12. [ ] Right-click menu (with `ContextTitle`/`ContextAction`, and `!SetOption Rainmeter Context…`)
13. [ ] Tray icon (GNOME hides tray icons without an extension)
14. [ ] Manage window: a GUI for Snowmeter outside the terminal, like Rainmeter's Manage dialog (see "Manage window" below)
15. [ ] SkinInstaller: install `.rmskin` packages (see "Skin installer" below)

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
  - [ ] GNOME: GNOME should move it, as on X11 (untested)
- [ ] Remember each skin's position and settings (Rainmeter.ini) 🟡 `Active=` (no skin on the command line = last time's skins); `WindowX`/`WindowY` with Rainmeter's `%`/`R`/`B`/`@N` forms, saved in the same form (500 ms after a move, and on close/refresh), and `AnchorX`/`AnchorY`. Still to do: keeping the anchor in place when `DynamicWindowSize` resizes the skin, formulas in `WindowX`/`WindowY`, the other settings
  - [x] X11: read and set through Xlib (`x11pos.rs`)
  - [x] Wayland (layer-shell): monitor + margins (`wlpos.rs`)
  - [ ] GNOME: through XWayland, as X11 (untested). Long-term plan below
  - Plan for GNOME (no layer-shell), as Desktop Icons NG does it: one transparent, borderless, monitor-sized window per monitor (not fullscreen, which would cover the top bar), with an input region covering only the skins, so clicks elsewhere pass through, and every skin drawn inside it at its own x,y (we choose the skin positions). Paired with a small GNOME Shell extension (JavaScript) that puts each window on its monitor, keeps it on the desktop layer, and hides it from Alt+Tab, the dock and the overview: Mutter gives those powers only to extensions. Without the extension: limited mode, where skins show and can be clicked but GNOME may put the window on the wrong monitor or below the top bar, it stacks like a normal app, and it shows in Alt+Tab. Costs either way: all skins on a monitor stack together (no per-skin on top / on desktop), clicking a skin takes focus, no skin across two monitors. To keep this open, skins should draw into a target with an offset instead of assuming they own a whole window.
- [x] Several skins at once (several on the command line; one with an error is skipped)
- [x] Load, unload and refresh skins
- [ ] Right-click menu
- [ ] Tray icon
  - [ ] X11: most panels show tray icons (StatusNotifierItem)
  - [ ] Wayland (layer-shell): the same
  - [ ] GNOME: hidden unless the user installs the AppIndicator extension
- [ ] Always on top, on the desktop, normal 🟡 `AlwaysOnTop=` from Rainmeter.ini works. `!ZPos` changes it while the skin runs. Still to do: the skin's own `[Rainmeter]` defaults
  - [x] X11: `_NET_WM_STATE_ABOVE`/`BELOW`, sent once the window manager sets `WM_STATE`. No separate "topmost" or "on desktop" level (2 = 1, -2 = -1). Skins are also left out of the taskbar and pager
  - [x] Wayland (layer-shell): layers -2/-1/0 → Bottom, 1 → Top, 2 → Overlay. "Normal" can't exist: layer windows are never mixed with normal ones
  - [ ] GNOME: through XWayland, as X11 (untested; Mutter may honour "above" loosely)
- [x] Click-through, Draggable, SnapEdges, KeepOnScreen. Missing window settings are written to the settings file when a skin loads, as Rainmeter does
  - `Draggable` (as in Rainmeter, a press that runs a `LeftMouseDownAction` doesn't drag)
    - [x] X11
    - [x] Wayland (layer-shell)
    - [ ] GNOME: should work (untested)
  - `ClickThrough` (an empty input region). Unlike Windows, holding Ctrl doesn't make a click-through skin clickable: we can't see the mouse over it
    - [ ] X11 🟡 works without a window manager, but openbox's frame still catches the clicks (window managers that composite should be fine; untested)
    - [x] Wayland (layer-shell)
    - [ ] GNOME: should work (untested)
  - `KeepOnScreen`, `SnapEdges` (10 px, other skins then screen edges)
    - [ ] X11 🟡 applied when the skin stops moving, so it jumps into place; Ctrl/Shift can't turn snapping off
    - [x] Wayland (layer-shell): during the drag; Ctrl/Shift turns snapping off
    - [ ] GNOME: through XWayland, as X11 (untested)

## Manage window
A normal GTK4 window (with a title bar) for running Snowmeter without the terminal, like Rainmeter's Manage dialog.
- [ ] Open it from the tray icon, the right-click menu, or by starting `snowmeter` with no skin
- [ ] Skins tab: tree of every config and .ini in the skins folder, with Load / Unload / Refresh / Edit buttons
- [ ] Skins tab: the selected skin's settings (X/Y, position, transparency, on hover, draggable, click-through, keep on screen, snap edges, load order)
- [ ] Skins tab: skin info from `[Metadata]` (Name, Author, Version, License, Information)
- [ ] Layouts tab: save and load layouts (which skins are loaded, and where)
- [ ] Settings tab: skins folder, text editor, logging
- [ ] Log / About window: messages and errors, loaded skins, version
- [ ] Start Snowmeter at login (an XDG autostart `.desktop` file)

## [Rainmeter] section
- [x] `Update`
- [x] `DynamicWindowSize`
- [x] `BackgroundMode=2` + `SolidColor`
- [ ] `SolidColor2` / `GradientAngle` (gradient background)
- [ ] `Background` image, `BackgroundMode` 0/1/3/4, `BackgroundMargins`
- [ ] `BevelType`
- [ ] `SkinWidth` / `SkinHeight`
- [ ] `DragMargins`
- [ ] `DefaultUpdateDivider`
- [ ] `OnRefreshAction`, `OnCloseAction`, `OnUpdateAction`, `OnFocusAction`/`OnUnfocusAction`
- [ ] `ContextTitle` / `ContextAction`
- [ ] `Group`
- [x] `LocalFont`
- [ ] `AccurateText`
- [ ] `Blur` / `BlurRegion` (X11: only some compositors, e.g. KWin's blur hint · Wayland (layer-shell): KDE has a blur protocol, others vary · GNOME: no)

## Interaction
- [x] Mouse actions: `LeftMouseUp/Down/DoubleClickAction` (and the Right/Middle versions)
- [ ] `MouseOverAction` / `MouseLeaveAction`
- [ ] Scroll actions (`MouseScrollUpAction`, …)
- [ ] `ToolTipText`, `ToolTipTitle`, `ToolTipIcon`, `ToolTipWidth`, `ToolTipType`, `ToolTipHidden`
- [x] Bangs: `!SetVariable` (formula values; `[#Var]` in actions is read at click time)
- [ ] Bangs: `!WriteKeyValue`
- [x] Bangs: `!ShowMeter`, `!HideMeter`, `!ToggleMeter` (take effect at the next update, as in Rainmeter)
- [ ] Bangs: `!ShowMeterGroup`, `!HideMeterGroup`, `!ToggleMeterGroup`
- [x] Bangs: `!SetOption` on meters and measures (read again at the next update, as in Rainmeter; a measure keeps its state, such as the Calc counter). `[Rainmeter]` (`ContextTitle`/`ContextAction`) waits for the right-click menu
- [ ] Bangs: `!SetOptionGroup`
- [x] Bangs: `!Update` (restarts the Update timer), `!Redraw`
- [x] Bangs: `!Refresh` (this skin only; reloads the file, restarts the timer, keeps the old skin on an error)
- [ ] Bangs: `!UpdateMeter`, `!UpdateMeasure`
- [x] Bangs: `!EnableMeasure`, `!DisableMeasure`, `!ToggleMeasure`, `!PauseMeasure`, `!UnpauseMeasure`, `!TogglePauseMeasure` (seen at the next update; disabled = 0, paused = last value)
- [ ] Bangs: `!CommandMeasure`
- [x] Bangs: `!Move` (plain pixel numbers from the primary monitor; KeepOnScreen applies, as in Rainmeter)
- [x] Bangs for the window settings: `!ZPos`, `!Draggable`, `!ClickThrough`, `!KeepOnScreen`, `!SnapEdges` (1, 0, or -1 to toggle; the new value is saved to the settings file)
- [x] Bangs: `!ActivateConfig`, `!DeactivateConfig`, `!ToggleConfig`
- [x] Bangs aimed at other skins: an extra config argument, or `*` for all (Rainmeter's DoBang rule)
- [ ] Running external programs from actions (`["program"]`)
- [ ] `IfCondition` / `IfTrueAction` / `IfFalseAction`
- [ ] `IfAbove` / `IfBelow` / `IfEqual` actions
- [ ] `IfMatch` actions
- [ ] `OnChangeAction`

## General measure options
- [x] `MinValue` / `MaxValue`
- [ ] `InvertMeasure` (reverted earlier)
- [ ] `UpdateDivider` (reverted earlier)
- [ ] `Substitute` / `RegExpSubstitute`
- [ ] `AverageSize`
- [x] `Disabled` / `Paused`
- [ ] `Group`

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
- [ ] `SolidColor2` / `GradientAngle`
- [ ] `BevelType` / `BevelColor` / `BevelColor2`
- [ ] `AntiAlias`
- [ ] `TransformationMatrix`
- [ ] `UpdateDivider`
- [ ] `Group`
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
- [ ] `Flip`
- [ ] `BarImage`
- [ ] `BarBorder`

## Image meter
- [x] `ImageName`, `ImagePath`, `ImageAlpha`, `W`/`H`
- [ ] `PreserveAspectRatio`
- [ ] `Tile`
- [ ] `ScaleMargins`
- [ ] `MaskImageName`
- [ ] Image modifiers: `ImageTint`, `ImageCrop`, `ImageRotate`, `ImageFlip`, `Greyscale`, `ColorMatrix`

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
