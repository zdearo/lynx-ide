//! Draws what shows an agent is in control, in software, for the views that blit the page's frame
//! themselves: the frame around the page, the cursor with the agent's mark at its corner, the
//! label of what it does, and the ring while a page loads. Nothing here touches the platform: it
//! paints into a transparent layer that the view lays over the page's pixels.
//!
//! The look is the one `native_view.rs` draws with cairo on Linux; the two agree by hand.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use tiny_skia::{Color, FillRule, LineCap, Paint, Path, PathBuilder, Pixmap, Stroke, Transform};

use super::agent_icon::{self, Op};
use super::agent_pointer::{CursorShape, PointerView};

/// The agent's colour: the `--agent` token of the theme, converted by hand.
pub const AGENT_RGB: (f64, f64, f64) = (0.958, 0.592, 0.323);
/// The cursor is drawn this much larger than a system pointer, so it is easy to follow.
const CURSOR_SCALE: f64 = 1.3;
const BADGE_RADIUS: f64 = 9.0;
const LABEL_HEIGHT: f64 = 22.0;
const LABEL_FONT_PX: f32 = 12.0;
/// The longest label drawn, in characters; the rest is cut with an ellipsis.
pub const MAX_LABEL_CHARS: usize = 38;
const DARK_TEXT: (f64, f64, f64) = (0.1, 0.07, 0.04);

/// The font labels are set in, read from the system when the view is made. Without one the labels
/// are left out; everything else still draws.
pub struct OverlayFont {
    font: fontdue::Font,
}

impl OverlayFont {
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
            .ok()
            .map(|font| Self { font })
    }

    /// A bold sans-serif font the system already has.
    pub fn load_system() -> Option<Self> {
        const CANDIDATES: &[&str] = &[
            r"C:\Windows\Fonts\segoeuib.ttf",
            r"C:\Windows\Fonts\arialbd.ttf",
            "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
            "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/liberation/LiberationSans-Bold.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        ];
        CANDIDATES
            .iter()
            .find_map(|path| std::fs::read(path).ok())
            .and_then(|bytes| Self::from_bytes(&bytes))
    }

    fn width(&self, text: &str, px: f32) -> f64 {
        text.chars()
            .map(|c| f64::from(self.font.metrics(c, px).advance_width))
            .sum()
    }
}

/// The layer to lay over the page: `size` is the view in logical pixels, `scale` the screen's
/// density, so the layer is `size * scale` pixels. None when nothing is to be drawn.
pub fn render(
    view: &PointerView,
    size: (f64, f64),
    scale: f64,
    seconds: f64,
    font: Option<&OverlayFont>,
) -> Option<Pixmap> {
    let width = (size.0 * scale).round().max(1.0) as u32;
    let height = (size.1 * scale).round().max(1.0) as u32;
    let mut layer = Pixmap::new(width, height)?;
    let mut painter = Painter {
        layer: &mut layer,
        scale,
        strength: view.strength,
    };

    painter.border(size);
    let Some((x, y)) = view.at else {
        return Some(layer);
    };
    if let Some(progress) = view.ripple {
        painter.ripple((x, y), progress);
    }
    let (corner, middle) = painter.cursor((x, y), view.cursor);

    let mut next_to = corner.0 + 6.0;
    let mut anchor_y = middle;
    if let Some(mark) = agent_icon::mark(&view.harness) {
        painter.badge(corner, mark);
        next_to = corner.0 + BADGE_RADIUS + 6.0;
        anchor_y = corner.1;
    }

    // What the agent does goes beside the mark, and flips inside the page near its right edge.
    let top = (anchor_y - LABEL_HEIGHT / 2.0).clamp(4.0, (size.1 - LABEL_HEIGHT - 4.0).max(4.0));
    if view.loading {
        let radius = LABEL_HEIGHT / 2.0;
        let left = if next_to + 2.0 * radius > size.0 {
            x - 8.0 - 2.0 * radius
        } else {
            next_to
        };
        painter.spinner((left + radius, top + radius), radius, seconds);
    } else if !view.label.is_empty()
        && let Some(font) = font
    {
        painter.label(font, &clipped(&view.label), (size.0, next_to, x, top));
    }
    Some(layer)
}

