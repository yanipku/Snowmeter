// The forms Rainmeter's WindowX= and WindowY= can take: 200, 50%, 20R (from the right),
// 30B (from the bottom), and @2 (on monitor 2), in any mix, such as 10%R@2.
// This file only does the arithmetic; monitors.rs says where the monitors are.
// AnchorX= and AnchorY= pick the point of the skin that is placed there (default: its
// top-left corner). Follows Rainmeter's SkinPosition.cpp.
use gtk4::gdk::Rectangle;

// A skin's position options, as written in the settings file
#[derive(Debug, Clone)]
pub struct Options {
    pub x: String,
    pub y: String,
    pub anchor_x: String,
    pub anchor_y: String,
}

// Rainmeter's default for each of them is 0
impl Default for Options {
    fn default() -> Options {
        Options { x: "0".into(), y: "0".into(), anchor_x: "0".into(), anchor_y: "0".into() }
    }
}

// AlwaysOnTop=: where the skin sits among other windows (Rainmeter's ZPOSITION)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZPos {
    OnDesktop, // -2: under every window, and still shown by "show desktop"
    Bottom,    // -1: under every window
    Normal,    // 0: among the other windows (the default)
    OnTop,     // 1: above normal windows
    Topmost,   // 2: above even other always-on-top windows
}

impl ZPos {
    // Read like Rainmeter's ReadInt (the whole number at the start); a value outside
    // -2..=2, or no number at all, is Normal
    pub fn parse(text: &str) -> ZPos {
        crate::sectionvar::leading_int(text).map_or(ZPos::Normal, ZPos::from_number)
    }

    // -2..=2 as in the settings file; any other number is Normal
    pub fn from_number(n: i32) -> ZPos {
        match n {
            -2 => ZPos::OnDesktop,
            -1 => ZPos::Bottom,
            1 => ZPos::OnTop,
            2 => ZPos::Topmost,
            _ => ZPos::Normal,
        }
    }

    // The number written for it in the settings file
    pub fn number(self) -> i32 {
        match self {
            ZPos::OnDesktop => -2,
            ZPos::Bottom => -1,
            ZPos::Normal => 0,
            ZPos::OnTop => 1,
            ZPos::Topmost => 2,
        }
    }
}

// One axis of a window position: WindowX or WindowY
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    value: f32,             // the number at the start
    percent: bool,          // `%`: value is a percentage of the monitor's width or height
    from_end: bool,         // `R` (X) or `B` (Y): counted from the right or bottom edge
    end_char: char,         // which letter that is for this axis
    pub monitor: Option<usize>, // `@N`; None = the primary monitor, Some(0) = all monitors
    anchor: i32,            // from AnchorX/AnchorY: pixels from the skin's left/top edge
}

