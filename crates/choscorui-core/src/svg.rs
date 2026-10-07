//! Import of a documented SVG subset for icons and simple vector art.
//!
//! Supported: `svg` (`viewBox`, `width`, `height`), `g`, `path`, `rect`
//! (including `rx`/`ry` as one radius), `circle`, `ellipse`, `line`,
//! `polyline`, `polygon`; presentation attributes and `style` declarations
//! for `fill`, `stroke`, `stroke-width`, `stroke-linecap`,
//! `stroke-linejoin`, `fill-rule`, `opacity`, `fill-opacity`, and
//! `stroke-opacity`; `transform` with `matrix`, `translate`, `scale`,
//! `rotate`, `skewX`, and `skewY`; colors as `#rgb`, `#rrggbb`,
//! `rgb(…)`, `none`, `currentColor`, and basic named colors. `title`,
//! `desc`, `metadata`, and comments are ignored. Any other element (for
//! example `text`, `use`, `image`, gradients, filters, masks) is rejected
//! with [`SvgError::Unsupported`] rather than silently dropped.

use crate::{
    Color, DisplayList, DisplayListError, FillRule, LineCap, LineJoin, Paint, Path, Point, Rect,
    StrokeStyle, Transform,
};
use std::fmt;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SvgError {
    Malformed(String),
    Unsupported(String),
    InvalidNumber(String),
    InvalidPathData(String),
    InvalidColor(String),
    MissingSize,
}

impl fmt::Display for SvgError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(detail) => write!(formatter, "malformed SVG: {detail}"),
            Self::Unsupported(name) => write!(formatter, "unsupported SVG feature `{name}`"),
            Self::InvalidNumber(text) => write!(formatter, "invalid SVG number `{text}`"),
            Self::InvalidPathData(text) => write!(formatter, "invalid SVG path data near `{text}`"),
            Self::InvalidColor(text) => write!(formatter, "invalid SVG color `{text}`"),
            Self::MissingSize => formatter.write_str("SVG needs a viewBox or width and height"),
        }
    }
}

impl std::error::Error for SvgError {}

#[derive(Clone, Debug, PartialEq)]
enum SvgPaint {
    None,
    Color(Color),
    Current,
}

/// One filled and/or stroked shape in document coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgShape {
    pub path: Path,
    fill: SvgPaint,
    stroke: SvgPaint,
    pub stroke_style: StrokeStyle,
    pub fill_rule: FillRule,
    pub fill_opacity: f64,
    pub stroke_opacity: f64,
}

/// A parsed SVG document ready to draw at any size.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgDocument {
    pub view_box: Rect,
    pub shapes: Vec<SvgShape>,
}

#[derive(Clone)]
struct Style {
    fill: SvgPaint,
    stroke: SvgPaint,
    stroke_width: f64,
    cap: LineCap,
    join: LineJoin,
    rule: FillRule,
    opacity: f64,
    fill_opacity: f64,
    stroke_opacity: f64,
    transform: Transform,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            fill: SvgPaint::Color(Color::BLACK),
            stroke: SvgPaint::None,
            stroke_width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
            rule: FillRule::NonZero,
            opacity: 1.0,
            fill_opacity: 1.0,
            stroke_opacity: 1.0,
            transform: Transform::IDENTITY,
        }
    }
}

struct Element {
    name: String,
    attributes: Vec<(String, String)>,
    self_closing: bool,
}

enum Token {
    Open(Element),
    Close(String),
}