/// Lays `layer` over `frame`, both BGRA, premultiplied, of the same size.
pub fn composite_over_bgra(frame: &mut [u8], layer: &Pixmap) {
    let (targets, _) = frame.as_chunks_mut::<4>();
    let (sources, _) = layer.data().as_chunks::<4>();
    for (target, source) in targets.iter_mut().zip(sources) {
        let alpha = u32::from(source[3]);
        if alpha == 0 {
            continue;
        }
        let inverse = 255 - alpha;
        // The layer is RGBA and the frame BGRA; both premultiplied.
        let blend = |over: u8, under: u8| {
            (u32::from(over) + (u32::from(under) * inverse + 127) / 255).min(255) as u8
        };
        target[0] = blend(source[2], target[0]);
        target[1] = blend(source[1], target[1]);
        target[2] = blend(source[0], target[2]);
        target[3] = blend(source[3], target[3]);
    }
}

pub fn clipped(label: &str) -> String {
    if label.chars().count() <= MAX_LABEL_CHARS {
        return label.to_owned();
    }
    let cut: String = label.chars().take(MAX_LABEL_CHARS - 1).collect();
    format!("{cut}…")
}

struct Painter<'a> {
    layer: &'a mut Pixmap,
    scale: f64,
    strength: f64,
}

