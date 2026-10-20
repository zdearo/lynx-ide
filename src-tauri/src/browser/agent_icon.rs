//! The marks of the agents that can drive the browser, drawn small beside the pointer so the
//! person sees which agent is in control. The paths are the same ones the interface shows (see
//! `AgentBrandIcon.tsx`, where each mark's source is credited); they are parsed here from SVG path
//! data, since the canvas has no SVG renderer. The marks are trademarks of their owners and are
//! shown only to name the agent.

use std::collections::HashMap;
use std::sync::OnceLock;

/// One drawing command, with absolute coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Move(f64, f64),
    Line(f64, f64),
    Curve(f64, f64, f64, f64, f64, f64),
    Close,
}

/// One filled outline of a mark, and its colour.
pub struct Shape {
    pub ops: Vec<Op>,
    pub rgb: (f64, f64, f64),
}

/// An agent's mark: its drawing area and the outlines in it.
pub struct Mark {
    pub width: f64,
    pub height: f64,
    pub shapes: Vec<Shape>,
}

/// (view box width and height, [(path data, fill colour)])
type MarkSource = ((f64, f64), &'static [(&'static str, u32)]);

const CLAUDE: MarkSource = (
    (24.0, 24.0),
    &[(
        "m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z",
        0xD97757,
    )],
);

const CODEX: MarkSource = (
    (24.0, 24.0),
    &[(
        "M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z",
        0xFFFFFF,
    )],
);

const OPENCODE: MarkSource = (
    (240.0, 300.0),
    &[
        ("M180 240H60V120H180V240Z", 0x4B4646),
        ("M180 60H60V240H180V60ZM240 300H0V0H240V300Z", 0xF1ECEC),
    ],
);

fn marks() -> &'static HashMap<&'static str, Mark> {
    static MARKS: OnceLock<HashMap<&'static str, Mark>> = OnceLock::new();
    MARKS.get_or_init(|| {
        [("claude", CLAUDE), ("codex", CODEX), ("opencode", OPENCODE)]
            .into_iter()
            .map(|(name, ((width, height), shapes))| {
                let shapes = shapes
                    .iter()
                    .map(|(path, colour)| Shape {
                        ops: parse_path(path),
                        rgb: (
                            f64::from((colour >> 16) & 0xff) / 255.0,
                            f64::from((colour >> 8) & 0xff) / 255.0,
                            f64::from(colour & 0xff) / 255.0,
                        ),
                    })
                    .collect();
                (
                    name,
                    Mark {
                        width,
                        height,
                        shapes,
                    },
                )
            })
            .collect()
    })
}

/// Whether there is a mark for this harness name.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
pub fn has_mark(harness: &str) -> bool {
    marks().contains_key(harness)
}

/// The mark for this harness name, for a renderer to draw however it can.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
pub fn mark(harness: &str) -> Option<&'static Mark> {
    marks().get(harness)
}

/// Draws the harness's mark centred at (`x`, `y`), fitting a square of `size` pixels, with `alpha`.
#[cfg(target_os = "linux")]
pub fn draw(
    context: &gtk::cairo::Context,
    harness: &str,
    (x, y): (f64, f64),
    size: f64,
    alpha: f64,
) {
    let Some(mark) = marks().get(harness) else {
        return;
    };
    let scale = size / mark.width.max(mark.height);
    let _ = context.save();
    context.translate(x - mark.width * scale / 2.0, y - mark.height * scale / 2.0);
    context.scale(scale, scale);
    for shape in &mark.shapes {
        context.new_path();
        for op in &shape.ops {
            match *op {
                Op::Move(x, y) => context.move_to(x, y),
                Op::Line(x, y) => context.line_to(x, y),
                Op::Curve(x1, y1, x2, y2, x, y) => context.curve_to(x1, y1, x2, y2, x, y),
                Op::Close => context.close_path(),
            }
        }
        let (red, green, blue) = shape.rgb;
        context.set_source_rgba(red, green, blue, alpha);
        let _ = context.fill();
    }
    let _ = context.restore();
}

