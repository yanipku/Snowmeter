# Roadmap

This list compares our engine with Rainmeter. The reference is the official source in `../rainmeter_code` (`Library/`).
Tick an item `[x]` when it's done. 🟡 in the text means the item is done in part.

## Progress compared with Rainmeter for Windows

- Overall estimate: **10–15%**
- Roadmap checklist: 41 / 145 ≈ **28%**

## Next up
1. [x] Transparent, borderless, draggable window
2. [x] String meter: `Prefix`/`Postfix`, `StringCase`
3. [ ] Mouse actions plus the first bangs (`!SetVariable`, `!ShowMeter`/`!HideMeter`, `!Refresh`)
4. [ ] `LocalFont` (skin fonts loaded for our program only, with fontconfig's `FcConfigAppFontAddFile`)
5. [ ] More built-in variables: `#CURRENTCONFIG#`, `#SCREENAREAWIDTH#`/`HEIGHT#`, `#WORKAREA…#`, `#PROGRAMPATH#`, `#SETTINGSPATH#`
6. [ ] Faster style lookup with a HashMap
7. [ ] More section variables: `[Measure:%]`, `[Measure:/1000,2]`, `[&Measure:EscapeRegExp]`, and the like
8. [ ] Load, unload and refresh skins (`!Refresh` needs this; everything below builds on it)
9. [ ] Several skins at once (one process, one window per skin)
10. [ ] Remember each skin's position and settings (our own Rainmeter.ini; Wayland limits window placement)
11. [ ] Right-click menu
12. [ ] Always on top / on the desktop / normal (Wayland needs compositor support)
13. [ ] Click-through, Draggable, SnapEdges, KeepOnScreen
14. [ ] The rest of the core bangs: `!SetVariable`, `!SetOption`, `!Update`/`!Redraw`, `!EnableMeasure`/`!DisableMeasure`/`!PauseMeasure`, `!Move`, `!ActivateConfig`/`!DeactivateConfig`
15. [ ] `[Rainmeter]` actions: `OnRefreshAction`, `OnUpdateAction`, `OnCloseAction`, `OnFocusAction`/`OnUnfocusAction`
16. [ ] Running programs from actions (safely: launched directly, never through a shell)
17. [ ] Tray icon (GNOME hides tray icons without an extension)
18. [ ] Manage window: a GUI for Snowmeter outside the terminal, like Rainmeter's Manage dialog (see "Manage window" below)
19. [ ] SkinInstaller: install `.rmskin` packages (see "Skin installer" below)

The core comes first: new meters and measures wait until items 1–19 are done.

---

## Skin files and variables
- [x] INI parsing, case-insensitive (UTF-8, UTF-16 and Windows-1252; 4 MB cap)
- [x] `@Include` (with checks for loops, nesting depth and files outside the skin folder)
- [x] `[Variables]`, including variables inside variables
- [x] Nested variables `[#Var]` (in `[Variables]`, meter and measure options, and mouse actions)
- [x] Built-in variables: `#@#`, `#CURRENTPATH#`, `#ROOTCONFIGPATH#`, `#SKINSPATH#`, `#CURRENTFILE#`, `#CURRENTSECTION#`
- [ ] More built-in variables: `#CURRENTCONFIG#`, `#SCREENAREAWIDTH#`/`HEIGHT#`, `#WORKAREA…#`, `#PROGRAMPATH#`, `#SETTINGSPATH#`
- [x] `MeterStyle` (chained and multiple styles)
- [ ] Faster style lookup with a HashMap
- [x] Section variables: `[Measure]`, `[Measure:MaxValue]`, `[Meter:X/Y/W/H/XW/YH]`
- [ ] More section variables: `[Measure:%]`, `[Measure:/1000,2]`, `[&Measure:EscapeRegExp]`, and the like
- [x] `DynamicVariables=1` (meters and measures)
- [x] The formula language (operators, functions, `pi`/`e`, `0b`/`0o`/hex numbers)

## Window and app
- [x] Command-line launcher (`--list`, `--skins-dir`, `--set-skins-dir`)
- [x] GTK4 window that redraws on the `Update` timer
- [x] Transparent window with no border
- [x] Drag the skin around
- [ ] Remember each skin's position and settings (Rainmeter.ini)
- [ ] Several skins at once
- [ ] Load, unload and refresh skins
- [ ] Right-click menu
- [ ] Tray icon
- [ ] Always on top, on the desktop, normal
- [ ] Click-through, Draggable, SnapEdges, KeepOnScreen

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
- [ ] `LocalFont`
- [ ] `AccurateText`
- [ ] `Blur` / `BlurRegion` (needs compositor support on Linux)

## Interaction
- [x] Mouse actions: `LeftMouseUp/Down/DoubleClickAction` (and the Right/Middle versions)
- [ ] `MouseOverAction` / `MouseLeaveAction`
- [ ] Scroll actions (`MouseScrollUpAction`, …)
- [ ] `ToolTipText`, `ToolTipTitle`, `ToolTipIcon`, `ToolTipWidth`, `ToolTipType`, `ToolTipHidden`
- [x] Bangs: `!SetVariable` (formula values; `[#Var]` in actions is read at click time)
- [ ] Bangs: `!WriteKeyValue`
- [x] Bangs: `!ShowMeter`, `!HideMeter`, `!ToggleMeter` (take effect at the next update, as in Rainmeter)
- [ ] Bangs: `!ShowMeterGroup`, `!HideMeterGroup`, `!ToggleMeterGroup`
- [ ] Bangs: `!SetOption`, `!SetOptionGroup`
- [x] Bangs: `!Update`, `!Redraw` (`!Update` doesn't restart the Update timer yet)
- [ ] Bangs: `!UpdateMeter`, `!UpdateMeasure`, `!Refresh`
- [ ] Bangs: `!EnableMeasure`, `!DisableMeasure`, `!PauseMeasure`, `!CommandMeasure`
- [ ] Bangs: `!Move`, `!ActivateConfig`, `!DeactivateConfig`
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
- [ ] `Disabled` / `Paused`
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
