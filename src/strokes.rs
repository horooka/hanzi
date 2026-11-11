//! Stroke-order data and a tiny SVG-path parser.
//!
//! The JSON files in `data/strokes/` come from hanzi-writer-data / Make Me a
//! Hanzi. Coordinates use a 1024-unit-wide box with the origin at the bottom
//! left and y increasing upward (`x` 0..1024, `y` -124..900).

use gtk::cairo;

use crate::RESOURCE_PREFIX;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seg {
    Move(Point),
    Line(Point),
    Curve(Point, Point, Point),
    Close,
}

#[derive(Debug, Clone)]
pub struct StrokeData {
    pub strokes: Vec<Vec<Seg>>,
    pub medians: Vec<Vec<Point>>,
}

#[derive(Debug, Clone, Copy)]
pub struct ViewTransform {
    pub scale: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

impl ViewTransform {
    /// Fit the 1024x1024 Make Me a Hanzi box into `width` x `height`.
    pub fn new(width: f64, height: f64, padding: f64) -> Self {
        let available_w = (width - 2.0 * padding).max(1.0);
        let available_h = (height - 2.0 * padding).max(1.0);
        let scale = (available_w / 1024.0).min(available_h / 1024.0);
        let drawn = 1024.0 * scale;

        Self {
            scale,
            offset_x: (width - drawn) / 2.0,
            offset_y: (height - drawn) / 2.0,
        }
    }

    pub fn matrix(&self) -> cairo::Matrix {
        cairo::Matrix::new(
            self.scale,
            0.0,
            0.0,
            -self.scale,
            self.offset_x,
            self.offset_y + 900.0 * self.scale,
        )
    }

    pub fn to_data(self, x: f64, y: f64) -> Point {
        Point {
            x: (x - self.offset_x) / self.scale,
            y: 900.0 - (y - self.offset_y) / self.scale,
        }
    }
}

impl StrokeData {
    pub fn load(character: char) -> Option<Self> {
        let path = format!("{RESOURCE_PREFIX}/strokes/{:x}.json", character as u32);
        let bytes = gio::resources_lookup_data(&path, gio::ResourceLookupFlags::NONE).ok()?;
        let raw: RawStrokeData = serde_json::from_slice(&bytes).ok()?;

        if raw.strokes.len() != raw.medians.len() || raw.strokes.is_empty() {
            return None;
        }

        let mut strokes = Vec::with_capacity(raw.strokes.len());
        for path in &raw.strokes {
            strokes.push(parse_path(path)?);
        }

        let medians = raw
            .medians
            .into_iter()
            .map(|median| {
                median
                    .into_iter()
                    .map(|point| Point {
                        x: point[0],
                        y: point[1],
                    })
                    .collect()
            })
            .collect();

        Some(Self { strokes, medians })
    }

    pub fn stroke_count(&self) -> usize {
        self.strokes.len()
    }

    pub fn append_stroke(&self, cr: &cairo::Context, index: usize) {
        let Some(stroke) = self.strokes.get(index) else {
            return;
        };

        for segment in stroke {
            match segment {
                Seg::Move(point) => cr.move_to(point.x, point.y),
                Seg::Line(point) => cr.line_to(point.x, point.y),
                Seg::Curve(c1, c2, end) => cr.curve_to(c1.x, c1.y, c2.x, c2.y, end.x, end.y),
                Seg::Close => cr.close_path(),
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct RawStrokeData {
    strokes: Vec<String>,
    medians: Vec<Vec<[f64; 2]>>,
}

#[derive(Debug, Clone, Copy)]
enum Token {
    Command(char),
    Number(f64),
}

fn tokenize(data: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let bytes = data.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];

        if byte.is_ascii_alphabetic() {
            tokens.push(Token::Command(byte as char));
            index += 1;
        } else if byte.is_ascii_whitespace() || byte == b',' {
            index += 1;
        } else if byte == b'-' || byte == b'+' || byte == b'.' || byte.is_ascii_digit() {
            if let Some((number, end)) = scan_number(data, index) {
                tokens.push(Token::Number(number));
                index = end;
            } else {
                index += 1;
            }
        } else {
            index += 1;
        }
    }

    tokens
}

fn scan_number(data: &str, start: usize) -> Option<(f64, usize)> {
    let bytes = data.as_bytes();
    let mut index = start;

    if index < bytes.len() && (bytes[index] == b'-' || bytes[index] == b'+') {
        index += 1;
    }

    let mut has_digit = false;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
        has_digit = true;
    }

    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            has_digit = true;
        }
    }