fn tokenize(source: &str) -> Result<Vec<Token>, SvgError> {
    let mut tokens = Vec::new();
    let mut rest = source;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        if let Some(after) = rest.strip_prefix("<!--") {
            let end = after
                .find("-->")
                .ok_or_else(|| SvgError::Malformed("unterminated comment".into()))?;
            rest = &after[end + 3..];
            continue;
        }
        if rest.starts_with("<?") || rest.starts_with("<!") {
            let end = rest
                .find('>')
                .ok_or_else(|| SvgError::Malformed("unterminated declaration".into()))?;
            rest = &rest[end + 1..];
            continue;
        }
        let end =
            find_tag_end(rest).ok_or_else(|| SvgError::Malformed("unterminated tag".into()))?;
        let tag = &rest[1..end];
        rest = &rest[end + 1..];
        if let Some(name) = tag.strip_prefix('/') {
            tokens.push(Token::Close(name.trim().to_owned()));
            continue;
        }
        let self_closing = tag.trim_end().ends_with('/');
        let body = tag.trim_end().trim_end_matches('/');
        let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
        let name = body[..name_end].to_owned();
        if name.is_empty() {
            return Err(SvgError::Malformed("empty tag name".into()));
        }
        let attributes = parse_attributes(&body[name_end..])?;
        tokens.push(Token::Open(Element {
            name,
            attributes,
            self_closing,
        }));
    }
    Ok(tokens)
}

fn find_tag_end(text: &str) -> Option<usize> {
    let mut quote = None;
    for (index, c) in text.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(index),
            _ => {}
        }
    }
    None
}

fn parse_attributes(text: &str) -> Result<Vec<(String, String)>, SvgError> {
    let mut out = Vec::new();
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let eq = rest
            .find('=')
            .ok_or_else(|| SvgError::Malformed(format!("attribute without value: {rest}")))?;
        let name = rest[..eq].trim().to_owned();
        let after = rest[eq + 1..].trim_start();
        let quote = after
            .chars()
            .next()
            .filter(|c| *c == '"' || *c == '\'')
            .ok_or_else(|| SvgError::Malformed(format!("unquoted attribute `{name}`")))?;
        let close = after[1..]
            .find(quote)
            .ok_or_else(|| SvgError::Malformed(format!("unterminated attribute `{name}`")))?;
        let value = decode_entities(&after[1..1 + close]);
        out.push((name, value));
        rest = after[close + 2..].trim_start();
    }
    Ok(out)
}

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn number(text: &str) -> Result<f64, SvgError> {
    let trimmed = text.trim().trim_end_matches("px");
    trimmed
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| SvgError::InvalidNumber(text.to_owned()))
}

fn numbers(text: &str) -> Result<Vec<f64>, SvgError> {
    let mut out = Vec::new();
    let mut scanner = Scanner::new(text);
    while scanner.skip_separators() {
        out.push(scanner.number()?);
    }
    Ok(out)
}

fn parse_color(text: &str) -> Result<SvgPaint, SvgError> {
    let value = text.trim();
    let invalid = || SvgError::InvalidColor(value.to_owned());
    if value == "none" || value == "transparent" {
        return Ok(SvgPaint::None);
    }
    if value == "currentColor" {
        return Ok(SvgPaint::Current);
    }
    if let Some(hex) = value.strip_prefix('#') {
        let digits: Vec<u8> = hex
            .chars()
            .map(|c| c.to_digit(16).map(|d| d as u8))
            .collect::<Option<_>>()
            .ok_or_else(invalid)?;
        return match digits.as_slice() {
            [r, g, b] => Ok(SvgPaint::Color(Color::rgb8(r * 17, g * 17, b * 17))),
            [r1, r2, g1, g2, b1, b2] => Ok(SvgPaint::Color(Color::rgb8(
                r1 * 16 + r2,
                g1 * 16 + g2,
                b1 * 16 + b2,
            ))),
            _ => Err(invalid()),
        };
    }
    if let Some(inner) = value.strip_prefix("rgb(").and_then(|v| v.strip_suffix(')')) {
        let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
        let [r, g, b] = parts.as_slice() else {
            return Err(invalid());
        };
        let channel = |part: &str| -> Result<u8, SvgError> {
            if let Some(percent) = part.strip_suffix('%') {
                Ok((number(percent)?.clamp(0.0, 100.0) * 2.55).round() as u8)
            } else {
                Ok(number(part)?.clamp(0.0, 255.0).round() as u8)
            }
        };
        return Ok(SvgPaint::Color(Color::rgb8(
            channel(r)?,
            channel(g)?,
            channel(b)?,
        )));
    }
    let named = match value.to_ascii_lowercase().as_str() {
        "black" => Color::rgb8(0, 0, 0),
        "white" => Color::rgb8(255, 255, 255),
        "red" => Color::rgb8(255, 0, 0),
        "green" => Color::rgb8(0, 128, 0),
        "blue" => Color::rgb8(0, 0, 255),
        "yellow" => Color::rgb8(255, 255, 0),
        "gray" | "grey" => Color::rgb8(128, 128, 128),
        "orange" => Color::rgb8(255, 165, 0),
        "purple" => Color::rgb8(128, 0, 128),
        _ => return Err(invalid()),
    };
    Ok(SvgPaint::Color(named))
}