impl Painter<'_> {
    fn transform(&self) -> Transform {
        Transform::from_scale(self.scale as f32, self.scale as f32)
    }

    fn paint(&self, (red, green, blue): (f64, f64, f64), alpha: f64) -> Paint<'static> {
        let mut paint = Paint::default();
        paint.set_color(
            Color::from_rgba(
                red as f32,
                green as f32,
                blue as f32,
                (alpha * self.strength).clamp(0.0, 1.0) as f32,
            )
            .unwrap_or(Color::TRANSPARENT),
        );
        paint.anti_alias = true;
        paint
    }

    fn fill(&mut self, path: &Path, colour: (f64, f64, f64), alpha: f64) {
        let paint = self.paint(colour, alpha);
        let transform = self.transform();
        self.layer
            .fill_path(path, &paint, FillRule::Winding, transform, None);
    }

    fn stroke(
        &mut self,
        path: &Path,
        colour: (f64, f64, f64),
        alpha: f64,
        width: f64,
        round: bool,
    ) {
        let paint = self.paint(colour, alpha);
        let stroke = Stroke {
            width: width as f32,
            line_cap: if round { LineCap::Round } else { LineCap::Butt },
            ..Stroke::default()
        };
        let transform = self.transform();
        self.layer
            .stroke_path(path, &paint, &stroke, transform, None);
    }

    /// The frame around the page while an agent is in control.
    fn border(&mut self, (width, height): (f64, f64)) {
        let mut builder = PathBuilder::new();
        builder.move_to(1.5, 1.5);
        builder.line_to((width - 1.5) as f32, 1.5);
        builder.line_to((width - 1.5) as f32, (height - 1.5) as f32);
        builder.line_to(1.5, (height - 1.5) as f32);
        builder.close();
        if let Some(path) = builder.finish() {
            self.stroke(&path, AGENT_RGB, 0.9, 3.0, false);
        }
    }

    fn ripple(&mut self, (x, y): (f64, f64), progress: f64) {
        if let Some(path) = circle(x, y, 6.0 + 22.0 * progress) {
            self.stroke(&path, AGENT_RGB, 0.55 * (1.0 - progress), 2.5, false);
        }
    }

    /// The arrow (tip at the point) or the text cursor (centred on it). Returns where the agent's
    /// mark goes, at the cursor's lower right, and the height of the cursor's middle.
    fn cursor(&mut self, (x, y): (f64, f64), shape: CursorShape) -> ((f64, f64), f64) {
        let scale = CURSOR_SCALE;
        match shape {
            CursorShape::Arrow => {
                let mut builder = PathBuilder::new();
                for (index, (dx, dy)) in [
                    (0.0, 0.0),
                    (0.0, 17.0),
                    (4.5, 13.0),
                    (7.5, 20.0),
                    (10.0, 19.0),
                    (7.0, 12.0),
                    (12.5, 12.0),
                ]
                .into_iter()
                .enumerate()
                {
                    let point = ((x + dx * scale) as f32, (y + dy * scale) as f32);
                    if index == 0 {
                        builder.move_to(point.0, point.1);
                    } else {
                        builder.line_to(point.0, point.1);
                    }
                }
                builder.close();
                if let Some(path) = builder.finish() {
                    self.fill(&path, (1.0, 1.0, 1.0), 1.0);
                    self.stroke(&path, (0.05, 0.05, 0.08), 1.0, 1.8, false);
                }
                (
                    (x + 12.5 * scale + 1.5, y + 20.0 * scale - 2.0),
                    y + 14.0 * scale,
                )
            }
            CursorShape::IBeam => {
                let (half_height, half_width) = (10.0 * scale, 4.0 * scale);
                let mut builder = PathBuilder::new();
                let mut segment = |from: (f64, f64), to: (f64, f64)| {
                    builder.move_to(from.0 as f32, from.1 as f32);
                    builder.line_to(to.0 as f32, to.1 as f32);
                };
                segment(
                    (x - half_width, y - half_height),
                    (x + half_width, y - half_height),
                );
                segment((x, y - half_height), (x, y + half_height));
                segment(
                    (x - half_width, y + half_height),
                    (x + half_width, y + half_height),
                );
                if let Some(path) = builder.finish() {
                    // A light outline under a dark line, so it shows on any page.
                    self.stroke(&path, (1.0, 1.0, 1.0), 1.0, 4.2, true);
                    self.stroke(&path, (0.05, 0.05, 0.08), 1.0, 1.8, true);
                }
                ((x + half_width + 9.0, y + half_height), y)
            }
        }
    }

    /// The agent's mark on a dark disc.
    fn badge(&mut self, (cx, cy): (f64, f64), mark: &agent_icon::Mark) {
        if let Some(disc) = circle(cx, cy, BADGE_RADIUS) {
            self.fill(&disc, (0.09, 0.09, 0.11), 0.95);
            self.stroke(&disc, (1.0, 1.0, 1.0), 0.45, 1.0, false);
        }
        let size = 2.0 * BADGE_RADIUS - 6.0;
        let fit = size / mark.width.max(mark.height);
        let origin = (cx - mark.width * fit / 2.0, cy - mark.height * fit / 2.0);
        for shape in &mark.shapes {
            let mut builder = PathBuilder::new();
            for op in &shape.ops {
                let at =
                    |x: f64, y: f64| ((origin.0 + x * fit) as f32, (origin.1 + y * fit) as f32);
                match *op {
                    Op::Move(x, y) => {
                        let (x, y) = at(x, y);
                        builder.move_to(x, y);
                    }
                    Op::Line(x, y) => {
                        let (x, y) = at(x, y);
                        builder.line_to(x, y);
                    }
                    Op::Curve(x1, y1, x2, y2, x, y) => {
                        let (a, b) = at(x1, y1);
                        let (c, d) = at(x2, y2);
                        let (e, f) = at(x, y);
                        builder.cubic_to(a, b, c, d, e, f);
                    }
                    Op::Close => builder.close(),
                }
            }
            if let Some(path) = builder.finish() {
                self.fill(&path, shape.rgb, 1.0);
            }
        }
    }

    fn spinner(&mut self, centre: (f64, f64), radius: f64, seconds: f64) {
        if let Some(disc) = circle(centre.0, centre.1, radius) {
            self.fill(&disc, AGENT_RGB, 1.0);
        }
        // A three-quarter ring turning about once a second.
        let turn = seconds * TAU * 1.1;
        if let Some(ring) = arc(centre, radius - 6.0, turn, PI * 1.5) {
            self.stroke(&ring, DARK_TEXT, 1.0, 2.4, true);
        }
    }

    /// (page width, where the label starts, the cursor's x, the label's top)
    fn label(
        &mut self,
        font: &OverlayFont,
        text: &str,
        (page_width, start, cursor_x, top): (f64, f64, f64, f64),
    ) {
        let px = LABEL_FONT_PX * self.scale as f32;
        let label_width = font.width(text, px) / self.scale + 16.0;
        let left = if start + label_width > page_width {
            cursor_x - 8.0 - label_width
        } else {
            start
        };
        if let Some(pill) = rounded_rectangle(left, top, label_width, LABEL_HEIGHT, 6.0) {
            self.fill(&pill, AGENT_RGB, 1.0);
        }
        self.text(
            font,
            text,
            ((left + 8.0) * self.scale, (top + 15.0) * self.scale),
            px,
        );
    }

    /// Sets `text` with its baseline starting at `at` (physical pixels), in the dark text colour.
    fn text(&mut self, font: &OverlayFont, text: &str, at: (f64, f64), px: f32) {
        let (red, green, blue) = DARK_TEXT;
        let mut pen = at.0;
        let (width, height) = (self.layer.width() as i64, self.layer.height() as i64);
        for character in text.chars() {
            let (metrics, coverage) = font.font.rasterize(character, px);
            let glyph_left = pen as i64 + i64::from(metrics.xmin);
            let glyph_top = at.1 as i64 - i64::from(metrics.ymin) - metrics.height as i64;
            for row in 0..metrics.height {
                for column in 0..metrics.width {
                    let (x, y) = (glyph_left + column as i64, glyph_top + row as i64);
                    if x < 0 || y < 0 || x >= width || y >= height {
                        continue;
                    }
                    let alpha =
                        f64::from(coverage[row * metrics.width + column]) / 255.0 * self.strength;
                    if alpha <= 0.0 {
                        continue;
                    }
                    let index = ((y * width + x) * 4) as usize;
                    let data = self.layer.data_mut();
                    let inverse = 1.0 - alpha;
                    for (offset, channel) in [red, green, blue].into_iter().enumerate() {
                        let over = channel * alpha * 255.0;
                        data[index + offset] = (over + f64::from(data[index + offset]) * inverse)
                            .round()
                            .min(255.0) as u8;
                    }
                    data[index + 3] = (alpha * 255.0 + f64::from(data[index + 3]) * inverse)
                        .round()
                        .min(255.0) as u8;
                }
            }
            pen += f64::from(metrics.advance_width);
        }
    }
}

