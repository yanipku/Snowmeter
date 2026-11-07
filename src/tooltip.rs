/*  What a meter's tooltip shows, laid out like a windows tooltip:
    the icon and the bold title on the first line, the text below. */
use crate::meter::{TipIcon, TipShow};
use gtk4::prelude::*;
use gtk4::{gdk, glib, pango, Align, Image, Label, Orientation, Widget};

pub fn build(tip: &TipShow, widget: &impl IsA<Widget>) -> Widget {
    let column = gtk4::Box::new(Orientation::Vertical, 4);
    // tip.icon is only set when there's a title (see meter::tooltip_at)
    if !tip.title.is_empty() {
        let row = gtk4::Box::new(Orientation::Horizontal, 6);
        let icon = match &tip.icon {
            Some(TipIcon::Name(name)) => {
                let image = Image::from_icon_name(name);
                image.add_css_class(&format!("snow-{name}")); // for its color, see manager.rs
                Some(image)
            }
            Some(TipIcon::File(pixbuf)) => Some(Image::from_paintable(Some(&gdk::Texture::for_pixbuf(pixbuf)))),
            None => None,
        };
        if let Some(image) = icon {
            image.set_pixel_size(16); //Windows draws a title's icon small
            row.append(&image);
        }
        // Markup for the bold; the skin's text is escaped, so a < or & in it shows as written
        let title = Label::new(None);
        title.set_markup(&format!("<b>{}</b>", glib::markup_escape_text(&tip.title)));
        row.append(&title);
        column.append(&row);
    }
    let text = Label::new(Some(&wrap(&tip.text, tip.width, widget)));
    text.set_halign(Align::Start);  // the label sits at the left of the tooltip
    text.set_xalign(0.0);           // and its lines are left-aligned inside it
    column.append(&text);
    column.upcast()
}

/*  Breaks 'text' into lines no wider than 'width' pixels, at spaces where it can, as a Windows
    tooltip does with ToolTipWidth (TTM_SETMAXTIPWIDTH). Pango, GTK's text engine, finds the
    breaks in the font the widget uses: we put a newline at each, so the label needs no width
    limit of its own, 0 or less: no limit. */
fn wrap(text: &str, width: f64, widget: &impl IsA<Widget>) -> String {
    let layout = widget.create_pango_layout(Some(text));
    if width > 0.0 {
        layout.set_width((width * pango::SCALE as f64) as i32);
    }
    layout.set_wrap(pango::WrapMode::WordChar);
    layout
        .lines_readonly()
        .iter()
        .map(|line| {
            let start = line.start_index() as usize;
            let end = start + line.length() as usize;
            text.get(start..end).unwrap_or("").trim_end()
        })
        .collect::<Vec<_>>()
        .join("\n")
}