impl Axis {
    // Reads an option such as "10%R@2". `end_char` is 'R' for WindowX and 'B' for WindowY.
    // `monitors` is how many monitors there are: an @N past that is ignored, as in Rainmeter.
    pub fn parse(text: &str, end_char: char, monitors: usize) -> Axis {
        // The number: the leading run of "-0123456789.", read like C's wtof
        // (as much of it as makes a number, else 0)
        let run_len = text.find(|c: char| !"-0123456789.".contains(c)).unwrap_or(text.len());
        let run = &text[..run_len];
        let value = (0..=run.len()).rev().find_map(|end| run[..end].parse::<f32>().ok()).unwrap_or(0.0);

        // A modifier counts only after the last '#', so one inside a variable name doesn't
        let after_hash = |pos: Option<usize>| match (pos, text.rfind('#')) {
            (Some(p), Some(h)) => p > h,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let percent = after_hash(text.rfind('%'));
        let from_end = after_hash(text.rfind(end_char));

        let at = text.rfind('@');
        let monitor = if after_hash(at) {
            let digits: String = text[at.unwrap() + 1..].chars().take_while(char::is_ascii_digit).collect();
            digits.parse::<usize>().ok().filter(|&n| n <= monitors)
        } else {
            None
        };
        Axis { value, percent, from_end, end_char, monitor, anchor: 0 }
    }

    // Reads AnchorX/AnchorY for a skin `size` pixels wide (or tall): 30 (pixels), 50%
    // (of the skin), 10R / 10B (from the skin's right or bottom edge). Unlike WindowX,
    // there is no minus sign and no '#' rule, as in Rainmeter's ParseAnchorOption.
    pub fn set_anchor(&mut self, text: &str, size: i32) {
        let run_len = text.find(|c: char| !"0123456789.".contains(c)).unwrap_or(text.len());
        let run = &text[..run_len];
        let mut n = (0..=run.len()).rev().find_map(|end| run[..end].parse::<f32>().ok()).unwrap_or(0.0);
        if text.contains('%') {
            n = size as f32 * (n / 100.0);
        }
        if text.contains(self.end_char) {
            n = size as f32 - n;
        }
        self.anchor = n as i32;
    }

    // The window's position in pixels, on a monitor that starts at `start` and is `len` wide
    // (or tall, for Y). `as i32` drops the fraction, like the (int) cast in Rainmeter.
    // The anchor point lands there, so the window's edge is `anchor` pixels before it.
    pub fn resolve(&self, start: i32, len: i32) -> i32 {
        let offset = if self.percent { (len as f32 * (self.value / 100.0)) as i32 } else { self.value as i32 };
        start + if self.from_end { len - offset } else { offset } - self.anchor
    }

    // The option text for a window at `pos`, written in this axis's form:
    // the reverse of resolve(). Percentages get 5 decimals, as in Rainmeter.
    pub fn to_option(&self, pos: i32, start: i32, len: i32) -> String {
        let offset = if self.from_end { start + len - pos - self.anchor } else { pos - start + self.anchor };
        let mut text = if self.percent {
            format!("{:.5}%", 100.0 * offset as f32 / len as f32)
        } else {
            offset.to_string()
        };
        if self.from_end {
            text.push(self.end_char);
        }
        if let Some(n) = self.monitor {
            text.push_str(&format!("@{n}"));
        }
        text
    }
}

// Where the window's top-left corner goes, for a skin `size` (width, height) big.
// `rects` is monitors::monitor_rects(): [0] all monitors, [1] the primary, then the others.
pub fn resolve(opts: &Options, size: (i32, i32), rects: &[Rectangle]) -> (i32, i32) {
    let ((ax, rx), (ay, ry)) = parse_both(opts, size, rects);
    (ax.resolve(rx.x(), rx.width()), ay.resolve(ry.y(), ry.height()))
}

// The reverse of resolve(): the WindowX= and WindowY= texts for a window now at `pos`,
// written in the form of the old ones, on the same monitor
pub fn to_options(opts: &Options, pos: (i32, i32), size: (i32, i32), rects: &[Rectangle]) -> (String, String) {
    let ((ax, rx), (ay, ry)) = parse_both(opts, size, rects);
    (ax.to_option(pos.0, rx.x(), rx.width()), ay.to_option(pos.1, ry.y(), ry.height()))
}

// Parses the options, and pairs each axis with its monitor's rectangle: the @N one, else
// the primary; a zero-size one if GTK gave us no monitors
fn parse_both(opts: &Options, size: (i32, i32), rects: &[Rectangle]) -> ((Axis, Rectangle), (Axis, Rectangle)) {
    let count = rects.len().saturating_sub(1);
    let mut ax = Axis::parse(&opts.x, 'R', count);
    let mut ay = Axis::parse(&opts.y, 'B', count);
    ax.set_anchor(&opts.anchor_x, size.0);
    ay.set_anchor(&opts.anchor_y, size.1);
    share_monitor(&mut ax, &mut ay);
    let rect = |a: &Axis| rects.get(a.monitor.unwrap_or(1)).copied().unwrap_or(Rectangle::new(0, 0, 0, 0));
    let (rx, ry) = (rect(&ax), rect(&ay));
    ((ax, rx), (ay, ry))
}

// KeepOnScreen: where a window `size` big at `pos` must go to be fully on a monitor.
// `monitors` are the monitors only (monitor_rects() without [0]), the primary first.
// As in Rainmeter's ClampPositionToScreenBounds: the monitor that holds the window's
// centre, else one of its corners (top-left, bottom-right, bottom-left, top-right);
// a window on none of them is pushed onto the primary.
pub fn keep_on_screen(pos: (i32, i32), size: (i32, i32), monitors: &[Rectangle]) -> (i32, i32) {
    let ((x, y), (w, h)) = (pos, size);
    let points = [(x + w / 2, y + h / 2), (x, y), (x + w, y + h), (x, y + h), (x + w, y)];
    let inside = |r: &Rectangle, (px, py): (i32, i32)| {
        px >= r.x() && px < r.x() + r.width() && py >= r.y() && py < r.y() + r.height()
    };
    let found = points.iter().find_map(|&p| monitors.iter().find(|r| inside(r, p)));
    let Some(r) = found.or(monitors.first()) else { return pos };
    // min then max, as in Rainmeter: a window bigger than the monitor keeps its left/top on it
    let nx = x.min(r.x() + r.width() - w).max(r.x());
    let ny = y.min(r.y() + r.height() - h).max(r.y());
    (nx, ny)
}

// How close, in pixels, an edge must come before SnapEdges lines it up (Rainmeter's g_SnapDistance)
const SNAP_DISTANCE: i32 = 10;

// SnapEdges: where a window `size` big that is being moved to `pos` snaps to. It lines up
// with the edges of the `others` (the other skins' rectangles), then with the edges of the
// monitor it overlaps most. Follows Rainmeter's OnWindowPosChanging and SnapToWindow.
pub fn snap(pos: (i32, i32), size: (i32, i32), others: &[Rectangle], monitors: &[Rectangle]) -> (i32, i32) {
    let ((mut x, mut y), (w, h)) = (pos, size);
    // Within SNAP_DISTANCE of `edge`, either side (not counting exactly that far)
    let near = |a: i32, edge: i32| a < edge + SNAP_DISTANCE && a > edge - SNAP_DISTANCE;

    for o in others {
        let (ox, oy, ow, oh) = (o.x(), o.y(), o.width(), o.height());
        // Side by side (they overlap from top to bottom): line up left and right edges
        if y < oy + oh && y + h > oy {
            if near(x, ox) { x = ox; }
            if near(x, ox + ow) { x = ox + ow; }
            if near(x + w, ox) { x = ox - w; }
            if near(x + w, ox + ow) { x = ox + ow - w; }
        }
        // One above the other: line up top and bottom edges
        if x < ox + ow && x + w > ox {
            if near(y, oy) { y = oy; }
            if near(y, oy + oh) { y = oy + oh; }
            if near(y + h, oy) { y = oy - h; }
            if near(y + h, oy + oh) { y = oy + oh - h; }
        }
    }

    // The monitor with the most of the window on it (a 0-size window counts as 1 pixel)
    let me = Rectangle::new(x, y, w.max(1), h.max(1));
    let area = |r: &Rectangle| me.intersect(r).map_or(0, |i| i64::from(i.width()) * i64::from(i.height()));
    let best = monitors.iter().filter(|r| area(r) > 0).max_by_key(|r| area(r));
    if let Some(r) = best {
        let (right, bottom) = (r.x() + r.width() - w, r.y() + r.height() - h);
        if near(x, r.x()) { x = r.x(); }
        if near(y, r.y()) { y = r.y(); }
        if near(x, right) { x = right; }
        if near(y, bottom) { y = bottom; }
    }
    (x, y)
}

// If only one of X and Y names a monitor, the other uses it too
pub fn share_monitor(x: &mut Axis, y: &mut Axis) {
    match (x.monitor, y.monitor) {
        (Some(n), None) => y.monitor = Some(n),
        (None, Some(n)) => x.monitor = Some(n),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn x(text: &str) -> Axis {
        Axis::parse(text, 'R', 2)
    }

    #[test]
    fn plain_numbers() {
        assert_eq!(x("200").resolve(0, 1920), 200);
        assert_eq!(x("-35").resolve(0, 1920), -35);
        assert_eq!(x("12.9").resolve(0, 1920), 12); // the fraction is dropped
        assert_eq!(x("").resolve(0, 1920), 0);
        assert_eq!(x("1-2").resolve(0, 1920), 1); // like wtof: as much as makes a number
        assert_eq!(x("200").resolve(1920, 1280), 2120); // counts from the monitor's left
    }

    #[test]
    fn percent_and_from_end() {
        assert_eq!(x("50%").resolve(0, 1920), 960);
        assert_eq!(x("20R").resolve(0, 1920), 1900);
        assert_eq!(x("10%R").resolve(0, 1920), 1728);
        assert_eq!(x("20r").resolve(0, 1920), 20); // the letter is case-sensitive
        assert_eq!(Axis::parse("30B", 'B', 1).resolve(0, 1080), 1050);
        assert_eq!(Axis::parse("30R", 'B', 1).resolve(0, 1080), 30); // R means nothing for Y
    }

    #[test]
    fn monitors() {
        assert_eq!(x("5@2").monitor, Some(2));
        assert_eq!(x("5@0").monitor, Some(0));
        assert_eq!(x("5@3").monitor, None); // only 2 monitors
        assert_eq!(x("5@").monitor, None);
        assert_eq!(x("5").monitor, None);

        let mut a = x("5@2");
        let mut b = Axis::parse("7", 'B', 2);
        share_monitor(&mut a, &mut b);
        assert_eq!(b.monitor, Some(2));
    }

    #[test]
    fn modifiers_only_after_the_last_hash() {
        assert!(!x("#Var%#").percent);
        assert!(x("#Var#%").percent);
        assert!(!x("#R#").from_end);
    }

    #[test]
    fn resolve_picks_the_monitor() {
        // A 1920x1080 primary, and a 1280x1024 monitor to its left
        let rects = [
            Rectangle::new(-1280, 0, 3200, 1080), // @0: all monitors
            Rectangle::new(0, 0, 1920, 1080),     // @1: primary
            Rectangle::new(-1280, 0, 1280, 1024), // @2
        ];
        let at = |x: &str, y: &str| resolve(&opts(x, y), (100, 50), &rects);
        assert_eq!(at("200", "100"), (200, 100));
        assert_eq!(at("20R", "30B"), (1900, 1050));
        assert_eq!(at("50%@2", "50%"), (-640, 512)); // Y uses @2 too
        assert_eq!(at("0@0", "0"), (-1280, 0));
        assert_eq!(at("5@9", "7"), (5, 7)); // no monitor 9: the primary
        assert_eq!(resolve(&opts("200", "100"), (100, 50), &[]), (200, 100)); // no monitors known

        // Saving after a move: same form, same monitor
        let save = |x: &str, y: &str, pos| to_options(&opts(x, y), pos, (100, 50), &rects);
        assert_eq!(save("20R", "30B", (1800, 1000)), ("120R".into(), "80B".into()));
        assert_eq!(save("50%@2", "0", (-960, 256)), ("25.00000%@2".into(), "256@2".into()));
    }

    #[test]
    fn keeping_on_screen() {
        // A 1920x1080 primary, and a 1280x1024 monitor to its left
        let mons = [Rectangle::new(0, 0, 1920, 1080), Rectangle::new(-1280, 0, 1280, 1024)];
        let keep = |x, y| keep_on_screen((x, y), (200, 100), &mons);
        assert_eq!(keep(100, 100), (100, 100)); // already on screen
        assert_eq!(keep(1800, 1050), (1720, 980)); // past the bottom-right: pushed back
        assert_eq!(keep(100, -50), (100, 0)); // past the top
        assert_eq!(keep(-150, 100), (-200, 100)); // centre on the left monitor: stays there
        assert_eq!(keep(-50, 100), (0, 100)); // centre on the primary: onto the primary
        assert_eq!(keep(-1250, 1000), (-1250, 924)); // left monitor is shorter
        assert_eq!(keep(5000, 5000), (1720, 980)); // nowhere: onto the primary
        // Bigger than the monitor: the top-left stays on it
        assert_eq!(keep_on_screen((50, 50), (3000, 2000), &mons), (0, 0));
        assert_eq!(keep_on_screen((7, 8), (10, 10), &[]), (7, 8)); // no monitors known
    }

    #[test]
    fn snapping() {
        let mons = [Rectangle::new(0, 0, 1920, 1080)];
        let size = (200, 100);
        let alone = |x, y| snap((x, y), size, &[], &mons);
        assert_eq!(alone(500, 500), (500, 500)); // far from everything
        assert_eq!(alone(7, 500), (0, 500)); // near the left edge
        assert_eq!(alone(-9, 500), (0, 500)); // just past it
        assert_eq!(alone(10, 500), (10, 500)); // exactly 10 away: no snap
        assert_eq!(alone(1715, 975), (1720, 980)); // near the bottom-right corner

        // Another skin at (600, 400), 300x150
        let other = [Rectangle::new(600, 400, 300, 150)];
        let next_to = |x, y| snap((x, y), size, &other, &mons);
        assert_eq!(next_to(905, 420), (900, 420)); // our left edge to its right edge
        assert_eq!(next_to(395, 420), (400, 420)); // our right edge to its left edge
        assert_eq!(next_to(605, 545), (600, 550)); // under it: tops and lefts line up
        assert_eq!(next_to(905, 700), (905, 700)); // not side by side: no snap
    }

    #[test]
    fn z_positions() {
        assert_eq!(ZPos::parse("-2"), ZPos::OnDesktop);
        assert_eq!(ZPos::parse(" 1 "), ZPos::OnTop);
        assert_eq!(ZPos::parse("2"), ZPos::Topmost);
        assert_eq!(ZPos::parse("3"), ZPos::Normal);
        assert_eq!(ZPos::parse(""), ZPos::Normal);
    }

    fn opts(x: &str, y: &str) -> Options {
        Options { x: x.into(), y: y.into(), ..Options::default() }
    }

    #[test]
    fn anchors() {
        let anchored = |ax: &str| {
            let mut a = x("0");
            a.set_anchor(ax, 200);
            a.anchor
        };
        assert_eq!(anchored("0"), 0);
        assert_eq!(anchored("30"), 30);
        assert_eq!(anchored("50%"), 100);
        assert_eq!(anchored("100%"), 200);
        assert_eq!(anchored("10R"), 190);
        assert_eq!(anchored("-30"), 0); // no minus sign in anchors

        // A 200x100 skin whose bottom-right corner sits 20 px from the primary's corner
        let rects = [Rectangle::new(0, 0, 1920, 1080), Rectangle::new(0, 0, 1920, 1080)];
        let o = Options { x: "20R".into(), y: "20B".into(), anchor_x: "100%".into(), anchor_y: "0B".into() };
        assert_eq!(resolve(&o, (200, 100), &rects), (1700, 960));
        // and centred on the screen
        let c = Options { x: "50%".into(), y: "50%".into(), anchor_x: "50%".into(), anchor_y: "50%".into() };
        assert_eq!(resolve(&c, (200, 100), &rects), (860, 490));
        // Saving takes the anchor back off
        assert_eq!(to_options(&o, (1600, 900), (200, 100), &rects), ("120R".into(), "80B".into()));
        assert_eq!(to_options(&c, (860, 490), (200, 100), &rects), ("50.00000%".into(), "50.00000%".into()));
    }

    #[test]
    fn saving_keeps_the_form() {
        assert_eq!(x("200").to_option(350, 0, 1920), "350");
        assert_eq!(x("20R").to_option(1820, 0, 1920), "100R");
        assert_eq!(x("50%").to_option(480, 0, 1920), "25.00000%");
        assert_eq!(x("10%R@2").to_option(1920 + 1152, 1920, 1280), "10.00000%R@2");
        // Round trip: what is saved resolves back to the same place
        let a = x("33%R");
        let saved = a.to_option(1234, 0, 1920);
        assert_eq!(x(&saved).resolve(0, 1920), 1234);
    }
}