fn circle(x: f64, y: f64, radius: f64) -> Option<Path> {
    PathBuilder::from_circle(x as f32, y as f32, radius as f32)
}

/// An arc of a circle as cubic curves, each at most a quarter turn.
fn arc((cx, cy): (f64, f64), radius: f64, start: f64, sweep: f64) -> Option<Path> {
    let pieces = (sweep.abs() / FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / pieces as f64;
    let handle = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |theta: f64| (cx + radius * theta.cos(), cy + radius * theta.sin());
    let tangent = |theta: f64| (-radius * theta.sin(), radius * theta.cos());
    let mut builder = PathBuilder::new();
    let first = point(start);
    builder.move_to(first.0 as f32, first.1 as f32);
    let mut theta = start;
    for _ in 0..pieces {
        let next = theta + step;
        let (p0, p3) = (point(theta), point(next));
        let (t0, t3) = (tangent(theta), tangent(next));
        builder.cubic_to(
            (p0.0 + handle * t0.0) as f32,
            (p0.1 + handle * t0.1) as f32,
            (p3.0 - handle * t3.0) as f32,
            (p3.1 - handle * t3.1) as f32,
            p3.0 as f32,
            p3.1 as f32,
        );
        theta = next;
    }
    builder.finish()
}

fn rounded_rectangle(x: f64, y: f64, width: f64, height: f64, radius: f64) -> Option<Path> {
    // The control-point distance that makes a cubic quarter-circle.
    let k = radius * 0.552_284_75;
    let (r, b) = (x + width, y + height);
    let f = |value: f64| value as f32;
    let mut builder = PathBuilder::new();
    builder.move_to(f(x + radius), f(y));
    builder.line_to(f(r - radius), f(y));
    builder.cubic_to(
        f(r - radius + k),
        f(y),
        f(r),
        f(y + radius - k),
        f(r),
        f(y + radius),
    );
    builder.line_to(f(r), f(b - radius));
    builder.cubic_to(
        f(r),
        f(b - radius + k),
        f(r - radius + k),
        f(b),
        f(r - radius),
        f(b),
    );
    builder.line_to(f(x + radius), f(b));
    builder.cubic_to(
        f(x + radius - k),
        f(b),
        f(x),
        f(b - radius + k),
        f(x),
        f(b - radius),
    );
    builder.line_to(f(x), f(y + radius));
    builder.cubic_to(
        f(x),
        f(y + radius - k),
        f(x + radius - k),
        f(y),
        f(x + radius),
        f(y),
    );
    builder.close();
    builder.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(label: &str, loading: bool, cursor: CursorShape) -> PointerView {
        PointerView {
            harness: "claude".to_owned(),
            label: label.to_owned(),
            cursor,
            busy: true,
            loading,
            at: Some((100.0, 80.0)),
            ripple: None,
            strength: 1.0,
        }
    }

    /// The layer over a white page, as the BGRA the view would show.
    fn over_white(view: &PointerView, scale: f64) -> (Vec<u8>, u32) {
        let font = OverlayFont::load_system();
        let layer = render(view, (400.0, 300.0), scale, 0.3, font.as_ref()).expect("a layer");
        let mut frame = vec![255u8; layer.data().len()];
        composite_over_bgra(&mut frame, &layer);
        if let Some(folder) = std::env::var_os("LYNX_RENDER_PREVIEW") {
            let name = format!("overlay-{}-{}.bgra", view.label.len(), scale);
            std::fs::write(std::path::Path::new(&folder).join(name), &frame).expect("write");
        }
        (frame, layer.width())
    }

    fn pixel(frame: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * width + x) * 4) as usize;
        [frame[at], frame[at + 1], frame[at + 2], frame[at + 3]]
    }

    const WHITE: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn the_frame_cursor_mark_and_label_are_drawn_over_the_page() {
        let (frame, width) =
            over_white(&view("Opening example.com", false, CursorShape::Arrow), 1.0);

        assert_ne!(
            pixel(&frame, width, 1, 150),
            WHITE,
            "the frame around the page"
        );
        assert_ne!(pixel(&frame, width, 112, 106), WHITE, "the mark's disc");
        if OverlayFont::load_system().is_some() {
            assert_ne!(pixel(&frame, width, 150, 104), WHITE, "the label's pill");
        }
        assert_eq!(pixel(&frame, width, 300, 250), WHITE, "far from everything");
    }

    #[test]
    fn a_page_load_puts_a_ring_where_the_label_would_be() {
        let (frame, width) =
            over_white(&view("Opening example.com", true, CursorShape::Arrow), 1.0);

        assert_ne!(pixel(&frame, width, 140, 104), WHITE);
        assert_eq!(pixel(&frame, width, 200, 104), WHITE);
    }

    #[test]
    fn typing_shows_the_text_cursor_instead_of_the_arrow() {
        let (frame, width) = over_white(&view("", false, CursorShape::IBeam), 1.0);

        assert_ne!(
            pixel(&frame, width, 100, 80),
            WHITE,
            "the stem runs through the point"
        );
        assert_eq!(pixel(&frame, width, 105, 70), WHITE);
    }

    #[test]
    fn a_denser_screen_gets_a_layer_of_the_same_look_in_more_pixels() {
        let (frame, width) = over_white(&view("Reading the page", false, CursorShape::Arrow), 2.0);

        assert_eq!(width, 800);
        assert_ne!(pixel(&frame, width, 2, 300), WHITE, "the frame, doubled");
        assert_ne!(pixel(&frame, width, 224, 212), WHITE, "the mark, doubled");
    }

    #[test]
    fn with_no_pointer_yet_only_the_frame_is_drawn() {
        let mut nowhere = view("Reading the page", false, CursorShape::Arrow);
        nowhere.at = None;
        let (frame, width) = over_white(&nowhere, 1.0);

        assert_ne!(pixel(&frame, width, 1, 150), WHITE);
        assert_eq!(pixel(&frame, width, 150, 104), WHITE);
    }

    #[test]
    fn a_fading_overlay_lets_the_page_through() {
        let mut fading = view("", false, CursorShape::Arrow);
        fading.strength = 0.5;
        let (frame, width) = over_white(&fading, 1.0);

        let [blue, green, red, _] = pixel(&frame, width, 1, 150);
        assert!(
            blue > 100 && green > 100 && red > 200,
            "half the colour, half the page: {blue} {green} {red}"
        );
    }

    #[test]
    fn compositing_keeps_untouched_pixels_and_blends_opaque_ones() {
        let mut layer = Pixmap::new(2, 1).unwrap();
        layer.data_mut()[4..8].copy_from_slice(&[255, 0, 0, 255]);
        let mut frame = vec![10u8, 20, 30, 255, 10, 20, 30, 255];

        composite_over_bgra(&mut frame, &layer);

        assert_eq!(
            &frame[0..4],
            &[10, 20, 30, 255],
            "nothing over the first pixel"
        );
        assert_eq!(
            &frame[4..8],
            &[0, 0, 255, 255],
            "an opaque red RGBA is red in BGRA"
        );
    }

    #[test]
    fn a_long_label_is_cut_with_an_ellipsis() {
        assert_eq!(clipped("short"), "short");
        let cut = clipped(&"x".repeat(80));
        assert_eq!(cut.chars().count(), MAX_LABEL_CHARS);
        assert!(cut.ends_with('…'));
    }
}