fn parse_transform(text: &str) -> Result<Transform, SvgError> {
    let mut result = Transform::IDENTITY;
    let mut rest = text.trim();
    while !rest.is_empty() {
        let open = rest
            .find('(')
            .ok_or_else(|| SvgError::Malformed(format!("transform `{text}`")))?;
        let close = rest[open..]
            .find(')')
            .ok_or_else(|| SvgError::Malformed(format!("transform `{text}`")))?
            + open;
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let args = numbers(&rest[open + 1..close])?;
        let bad = || SvgError::Malformed(format!("transform `{name}` arguments"));
        let next = match (name, args.as_slice()) {
            ("matrix", [a, b, c, d, e, f]) => Transform {
                a: *a,
                b: *b,
                c: *c,
                d: *d,
                tx: *e,
                ty: *f,
            },
            ("translate", [x]) => Transform::translate(*x, 0.0),
            ("translate", [x, y]) => Transform::translate(*x, *y),
            ("scale", [s]) => Transform::scale(*s, *s),
            ("scale", [x, y]) => Transform::scale(*x, *y),
            ("rotate", [deg]) => Transform::rotate(deg.to_radians()),
            ("rotate", [deg, cx, cy]) => Transform::translate(-cx, -cy)
                .then(Transform::rotate(deg.to_radians()))
                .then(Transform::translate(*cx, *cy)),
            ("skewX", [deg]) => Transform {
                c: deg.to_radians().tan(),
                ..Transform::IDENTITY
            },
            ("skewY", [deg]) => Transform {
                b: deg.to_radians().tan(),
                ..Transform::IDENTITY
            },
            ("matrix" | "translate" | "scale" | "rotate" | "skewX" | "skewY", _) => {
                return Err(bad());
            }
            (other, _) => return Err(SvgError::Unsupported(format!("transform {other}"))),
        };
        // SVG lists apply right to left: the last transform applies first.
        result = next.then(result);
        rest = rest[close + 1..].trim_start();
    }
    Ok(result)
}

