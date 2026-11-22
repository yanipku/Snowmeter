/*  Blur regions for [Rainmeter] Blur=1 / BlurRegion=... and the !AddBlur / !RemoveBlur bangs,
    as Rainmeter's Skin::ResizeBlur (Library/Skin.cpp). Linux compositors only take
    rectangles, so a region is kept as spans per pixel row: rounded corners and ellipses
    become stacks of rows 1 pixel tall. */

use crate::formula;
use crate::sectionvar::leading_int;
use std::collections::BTreeMap;

/*  Shapes are cut to 0..LIMIT on both sides: nothing outside the window can be blurred, and
    a window is never bigger (skin.rs MAX_WINDOW_SIDE). It also keeps a huge BlurRegion
    from using lots of memory. */
const LIMIT: i32 = 16384;

// Warning for the log, about a shape that isn't right
const MISSING_R: &str = "a rounded rectangle needs r. Blurred as rectangle here, \
    but Rainmeter for windows ignores the shape and blurs nothing there";
const ZERO_SIZE: &str = "w and h can't be 0, so the shape is ignored (as in Rainmeter)";
const UNKNOWN_TYPE: &str = "the type must be 1, 2 or 3, so the shape is ignored (as in Rainmeter)";

#[derive(Debug, PartialEq, Clone, Copy)]
enum Kind {
    Rect,
    Round(i32), // the size of the ellipse that rounds the corners
    Ellipse,
}

/*  One shape from BlurRegion= or a bang: "type, x, y, w, h[, r]".
    type 1 = rectangle, 2 = rounded rectangle, 3 = ellipse. */
#[derive(Debug, PartialEq)]
pub struct Shape {
    kind: Kind,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Shape {
    /*  One shape, and a warning for the log when somthing is wrong with it. the shape is None
        when Rainmeter ignore it: w or h is 0, or the type is unknown. Empty parts are skipped
        ("1,,0" = "1,0"), as wcstok does. */
    pub fn read(text: &str) -> (Option<Shape>, Option<&'static str>) {
        let mut parts = text.split(',').filter(|part| !part.is_empty()).map(int);
        let mut next = || parts.next().unwrap_or(0);
        let (kind, x, y, w, h) = (next(), next(), next(), next(), next());
        if w == 0 || h == 0 {
            return (None, Some(ZERO_SIZE));
        }
        let mut warning = None;
        let kind = match kind {
            1 => Kind::Rect,
            2 => match parts.next() {
                Some(r) => Kind::Round(r),
                None => {
                    //Our choice: a rectangle. Rainmeter ignores the shape.
                    warning= Some(MISSING_R);
                    Kind::Rect
                }
            },
            3 => Kind::Ellipse,
            _ => return (None, Some(UNKNOWN_TYPE)),
        };
        // A negative w or h counts back from x or y, as Windows' regions do
        let (x2, y2) = (x.saturating_add(w), y.saturating_add(h));
        (Some(Shape { kind, left: x.min(x2), top: y.min(y2), right: x.max(x2), bottom: y.max(y2) }), warning)
    }
    
    /*  The shape row by row: (y, start, end), with end not included. Each row is measured at
        its middle (y + 0.5), so a round edge lands on the nearest pixel. */
    fn rows(&self) -> impl Iterator<Item = (i32, i32, i32)> + '_ {
        let (l, r) = (f64::from(self.left), f64::from(self.right));
        let (t, b) = (f64::from(self.top), f64::from(self.bottom));
        (self.top.max(0)..self.bottom.min(LIMIT)).filter_map(move |y| {
            let cy = f64::from(y) + 0.5;
            let inset = match self.kind {
                Kind::Rect => 0.0,
                Kind::Round(size) => {
                    let size = f64::from(size).abs();
                    corner_inset(size.min(r - l) / 2.0, size.min(b - t) / 2.0, cy, t, b)
                }
                Kind::Ellipse => corner_inset((r - l) / 2.0, (b - t) / 2.0, cy, t, b),
            };
            let start = ((l + inset).round() as i32).max(0);
            let end = ((r - inset).round() as i32).min(LIMIT);
            (start < end).then_some((y, start, end))
        })
    }
}

/*  How far a round edge comes in from the side on the row whose middle is cy. The corners
    are quarter ellipses with radii rx and ry, between top t and bottom b. an ellipse is the
    case where the radii are half the shape. */
fn corner_inset(rx: f64, ry: f64, cy: f64, t: f64, b: f64) -> f64 {
    let dy = if cy < t + ry {
        t + ry - cy
    } else if cy > b - ry {
        cy - (b - ry)
    } else {
        return 0.0; // between the corners: straight sides
    };
    rx - rx * (1.0 - (dy / ry).powi(2)).max(0.0).sqrt()
}

/*  A number as Rainmeter's ParseInt reads it: a (formula), or the whole number at the start
    of the text ("10px" = 10). anything else, or a bad formula, is 0. */
fn int(text: &str) -> i32 {
    let text = text.trim_start();
    if text.starts_with('(') {
        formula::parse_number(text).map_or(0, |v| v as i32)
    } else {
        leading_int(text).unwrap_or(0)
    }
}