    if !has_digit {
        return None;
    }

    if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
        let exponent_start = index;
        index += 1;
        if index < bytes.len() && (bytes[index] == b'-' || bytes[index] == b'+') {
            index += 1;
        }
        let mut exponent_digits = false;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            exponent_digits = true;
        }
        if !exponent_digits {
            index = exponent_start;
        }
    }

    data[start..index]
        .parse()
        .ok()
        .map(|number| (number, index))
}

fn take(tokens: &[Token], index: &mut usize, count: usize) -> Option<Vec<f64>> {
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        match tokens.get(*index) {
            Some(Token::Number(value)) => {
                values.push(*value);
                *index += 1;
            }
            _ => return None,
        }
    }
    Some(values)
}

/// Parse the subset of SVG path commands used by Make Me a Hanzi: M/L/H/V/C/S/Q/T/Z.
fn parse_path(data: &str) -> Option<Vec<Seg>> {
    let tokens = tokenize(data);
    let mut segments = Vec::new();
    let mut index = 0;
    let mut command: Option<char> = None;
    let mut current = Point { x: 0.0, y: 0.0 };
    let mut subpath_start = current;
    let mut last_control: Option<Point> = None;
    let mut last_was_cubic = false;
    let mut last_was_quadratic = false;

    while index < tokens.len() {
        let explicit = match tokens.get(index) {
            Some(Token::Command(character)) => {
                index += 1;
                Some(*character)
            }
            _ => None,
        };
        let command_character = explicit.or(command)?;
        let relative = command_character.is_ascii_lowercase();
        let upper = command_character.to_ascii_uppercase();

        let point = |x: f64, y: f64| {
            if relative {
                Point {
                    x: current.x + x,
                    y: current.y + y,
                }
            } else {
                Point { x, y }
            }
        };

        match upper {
            'M' => {
                let values = take(&tokens, &mut index, 2)?;
                let end = point(values[0], values[1]);
                segments.push(Seg::Move(end));
                current = end;
                subpath_start = end;
                // Additional coordinate pairs after a moveto are implicit linetos.
                command = Some(if relative { 'l' } else { 'L' });
                last_was_cubic = false;
                last_was_quadratic = false;
                continue;
            }
            'L' => {
                let values = take(&tokens, &mut index, 2)?;
                let end = point(values[0], values[1]);
                segments.push(Seg::Line(end));
                current = end;
                last_was_cubic = false;
                last_was_quadratic = false;
            }
            'H' => {
                let values = take(&tokens, &mut index, 1)?;
                let x = if relative {
                    current.x + values[0]
                } else {
                    values[0]
                };
                let end = Point { x, y: current.y };
                segments.push(Seg::Line(end));
                current = end;
                last_was_cubic = false;
                last_was_quadratic = false;
            }
            'V' => {
                let values = take(&tokens, &mut index, 1)?;
                let y = if relative {
                    current.y + values[0]
                } else {
                    values[0]
                };
                let end = Point { x: current.x, y };
                segments.push(Seg::Line(end));
                current = end;
                last_was_cubic = false;
                last_was_quadratic = false;
            }
            'C' => {
                let values = take(&tokens, &mut index, 6)?;
                let c1 = point(values[0], values[1]);
                let c2 = point(values[2], values[3]);
                let end = point(values[4], values[5]);
                segments.push(Seg::Curve(c1, c2, end));
                current = end;
                last_control = Some(c2);
                last_was_cubic = true;
                last_was_quadratic = false;
            }
            'S' => {
                let values = take(&tokens, &mut index, 4)?;
                let c1 = if last_was_cubic {
                    let control = last_control.unwrap_or(current);
                    Point {
                        x: 2.0 * current.x - control.x,
                        y: 2.0 * current.y - control.y,
                    }
                } else {
                    current
                };
                let c2 = point(values[0], values[1]);
                let end = point(values[2], values[3]);
                segments.push(Seg::Curve(c1, c2, end));
                current = end;
                last_control = Some(c2);
                last_was_cubic = true;
                last_was_quadratic = false;
            }
            'Q' => {
                let values = take(&tokens, &mut index, 4)?;
                let control = point(values[0], values[1]);
                let end = point(values[2], values[3]);
                segments.push(quadratic_to_cubic(current, control, end));
                current = end;
                last_control = Some(control);
                last_was_cubic = false;
                last_was_quadratic = true;
            }
            'T' => {
                let values = take(&tokens, &mut index, 2)?;
                let control = if last_was_quadratic {
                    let previous = last_control.unwrap_or(current);
                    Point {
                        x: 2.0 * current.x - previous.x,
                        y: 2.0 * current.y - previous.y,
                    }
                } else {
                    current
                };
                let end = point(values[0], values[1]);
                segments.push(quadratic_to_cubic(current, control, end));
                current = end;
                last_control = Some(control);
                last_was_cubic = false;
                last_was_quadratic = true;
            }
            'Z' => {
                segments.push(Seg::Close);
                current = subpath_start;
                last_was_cubic = false;
                last_was_quadratic = false;
            }
            _ => {
                // Unknown command: skip it and stop trying to reuse it.
                command = None;
                continue;
            }
        }

        command = Some(command_character);
    }

    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

fn quadratic_to_cubic(start: Point, control: Point, end: Point) -> Seg {
    let c1 = Point {
        x: start.x + (control.x - start.x) * 2.0 / 3.0,
        y: start.y + (control.y - start.y) * 2.0 / 3.0,
    };
    let c2 = Point {
        x: end.x + (control.x - end.x) * 2.0 / 3.0,
        y: end.y + (control.y - end.y) * 2.0 / 3.0,
    };
    Seg::Curve(c1, c2, end)
}

pub fn polyline_length(points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|pair| distance(pair[0], pair[1]))
        .sum()
}