struct Scanner<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Scanner<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    /// Skips whitespace and commas; returns whether input remains.
    fn skip_separators(&mut self) -> bool {
        let bytes = self.text.as_bytes();
        while self.pos < bytes.len()
            && (bytes[self.pos].is_ascii_whitespace() || bytes[self.pos] == b',')
        {
            self.pos += 1;
        }
        self.pos < bytes.len()
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn number(&mut self) -> Result<f64, SvgError> {
        self.skip_separators();
        let bytes = self.text.as_bytes();
        let start = self.pos;
        let mut seen_dot = false;
        let mut seen_exp = false;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.pos += 1;
        }
        while let Some(c) = self.peek() {
            match c {
                b'0'..=b'9' => self.pos += 1,
                b'.' if !seen_dot && !seen_exp => {
                    seen_dot = true;
                    self.pos += 1;
                }
                b'e' | b'E' if !seen_exp && self.pos > start => {
                    seen_exp = true;
                    self.pos += 1;
                    if matches!(self.peek(), Some(b'+' | b'-')) {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
        let token = &self.text[start..self.pos];
        if token.is_empty() || token == "-" || token == "+" || token == "." {
            let near: String = self.text[start..].chars().take(12).collect();
            return Err(SvgError::InvalidPathData(near));
        }
        let _ = bytes;
        number(token)
    }

    fn flag(&mut self) -> Result<bool, SvgError> {
        self.skip_separators();
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.pos += 1;
                Ok(true)
            }
            _ => Err(SvgError::InvalidPathData(
                self.text[self.pos..].chars().take(12).collect(),
            )),
        }
    }
}

/// Parses SVG path data (`M L H V C S Q T A Z`, absolute and relative).
pub fn parse_path_data(data: &str) -> Result<Path, SvgError> {
    let mut path = Path::new();
    let mut s = Scanner::new(data);
    let mut current = Point::default();
    let mut start = Point::default();
    let mut last_cubic: Option<Point> = None;
    let mut last_quad: Option<Point> = None;
    let mut command: Option<u8> = None;
    while s.skip_separators() {
        let c = s.peek().expect("not empty");
        if c.is_ascii_alphabetic() {
            command = Some(c);
            s.pos += 1;
        } else if command.is_none() {
            return Err(SvgError::InvalidPathData(data.chars().take(12).collect()));
        }
        let cmd = command.expect("set above");
        let relative = cmd.is_ascii_lowercase();
        let base = if relative { current } else { Point::default() };
        let pt = |s: &mut Scanner| -> Result<Point, SvgError> {
            let x = s.number()?;
            let y = s.number()?;
            Ok(Point::new(base.x + x, base.y + y))
        };
        let (mut next_cubic, mut next_quad) = (None, None);
        match cmd.to_ascii_uppercase() {
            b'M' => {
                current = pt(&mut s)?;
                start = current;
                path.move_to(current);
                // Subsequent pairs are implicit line-tos.
                command = Some(if relative { b'l' } else { b'L' });
            }
            b'L' => {
                current = pt(&mut s)?;
                path.line_to(current);
            }
            b'H' => {
                let x = s.number()?;
                current = Point::new(if relative { current.x + x } else { x }, current.y);
                path.line_to(current);
            }
            b'V' => {
                let y = s.number()?;
                current = Point::new(current.x, if relative { current.y + y } else { y });
                path.line_to(current);
            }
            b'C' => {
                let (c1, c2, end) = (pt(&mut s)?, pt(&mut s)?, pt(&mut s)?);
                path.cubic_to(c1, c2, end);
                next_cubic = Some(c2);
                current = end;
            }
            b'S' => {
                let c1 = last_cubic.map_or(current, |c| {
                    Point::new(2.0 * current.x - c.x, 2.0 * current.y - c.y)
                });
                let (c2, end) = (pt(&mut s)?, pt(&mut s)?);
                path.cubic_to(c1, c2, end);
                next_cubic = Some(c2);
                current = end;
            }
            b'Q' => {
                let (c, end) = (pt(&mut s)?, pt(&mut s)?);
                path.quad_to(c, end);
                next_quad = Some(c);
                current = end;
            }
            b'T' => {
                let c = last_quad.map_or(current, |q| {
                    Point::new(2.0 * current.x - q.x, 2.0 * current.y - q.y)
                });
                let end = pt(&mut s)?;
                path.quad_to(c, end);
                next_quad = Some(c);
                current = end;
            }
            b'A' => {
                let rx = s.number()?.abs();
                let ry = s.number()?.abs();
                let rotation = s.number()?;
                let large = s.flag()?;
                let sweep = s.flag()?;
                let end = pt(&mut s)?;
                arc_to(&mut path, current, rx, ry, rotation, large, sweep, end);
                current = end;
            }
            b'Z' => {
                path.close();
                current = start;
                command = None;
            }
            other => {
                return Err(SvgError::Unsupported(format!(
                    "path command {}",
                    other as char
                )));
            }
        }
        last_cubic = next_cubic;
        last_quad = next_quad;
    }
    Ok(path)
}

/// Converts an SVG elliptical arc to cubic segments (SVG spec F.6).
#[allow(clippy::too_many_arguments)]
fn arc_to(
    path: &mut Path,
    from: Point,
    rx: f64,
    ry: f64,
    rotation: f64,
    large: bool,
    sweep: bool,
    to: Point,
) {
    if from == to {
        return;
    }
    if rx == 0.0 || ry == 0.0 {
        path.line_to(to);
        return;
    }
    let phi = rotation.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let dx = (from.x - to.x) / 2.0;
    let dy = (from.y - to.y) / 2.0;
    let x1 = cos_phi * dx + sin_phi * dy;
    let y1 = -sin_phi * dx + cos_phi * dy;
    let (mut rx, mut ry) = (rx, ry);
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut coef = (num / den).max(0.0).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let cx1 = coef * rx * y1 / ry;
    let cy1 = -coef * ry * x1 / rx;
    let cx = cos_phi * cx1 - sin_phi * cy1 + (from.x + to.x) / 2.0;
    let cy = sin_phi * cx1 + cos_phi * cy1 + (from.y + to.y) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let theta1 = angle(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
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
    let segments = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = delta / segments as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| {
        let (s, c) = t.sin_cos();
        Point::new(
            cx + rx * c * cos_phi - ry * s * sin_phi,
            cy + rx * c * sin_phi + ry * s * cos_phi,
        )
    };
    let derivative = |t: f64| {
        let (s, c) = t.sin_cos();
        Point::new(
            -rx * s * cos_phi - ry * c * sin_phi,
            -rx * s * sin_phi + ry * c * cos_phi,
        )
    };
    let mut t = theta1;
    for _ in 0..segments {
        let (p0, p3) = (point(t), point(t + step));
        let (d0, d3) = (derivative(t), derivative(t + step));
        path.cubic_to(
            Point::new(p0.x + k * d0.x, p0.y + k * d0.y),
            Point::new(p3.x - k * d3.x, p3.y - k * d3.y),
            p3,
        );
        t += step;
    }
}

fn apply_style(style: &mut Style, name: &str, value: &str) -> Result<(), SvgError> {
    match name {
        "fill" => style.fill = parse_color(value)?,
        "stroke" => style.stroke = parse_color(value)?,
        "stroke-width" => style.stroke_width = number(value)?,
        "stroke-linecap" => {
            style.cap = match value.trim() {
                "butt" => LineCap::Butt,
                "round" => LineCap::Round,
                "square" => LineCap::Square,
                other => return Err(SvgError::Unsupported(format!("stroke-linecap {other}"))),
            }
        }
        "stroke-linejoin" => {
            style.join = match value.trim() {
                "miter" => LineJoin::Miter,
                "round" => LineJoin::Round,
                "bevel" => LineJoin::Bevel,
                other => return Err(SvgError::Unsupported(format!("stroke-linejoin {other}"))),
            }
        }
        "fill-rule" => {
            style.rule = match value.trim() {
                "nonzero" => FillRule::NonZero,
                "evenodd" => FillRule::EvenOdd,
                other => return Err(SvgError::Unsupported(format!("fill-rule {other}"))),
            }
        }
        "opacity" => style.opacity *= number(value)?.clamp(0.0, 1.0),
        "fill-opacity" => style.fill_opacity = number(value)?.clamp(0.0, 1.0),
        "stroke-opacity" => style.stroke_opacity = number(value)?.clamp(0.0, 1.0),
        "transform" => style.transform = parse_transform(value)?.then(style.transform),
        "style" => {
            for declaration in value.split(';') {
                if let Some((key, val)) = declaration.split_once(':') {
                    apply_style(style, key.trim(), val.trim())?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn attr<'a>(element: &'a Element, name: &str) -> Option<&'a str> {
    element
        .attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn num_attr(element: &Element, name: &str) -> Result<f64, SvgError> {
    attr(element, name).map_or(Ok(0.0), number)
}

impl SvgDocument {
    pub fn parse(source: &str) -> Result<Self, SvgError> {
        let tokens = tokenize(source)?;
        let mut stack: Vec<(String, Style)> = Vec::new();
        let mut shapes = Vec::new();
        let mut view_box = None;
        let mut ignored_depth = 0usize;
        for token in tokens {
            match token {
                Token::Close(name) => {
                    if ignored_depth > 0 {
                        ignored_depth -= 1;
                        continue;
                    }
                    match stack.pop() {
                        Some((open, _)) if open == name => {}
                        _ => return Err(SvgError::Malformed(format!("unexpected </{name}>"))),
                    }
                }
                Token::Open(element) => {
                    if ignored_depth > 0 {
                        if !element.self_closing {
                            ignored_depth += 1;
                        }
                        continue;
                    }
                    let local = element.name.rsplit(':').next().unwrap_or(&element.name);
                    if matches!(local, "title" | "desc" | "metadata") {
                        if !element.self_closing {
                            ignored_depth = 1;
                        }
                        continue;
                    }
                    let parent = stack.last().map(|(_, s)| s.clone()).unwrap_or_default();
                    let mut style = parent.clone();
                    // Transforms compose with the parent's; other attributes
                    // override inherited values.
                    style.transform = Transform::IDENTITY;
                    for (name, value) in &element.attributes {
                        apply_style(&mut style, name, value)?;
                    }
                    style.transform = style.transform.then(parent.transform);
                    if stack.is_empty() {
                        if local != "svg" {
                            return Err(SvgError::Malformed("root element must be <svg>".into()));
                        }
                        view_box = Some(root_view_box(&element)?);
                    } else {
                        let path = match local {
                            "g" | "svg" => None,
                            "path" => Some(parse_path_data(attr(&element, "d").unwrap_or(""))?),
                            "rect" => {
                                let rect = Rect::from_xywh(
                                    num_attr(&element, "x")?,
                                    num_attr(&element, "y")?,
                                    num_attr(&element, "width")?,
                                    num_attr(&element, "height")?,
                                );
                                let radius =
                                    num_attr(&element, "rx")?.max(num_attr(&element, "ry")?);
                                Some(Path::rounded_rect(rect, radius))
                            }
                            "circle" => {
                                let r = num_attr(&element, "r")?;
                                let (cx, cy) =
                                    (num_attr(&element, "cx")?, num_attr(&element, "cy")?);
                                Some(Path::ellipse(Rect::from_xywh(
                                    cx - r,
                                    cy - r,
                                    2.0 * r,
                                    2.0 * r,
                                )))
                            }
                            "ellipse" => {
                                let (rx, ry) =
                                    (num_attr(&element, "rx")?, num_attr(&element, "ry")?);
                                let (cx, cy) =
                                    (num_attr(&element, "cx")?, num_attr(&element, "cy")?);
                                Some(Path::ellipse(Rect::from_xywh(
                                    cx - rx,
                                    cy - ry,
                                    2.0 * rx,
                                    2.0 * ry,
                                )))
                            }
                            "line" => Some(Path::line(
                                Point::new(num_attr(&element, "x1")?, num_attr(&element, "y1")?),
                                Point::new(num_attr(&element, "x2")?, num_attr(&element, "y2")?),
                            )),
                            "polyline" | "polygon" => {
                                let values = numbers(attr(&element, "points").unwrap_or(""))?;
                                if values.len() % 2 != 0 {
                                    return Err(SvgError::Malformed(format!("odd {local} points")));
                                }
                                let points: Vec<Point> =
                                    values.chunks(2).map(|p| Point::new(p[0], p[1])).collect();
                                if local == "polygon" {
                                    Some(Path::polygon(&points))
                                } else {
                                    let mut path = Path::new();
                                    if let Some((first, rest)) = points.split_first() {
                                        path.move_to(*first);
                                        for p in rest {
                                            path.line_to(*p);
                                        }
                                    }
                                    Some(path)
                                }
                            }
                            other => return Err(SvgError::Unsupported(other.to_owned())),
                        };
                        if let Some(path) = path {
                            shapes.push(SvgShape {
                                path: path.transformed(style.transform),
                                fill: style.fill.clone(),
                                stroke: style.stroke.clone(),
                                stroke_style: StrokeStyle {
                                    width: style.stroke_width * transform_scale(style.transform),
                                    cap: style.cap,
                                    join: style.join,
                                    dash: Vec::new(),
                                },
                                fill_rule: style.rule,
                                fill_opacity: style.fill_opacity * style.opacity,
                                stroke_opacity: style.stroke_opacity * style.opacity,
                            });
                        }
                    }
                    if !element.self_closing {
                        stack.push((element.name.clone(), style));
                    }
                }
            }
        }
        if !stack.is_empty() {
            return Err(SvgError::Malformed("unclosed elements".into()));
        }
        Ok(Self {
            view_box: view_box.ok_or_else(|| SvgError::Malformed("no <svg> element".into()))?,
            shapes,
        })
    }

    /// Appends the document scaled uniformly and centered into `dest`.
    /// `current_color` resolves `currentColor`, so icons follow the theme.
    pub fn draw(
        &self,
        list: &mut DisplayList,
        dest: Rect,
        current_color: Color,
    ) -> Result<(), DisplayListError> {
        let vb = self.view_box;
        if vb.size.width <= 0.0 || vb.size.height <= 0.0 {
            return Ok(());
        }
        let scale = (dest.size.width / vb.size.width).min(dest.size.height / vb.size.height);
        let dx = dest.min_x() + (dest.size.width - vb.size.width * scale) / 2.0;
        let dy = dest.min_y() + (dest.size.height - vb.size.height * scale) / 2.0;
        let transform = Transform::translate(-vb.min_x(), -vb.min_y())
            .then(Transform::scale(scale, scale))
            .then(Transform::translate(dx, dy));
        for shape in &self.shapes {
            let path = shape.path.transformed(transform);
            let resolve = |paint: &SvgPaint, opacity: f64| match paint {
                SvgPaint::None => None,
                SvgPaint::Color(color) => Some(color.with_alpha(opacity)),
                SvgPaint::Current => Some(current_color.with_alpha(opacity)),
            };
            if let Some(color) = resolve(&shape.fill, shape.fill_opacity) {
                list.fill_path(path.clone(), Paint::Solid(color), shape.fill_rule)?;
            }
            if let Some(color) = resolve(&shape.stroke, shape.stroke_opacity) {
                let mut style = shape.stroke_style.clone();
                style.width *= scale;
                if style.width > 0.0 {
                    list.stroke_path(path, Paint::Solid(color), style)?;
                }
            }
        }
        Ok(())
    }
}

fn transform_scale(t: Transform) -> f64 {
    t.determinant().abs().sqrt()
}

fn root_view_box(element: &Element) -> Result<Rect, SvgError> {
    if let Some(value) = attr(element, "viewBox") {
        let values = numbers(value)?;
        let [x, y, w, h] = values.as_slice() else {
            return Err(SvgError::Malformed(format!("viewBox `{value}`")));
        };
        if *w <= 0.0 || *h <= 0.0 {
            return Err(SvgError::Malformed(format!("viewBox `{value}`")));
        }
        return Ok(Rect::from_xywh(*x, *y, *w, *h));
    }
    match (attr(element, "width"), attr(element, "height")) {
        (Some(w), Some(h)) => Ok(Rect::from_xywh(0.0, 0.0, number(w)?, number(h)?)),
        _ => Err(SvgError::MissingSize),
    }
}