/// Reads SVG path data into absolute commands. Covers what the marks use: moves, lines, curves,
/// arcs and closes, absolute and relative. A malformed path ends where it went wrong.
fn parse_path(data: &str) -> Vec<Op> {
    let mut reader = Reader {
        chars: data.as_bytes(),
        at: 0,
    };
    let mut ops = Vec::new();
    let (mut x, mut y) = (0.0, 0.0);
    let (mut start_x, mut start_y) = (0.0, 0.0);
    // The second control point of the last curve, for the smooth commands.
    let mut previous_control: Option<(f64, f64)> = None;
    let mut command = b'M';

    while reader.skip_separators() {
        if reader.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            command = reader.chars[reader.at];
            reader.at += 1;
            if matches!(command, b'Z' | b'z') {
                ops.push(Op::Close);
                (x, y) = (start_x, start_y);
                previous_control = None;
                continue;
            }
        }
        let relative = command.is_ascii_lowercase();
        let (base_x, base_y) = if relative { (x, y) } else { (0.0, 0.0) };
        let step = (|| -> Option<()> {
            match command.to_ascii_uppercase() {
                b'M' => {
                    let (nx, ny) = (base_x + reader.number()?, base_y + reader.number()?);
                    ops.push(Op::Move(nx, ny));
                    (x, y) = (nx, ny);
                    (start_x, start_y) = (nx, ny);
                    previous_control = None;
                    // Further pairs after a move are lines.
                    command = if relative { b'l' } else { b'L' };
                }
                b'L' => {
                    let (nx, ny) = (base_x + reader.number()?, base_y + reader.number()?);
                    ops.push(Op::Line(nx, ny));
                    (x, y) = (nx, ny);
                    previous_control = None;
                }
                b'H' => {
                    x = base_x + reader.number()?;
                    ops.push(Op::Line(x, y));
                    previous_control = None;
                }
                b'V' => {
                    y = base_y + reader.number()?;
                    ops.push(Op::Line(x, y));
                    previous_control = None;
                }
                b'C' => {
                    let c1 = (base_x + reader.number()?, base_y + reader.number()?);
                    let c2 = (base_x + reader.number()?, base_y + reader.number()?);
                    let end = (base_x + reader.number()?, base_y + reader.number()?);
                    ops.push(Op::Curve(c1.0, c1.1, c2.0, c2.1, end.0, end.1));
                    (x, y) = end;
                    previous_control = Some(c2);
                }
                b'S' => {
                    let c2 = (base_x + reader.number()?, base_y + reader.number()?);
                    let end = (base_x + reader.number()?, base_y + reader.number()?);
                    let c1 =
                        previous_control.map_or((x, y), |(px, py)| (2.0 * x - px, 2.0 * y - py));
                    ops.push(Op::Curve(c1.0, c1.1, c2.0, c2.1, end.0, end.1));
                    (x, y) = end;
                    previous_control = Some(c2);
                }
                b'A' => {
                    let (rx, ry, rotation) = (reader.number()?, reader.number()?, reader.number()?);
                    let (large, sweep) = (reader.flag()?, reader.flag()?);
                    let end = (base_x + reader.number()?, base_y + reader.number()?);
                    arc_to_curves(&mut ops, (x, y), end, (rx, ry), rotation, large, sweep);
                    (x, y) = end;
                    previous_control = None;
                }
                _ => return None,
            }
            Some(())
        })();
        if step.is_none() {
            break;
        }
    }
    ops
}

struct Reader<'a> {
    chars: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn peek(&self) -> Option<u8> {
        self.chars.get(self.at).copied()
    }

    /// Skips spaces and commas; false at the end of the data.
    fn skip_separators(&mut self) -> bool {
        while self
            .peek()
            .is_some_and(|c| c == b' ' || c == b',' || c.is_ascii_whitespace())
        {
            self.at += 1;
        }
        self.at < self.chars.len()
    }

    /// A number such as `-.5`, `12`, `1.5e-3`. A second dot starts the next number.
    fn number(&mut self) -> Option<f64> {
        self.skip_separators();
        let start = self.at;
        if matches!(self.peek(), Some(b'-' | b'+')) {
            self.at += 1;
        }
        let mut seen_dot = false;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                self.at += 1;
            } else if c == b'.' && !seen_dot {
                seen_dot = true;
                self.at += 1;
            } else if matches!(c, b'e' | b'E') {
                self.at += 1;
                if matches!(self.peek(), Some(b'-' | b'+')) {
                    self.at += 1;
                }
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.chars[start..self.at])
            .ok()?
            .parse()
            .ok()
    }

    /// An arc flag: one digit, which may run straight into the number after it.
    fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        let flag = match self.peek()? {
            b'0' => false,
            b'1' => true,
            _ => return None,
        };
        self.at += 1;
        Some(flag)
    }
}