pub fn resample(points: &[Point], count: usize) -> Vec<Point> {
    if points.len() < 2 || count < 2 {
        return points.to_vec();
    }

    let total = polyline_length(points);
    if total <= f64::EPSILON {
        return vec![points[0]; count];
    }

    let step = total / (count - 1) as f64;
    let mut result = Vec::with_capacity(count);
    result.push(points[0]);

    let mut segment_start = points[0];
    let mut segment_end = points[1];
    let mut segment_index = 0;
    let mut segment_length = distance(segment_start, segment_end);
    let mut walked = 0.0;

    for sample in 1..count {
        let target = step * sample as f64;

        while walked + segment_length < target && segment_index + 1 < points.len() - 1 {
            walked += segment_length;
            segment_index += 1;
            segment_start = points[segment_index];
            segment_end = points[segment_index + 1];
            segment_length = distance(segment_start, segment_end);
        }

        let remaining = (target - walked).max(0.0);
        let ratio = if segment_length > f64::EPSILON {
            (remaining / segment_length).clamp(0.0, 1.0)
        } else {
            0.0
        };

        result.push(Point {
            x: segment_start.x + (segment_end.x - segment_start.x) * ratio,
            y: segment_start.y + (segment_end.y - segment_start.y) * ratio,
        });
    }

    // The last sample can drift a little because of floating point; snap it.
    if let Some(last) = result.last_mut() {
        *last = *points.last().unwrap();
    }
    result
}

pub fn prefix(points: &[Point], progress: f64) -> Vec<Point> {
    if points.is_empty() {
        return Vec::new();
    }

    let progress = progress.clamp(0.0, 1.0);
    if progress >= 1.0 {
        return points.to_vec();
    }

    let total = polyline_length(points);
    if total <= f64::EPSILON {
        return vec![points[0]];
    }

    let target = total * progress;
    let mut result = vec![points[0]];
    let mut walked = 0.0;

    for pair in points.windows(2) {
        let segment = distance(pair[0], pair[1]);
        if walked + segment >= target {
            let ratio = if segment > f64::EPSILON {
                ((target - walked) / segment).clamp(0.0, 1.0)
            } else {
                0.0
            };
            result.push(Point {
                x: pair[0].x + (pair[1].x - pair[0].x) * ratio,
                y: pair[0].y + (pair[1].y - pair[0].y) * ratio,
            });
            return result;
        }
        walked += segment;
        result.push(pair[1]);
    }

    result
}