/*  A blur region: for each pixel row, the spans (start, end)that are blurred, sorted and
    never touching. Rows with nothing are left out. */
#[derive(Debug, Default)]
pub struct Region(BTreeMap<i32, Vec<(i32, i32)>>);

impl Region {
    // Rainmeter's RGN_OR: BlurRegion= and !AddBlur add the shape
    pub fn add(&mut self, shape: &Shape) {
        for (y, start, end) in shape.rows() {
            let spans = self.0.entry(y).or_default();
            spans.push((start, end));
            spans.sort_unstable();
            // Join the spans that overlap or touch
            let mut joined: Vec<(i32, i32)> = Vec::with_capacity(spans.len());
            for &(s, e) in spans.iter() {
                match joined.last_mut() {
                    Some(last) if s <= last.1 => last.1 = last.1.max(e),
                    _ => joined.push((s, e)),
                }
            }
            *spans = joined;
        }
    }

    //Rainemeter's RGN_DIFF: !RemoveBlur cuts the shape out
    pub fn remove(&mut self, shape: &Shape) {
        for (y, start, end) in shape.rows() {
            let Some(spans) = self.0.get_mut(&y) else { continue };
            // What is left of each span: the part before the cut and the part after it
            *spans =spans
                .iter()
                .flat_map(|&(s, e)| [(s, e.min(start)), (s.max(end), e)])
                .filter(|&(s, e)| s < e)
                .collect();
            if spans.is_empty() {
                self.0.remove(&y);
            }
        }
    }

    /*  The region as rectangles [x, y, w, h] for the compositor. Rows that follow each
        other with the same spans are joined into taller rectangles. */
    pub fn rects(&self) -> Vec<[i32; 4]> {
        let mut rects = Vec::new();
        let mut rows = self.0.iter().peekable();
        while let Some((&y, spans)) = rows.next() {
            let mut h = 1;
            while rows.next_if(|&(&next_y, next_spans)| next_y == y + h && next_spans == spans).is_some() {
                h += 1;
            }
            rects.extend(spans.iter().map(|&(start, end)| [start, y, end - start, h]));
        }
        rects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(shapes: &[&str]) -> Region {
        let mut region = Region::default();
        for text in shapes {
            region.add(&Shape::read(text).0.unwrap());
        }
        region
    }

    #[test]
    fn shapes_are_read_like_rainmeter() {
        let rect = |left, top, right, bottom| Some(Shape { kind: Kind::Rect, left, top, right, bottom });
        let shape = |text: &str| Shape::read(text).0;
        assert_eq!(shape("1, 10, 20, 30, 40"), rect(10, 20, 40, 60));
        assert_eq!(shape("1, 10, 20, -30, 40"), rect(-20, 20, 10, 60));
        assert_eq!(shape("1,,10,20px,30,(20*2)"), rect(10, 20, 40, 60));
        assert_eq!(shape("2, 0, 0, 10, 10, (2*3)").map(|s| s.kind), Some(Kind::Round(6)));
        assert_eq!(Shape::read("1, 0, 0, 10, 10").1, None); // a good shape has no warning
        assert_eq!(Shape::read("2, 0, 0, 10, 10"), (rect(0, 0, 10, 10), Some(MISSING_R))); // no r
        assert_eq!(Shape::read("1, 0, 0, 0, 10"), (None, Some(ZERO_SIZE))); // w = 0
        assert_eq!(Shape::read("4, 0, 0, 10, 10"), (None, Some(UNKNOWN_TYPE)));
        assert_eq!(Shape::read(""), (None, Some(ZERO_SIZE)));
    }

    #[test]
    fn regions_add_and_remove() {
        let mut blur = region(&["1, 0, 0, 10, 2", "1, 5, 0, 10, 2"]);
        assert_eq!(blur.rects(), [[0, 0, 15, 2]]); // the overlap is joined
        blur.remove(&Shape::read("1, 4, 1, 2, 1").0.unwrap());
        assert_eq!(blur.rects(), [[0, 0, 15, 1], [0, 1, 4, 1], [6, 1, 9, 1]]); // a hole
        blur.remove(&Shape::read("1, -5, -5, 100, 100").0.unwrap());
        assert!(blur.rects().is_empty());
    }

    #[test]
    fn round_shapes_become_rows() {
        assert_eq!(region(&["3, 0, 0, 4, 4"]).rects(), [[1, 0, 2, 1], [0, 1, 4, 2], [1, 3, 2, 1]]);
        assert_eq!(region(&["2, 0, 0, 10, 4, 4"]).rects(), [[1, 0, 8, 1], [0, 1, 10, 2], [1, 3, 8, 1]]);
        // Cut to the window's possible area
        assert_eq!(region(&["1, -10, -10, 20, 20"]).rects(), [[0, 0, 10, 10]]);
        assert_eq!(region(&["1, 0, 0, 1, 99999"]).rects(), [[0, 0, 1, LIMIT]]);
    }
}