/// Appends an SVG elliptical arc as cubic curves (the endpoint-to-centre conversion of the SVG
/// specification, each piece at most a quarter turn).
fn arc_to_curves(
    ops: &mut Vec<Op>,
    from: (f64, f64),
    to: (f64, f64),
    radii: (f64, f64),
    rotation_degrees: f64,
    large: bool,
    sweep: bool,
) {
    let (mut rx, mut ry) = (radii.0.abs(), radii.1.abs());
    if from == to {
        return;
    }
    if rx == 0.0 || ry == 0.0 {
        ops.push(Op::Line(to.0, to.1));
        return;
    }
    let phi = rotation_degrees.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (from.0 - to.0) / 2.0;
    let dy = (from.1 - to.1) / 2.0;
    let x1 = cos_phi * dx + sin_phi * dy;
    let y1 = -sin_phi * dx + cos_phi * dy;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let numerator = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let denominator = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut factor = (numerator / denominator).max(0.0).sqrt();
    if large == sweep {
        factor = -factor;
    }
    let cx1 = factor * rx * y1 / ry;
    let cy1 = -factor * ry * x1 / rx;
    let cx = cos_phi * cx1 - sin_phi * cy1 + (from.0 + to.0) / 2.0;
    let cy = sin_phi * cx1 + cos_phi * cy1 + (from.1 + to.1) / 2.0;

    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let dot = ux * vx + uy * vy;
        let length = ux.hypot(uy) * vx.hypot(vy);
        let mut angle = (dot / length).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            angle = -angle;
        }
        angle
    };
    let start = angle(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut delta = angle(
        (x1 - cx1) / rx,
        (y1 - cy1) / ry,
        (-x1 - cx1) / rx,
        (-y1 - cy1) / ry,
    );
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }

    let pieces = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = delta / pieces as f64;
    let alpha = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |theta: f64| {
        let (sin, cos) = theta.sin_cos();
        (
            cx + rx * cos * cos_phi - ry * sin * sin_phi,
            cy + rx * cos * sin_phi + ry * sin * cos_phi,
        )
    };
    let tangent = |theta: f64| {
        let (sin, cos) = theta.sin_cos();
        (
            -rx * sin * cos_phi - ry * cos * sin_phi,
            -rx * sin * sin_phi + ry * cos * cos_phi,
        )
    };
    let mut theta = start;
    for piece in 0..pieces {
        let next = theta + step;
        let (p0, p3) = (point(theta), point(next));
        let (t0, t3) = (tangent(theta), tangent(next));
        // The last piece ends exactly where the path said, whatever the rounding.
        let end = if piece + 1 == pieces { to } else { p3 };
        ops.push(Op::Curve(
            p0.0 + alpha * t0.0,
            p0.1 + alpha * t0.1,
            p3.0 - alpha * t3.0,
            p3.1 - alpha * t3.1,
            end.0,
            end.1,
        ));
        theta = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_and_relative_moves_become_absolute() {
        let ops = parse_path("M10 10h5v5H0z");
        assert_eq!(
            ops,
            [
                Op::Move(10.0, 10.0),
                Op::Line(15.0, 10.0),
                Op::Line(15.0, 15.0),
                Op::Line(0.0, 15.0),
                Op::Close,
            ]
        );
    }

    #[test]
    fn numbers_may_run_together_the_way_minified_paths_write_them() {
        let ops = parse_path("m1.5.5-.25-.75");
        assert_eq!(ops, [Op::Move(1.5, 0.5), Op::Line(1.25, -0.25)]);
    }

    #[test]
    fn an_arc_ends_where_the_path_says() {
        let ops = parse_path("M0 0a5 5 0 0 1 10 0");
        let Some(Op::Curve(.., x, y)) = ops.last().copied() else {
            panic!("an arc becomes curves: {ops:?}");
        };
        assert_eq!((x, y), (10.0, 0.0));
        assert!(ops.len() >= 3, "a half turn takes more than one piece");
    }

    #[test]
    fn arc_flags_may_be_written_without_separators() {
        let ops = parse_path("M0 0a5 5 0 0110 0");
        assert!(matches!(ops.last(), Some(Op::Curve(.., x, _)) if *x == 10.0));
    }

    #[test]
    fn every_mark_parses_into_a_closed_drawing_inside_its_box() {
        for (name, mark) in marks() {
            assert!(!mark.shapes.is_empty(), "{name}");
            for shape in &mark.shapes {
                assert!(shape.ops.len() > 3, "{name} parsed to almost nothing");
                assert!(shape.ops.contains(&Op::Close), "{name}");
                for op in &shape.ops {
                    let points: Vec<(f64, f64)> = match *op {
                        Op::Move(x, y) | Op::Line(x, y) => vec![(x, y)],
                        Op::Curve(a, b, c, d, e, f) => vec![(a, b), (c, d), (e, f)],
                        Op::Close => vec![],
                    };
                    for (x, y) in points {
                        assert!(
                            x > -2.0 && x < mark.width + 2.0 && y > -2.0 && y < mark.height + 2.0,
                            "{name}: ({x}, {y}) is outside {}x{}",
                            mark.width,
                            mark.height
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_supported_agents_have_marks_and_others_do_not() {
        assert!(has_mark("claude") && has_mark("codex") && has_mark("opencode"));
        assert!(!has_mark("cursor") && !has_mark(""));
    }
}