/// Compare one user-drawn stroke with the expected median polyline.
///
/// Coordinates are in the 1024-unit box, so the thresholds are scale-independent.
pub fn stroke_matches(input: &[Point], expected: &[Point]) -> bool {
    if input.len() < 2 || expected.len() < 2 {
        return false;
    }

    let expected_length = polyline_length(expected);
    let input_length = polyline_length(input);
    if expected_length <= f64::EPSILON {
        return false;
    }

    let length_ratio = input_length / expected_length;
    if !(0.45..=2.4).contains(&length_ratio) {
        return false;
    }

    let input_samples = resample(input, 12);
    let expected_samples = resample(expected, 12);

    let forward: f64 = input_samples
        .iter()
        .zip(&expected_samples)
        .map(|(a, b)| distance(*a, *b))
        .sum::<f64>()
        / input_samples.len() as f64;

    let reversed: f64 = input_samples
        .iter()
        .zip(expected_samples.iter().rev())
        .map(|(a, b)| distance(*a, *b))
        .sum::<f64>()
        / input_samples.len() as f64;

    if reversed < forward {
        // The stroke was drawn in the wrong direction.
        return false;
    }

    let start = distance(input_samples[0], expected_samples[0]);
    let end = distance(
        input_samples[input_samples.len() - 1],
        expected_samples[expected_samples.len() - 1],
    );

    forward < 140.0 && start < 190.0 && end < 190.0
}

pub fn distance(a: Point, b: Point) -> f64 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line() -> Vec<Point> {
        vec![
            Point { x: 100.0, y: 100.0 },
            Point { x: 200.0, y: 100.0 },
            Point { x: 300.0, y: 100.0 },
        ]
    }

    #[test]
    fn parses_quadratic_and_close() {
        let segments = parse_path("M 0 0 L 10 0 Q 20 10 30 0 Z").expect("valid path");
        assert_eq!(segments.len(), 4);
        assert!(matches!(segments[0], Seg::Move(_)));
        assert!(matches!(segments[1], Seg::Line(_)));
        assert!(matches!(segments[2], Seg::Curve(_, _, _)));
        assert!(matches!(segments[3], Seg::Close));
    }

    #[test]
    fn parses_relative_commands() {
        let segments = parse_path("m 10 10 l 10 0 h 5 v 5 z").expect("valid path");
        let Seg::Move(start) = segments[0] else {
            panic!("expected move");
        };
        assert!((start.x - 10.0).abs() < 1e-9);
        let Seg::Line(first) = segments[1] else {
            panic!("expected line");
        };
        assert!((first.x - 20.0).abs() < 1e-9);
    }

    #[test]
    fn stroke_accepts_its_own_median() {
        assert!(stroke_matches(&line(), &line()));
    }

    #[test]
    fn stroke_rejects_translation_and_reversal() {
        let shifted: Vec<_> = line()
            .into_iter()
            .map(|point| Point {
                x: point.x + 250.0,
                y: point.y,
            })
            .collect();
        assert!(!stroke_matches(&shifted, &line()));

        let reversed: Vec<_> = line().into_iter().rev().collect();
        assert!(!stroke_matches(&reversed, &line()));
    }

    #[test]
    fn transform_round_trips() {
        let transform = ViewTransform::new(420.0, 420.0, 12.0);
        let point = Point { x: 341.0, y: 523.0 };
        let screen_x = transform.offset_x + transform.scale * point.x;
        let screen_y = transform.offset_y + transform.scale * (900.0 - point.y);
        let back = transform.to_data(screen_x, screen_y);
        assert!((back.x - point.x).abs() < 1e-9);
        assert!((back.y - point.y).abs() < 1e-9);
    }
}
