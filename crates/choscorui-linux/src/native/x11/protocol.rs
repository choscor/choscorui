//! The parts of the X protocol that are plain data: replies, property
//! values, client messages, and the scroll valuators of XInput 2.

use crate::ffi::x11::{
    Bytes, XCB_CLIENT_MESSAGE, XCB_INPUT_DEVICE_CLASS_TYPE_SCROLL,
    XCB_INPUT_DEVICE_CLASS_TYPE_VALUATOR, XCB_INPUT_SCROLL_TYPE_VERTICAL,
    XCB_RENDER_PICT_TYPE_DIRECT,
};
use crate::native::types::ResizeEdge;
use choscorui_core::{Point, PointerButton, Rect, Size};
use std::collections::HashMap;

/// One scroll increment of XInput 2, which is one wheel detent, in
/// points. It is the axis value that Wayland compositors send for one
/// detent through libinput, so both display servers scroll alike.
pub(crate) const POINTS_PER_SCROLL_INCREMENT: f64 = 15.0;

/// The ICCCM `WM_SIZE_HINTS` flags.
const US_POSITION: u32 = 1;
const US_SIZE: u32 = 2;
const P_MIN_SIZE: u32 = 16;
const P_WIN_GRAVITY: u32 = 512;
/// `NorthWestGravity`.
const NORTH_WEST: u32 = 1;

/// The scale of `Xft.dpi` in the resource database text: the DPI over
/// 96, or 1 without a valid value.
pub(crate) fn xft_scale(resources: &str) -> f64 {
    resources
        .lines()
        .find_map(|line| line.strip_prefix("Xft.dpi:"))
        .and_then(|value| value.trim().parse::<f64>().ok())
        .filter(|dpi| dpi.is_finite() && *dpi > 0.0)
        .map_or(1.0, |dpi| dpi / 96.0)
}

/// The rectangles of the monitors of an `RRGetMonitors` reply, in
/// pixels.
pub(crate) fn monitors(reply: &Bytes) -> Vec<Rect> {
    let count = reply.u32(12);
    let mut offset = 32;
    let mut rects = Vec::new();
    for _ in 0..count {
        let outputs = usize::from(reply.u16(offset + 6));
        let (width, height) = (reply.u16(offset + 12), reply.u16(offset + 14));
        if width > 0 && height > 0 {
            rects.push(Rect::new(
                Point::new(
                    f64::from(reply.i16(offset + 8)),
                    f64::from(reply.i16(offset + 10)),
                ),
                Size::new(f64::from(width), f64::from(height)),
            ));
        }
        offset += 24 + outputs * 4;
    }
    rects
}

/// The origin of a popup of `size` next to `anchor`, kept on the
/// monitor of `monitors` that holds the center of `anchor`, or else on
/// the first one. All values are root pixels.
///
/// As the Wayland positioner of the backend asks: the popup opens below
/// the bottom-left corner of `anchor`. When it does not fit, it flips
/// above `anchor` or to the left of its right edge, if the flipped
/// popup fits. Then it slides onto the monitor.
pub(crate) fn place_popup(anchor: Rect, size: Size, monitors: &[Rect]) -> Point {
    let center = anchor.center();
    let monitor = monitors
        .iter()
        .find(|monitor| {
            (monitor.min_x()..monitor.max_x()).contains(&center.x)
                && (monitor.min_y()..monitor.max_y()).contains(&center.y)
        })
        .or_else(|| monitors.first())
        .copied();
    let x = anchor.min_x();
    let y = anchor.max_y();
    let Some(monitor) = monitor else {
        return Point::new(x, y);
    };
    let axis = |start: f64, flipped: f64, length: f64, (low, high): (f64, f64)| {
        let start = if start + length > high && flipped >= low {
            flipped
        } else {
            start
        };
        // The slide keeps the start on the monitor when the popup is
        // longer than it.
        start.min(high - length).max(low)
    };
    Point::new(
        axis(
            x,
            anchor.max_x() - size.width,
            size.width,
            (monitor.min_x(), monitor.max_x()),
        ),
        axis(
            y,
            anchor.min_y() - size.height,
            size.height,
            (monitor.min_y(), monitor.max_y()),
        ),
    )
}

/// The id of the ARGB32 picture format of a `RenderQueryPictFormats`
/// reply: 32 bits, alpha at bit 24, red at 16, green at 8, blue at 0.
pub(crate) fn argb32_format(reply: &Bytes) -> Option<u32> {
    let count = reply.u32(8) as usize;
    (0..count).map(|index| 32 + index * 28).find_map(|at| {
        let direct = reply.u8(at + 4) == XCB_RENDER_PICT_TYPE_DIRECT && reply.u8(at + 5) == 32;
        // Each channel is a shift and an 8-bit mask, starting with red.
        let channels = [(at + 8, 16), (at + 12, 8), (at + 16, 0), (at + 20, 24)]
            .iter()
            .all(|&(offset, shift)| reply.u16(offset) == shift && reply.u16(offset + 2) == 0xff);
        (direct && channels).then(|| reply.u32(at))
    })
}

/// The `WM_NORMAL_HINTS` value: the minimum size in pixels, north-west
/// gravity, and, after the application placed the window, the
/// user-specified position and size.
pub(crate) fn size_hints(minimum: (u32, u32), placed: bool) -> [u32; 18] {
    let mut hints = [0u32; 18];
    hints[0] = P_MIN_SIZE | P_WIN_GRAVITY | if placed { US_POSITION | US_SIZE } else { 0 };
    hints[5] = minimum.0;
    hints[6] = minimum.1;
    hints[17] = NORTH_WEST;
    hints
}

/// A 32-bit client message about `window`.
pub(crate) fn client_message(window: u32, kind: u32, data: [u32; 5]) -> [u8; 32] {
    let mut event = [0u8; 32];
    event[0] = XCB_CLIENT_MESSAGE;
    event[1] = 32;
    event[4..8].copy_from_slice(&window.to_ne_bytes());
    event[8..12].copy_from_slice(&kind.to_ne_bytes());
    for (index, value) in data.iter().enumerate() {
        let at = 12 + index * 4;
        event[at..at + 4].copy_from_slice(&value.to_ne_bytes());
    }
    event
}

/// The `_NET_WM_MOVERESIZE` direction of an edge.
pub(crate) fn moveresize_direction(edge: ResizeEdge) -> u32 {
    match edge {
        ResizeEdge::TopLeft => 0,
        ResizeEdge::Top => 1,
        ResizeEdge::TopRight => 2,
        ResizeEdge::Right => 3,
        ResizeEdge::BottomRight => 4,
        ResizeEdge::Bottom => 5,
        ResizeEdge::BottomLeft => 6,
        ResizeEdge::Left => 7,
    }
}

/// The `_NET_WM_MOVERESIZE` direction of a move.
pub(crate) const MOVERESIZE_MOVE: u32 = 8;

/// What an X pointer button does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Button {
    Press(PointerButton),
    /// A wheel detent of buttons 4 to 7, in lines: positive scrolls
    /// down and right.
    Wheel(Point),
}

/// The meaning of X button `number`: 1, 2, and 3 are primary, middle,
/// and secondary; 4 to 7 scroll; 8 and up follow macOS numbering from 3.
pub(crate) fn button(number: u32) -> Button {
    match number {
        1 => Button::Press(PointerButton::Primary),
        2 => Button::Press(PointerButton::Middle),
        3 => Button::Press(PointerButton::Secondary),
        4 => Button::Wheel(Point::new(0.0, -1.0)),
        5 => Button::Wheel(Point::new(0.0, 1.0)),
        6 => Button::Wheel(Point::new(-1.0, 0.0)),
        7 => Button::Wheel(Point::new(1.0, 0.0)),
        number => Button::Press(PointerButton::Other(
            usize::try_from(number - 5).unwrap_or(3),
        )),
    }
}

/// A scroll valuator of a device.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollClass {
    number: u16,
    vertical: bool,
    increment: f64,
}

/// The scroll valuators of the XInput 2 devices and their last values.
#[derive(Debug, Default)]
pub(crate) struct Scrolling {
    classes: HashMap<u16, Vec<ScrollClass>>,
    values: HashMap<(u16, u16), f64>,
}

impl Scrolling {
    /// Reads the scroll classes and the valuator values of an
    /// `XIQueryDevice` reply.
    pub(crate) fn from_reply(reply: &Bytes) -> Self {
        let mut scrolling = Self::default();
        let count = reply.u16(8);
        let mut at = 32;
        for _ in 0..count {
            let device = reply.u16(at);
            let classes = reply.u16(at + 6);
            let name = usize::from(reply.u16(at + 8));
            at += 12 + name.div_ceil(4) * 4;
            let mut values = Vec::new();
            for _ in 0..classes {
                let kind = reply.u16(at);
                let length = usize::from(reply.u16(at + 2)) * 4;
                if length == 0 {
                    return scrolling;
                }
                let number = reply.u16(at + 6);
                if kind == XCB_INPUT_DEVICE_CLASS_TYPE_SCROLL {
                    let increment = reply.fp3232(at + 16);
                    if increment != 0.0 {
                        scrolling
                            .classes
                            .entry(device)
                            .or_default()
                            .push(ScrollClass {
                                number,
                                vertical: reply.u16(at + 8) == XCB_INPUT_SCROLL_TYPE_VERTICAL,
                                increment,
                            });
                    }
                } else if kind == XCB_INPUT_DEVICE_CLASS_TYPE_VALUATOR {
                    values.push((number, reply.fp3232(at + 28)));
                }
                at += length;
            }
            for (number, value) in values {
                scrolling.values.insert((device, number), value);
            }
        }
        scrolling
    }

    /// The scroll of the valuators of a motion event of `device`, in
    /// increments: positive scrolls down and right. None when no scroll
    /// valuator moved.
    pub(crate) fn motion(&mut self, device: u16, valuators: &[(u16, f64)]) -> Option<Point> {
        let classes = self.classes.get(&device)?;
        let mut delta = Point::new(0.0, 0.0);
        for &(number, value) in valuators {
            let Some(class) = classes.iter().find(|class| class.number == number) else {
                continue;
            };
            // The first value after a reset only sets the start.
            if let Some(last) = self.values.insert((device, number), value) {
                let steps = (value - last) / class.increment;
                if class.vertical {
                    delta.y += steps;
                } else {
                    delta.x += steps;
                }
            }
        }
        (delta.x != 0.0 || delta.y != 0.0).then_some(delta)
    }
}

/// The valuators of an XInput 2 device event: the number and the value
/// of each bit of the valuator mask.
pub(crate) fn valuators(event: &Bytes) -> Vec<(u16, f64)> {
    let buttons = usize::from(event.u16(48)) * 4;
    let words = usize::from(event.u16(50));
    let mask = 80 + buttons;
    let mut values = mask + words * 4;
    let mut result = Vec::new();
    for word in 0..words {
        let bits = event.u32(mask + word * 4);
        for bit in 0..32 {
            if bits & (1 << bit) != 0 {
                let number = u16::try_from(word * 32 + bit).unwrap_or(u16::MAX);
                result.push((number, event.fp3232(values)));
                values += 8;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(bytes: &mut Vec<u8>, at: usize, value: &[u8]) {
        if bytes.len() < at + value.len() {
            bytes.resize(at + value.len(), 0);
        }
        bytes[at..at + value.len()].copy_from_slice(value);
    }

    #[test]
    fn xft_dpi_sets_the_scale() {
        assert_eq!(xft_scale("Xft.antialias:\t1\nXft.dpi:\t144\n"), 1.5);
        assert_eq!(xft_scale("Xft.dpi: none"), 1.0);
        assert_eq!(xft_scale(""), 1.0);
    }

    #[test]
    fn monitors_skip_their_outputs() {
        let mut reply = vec![0u8; 32];
        put(&mut reply, 12, &2u32.to_ne_bytes());
        // The first monitor has one output.
        put(&mut reply, 32 + 6, &1u16.to_ne_bytes());
        put(&mut reply, 32 + 12, &1280u16.to_ne_bytes());
        put(&mut reply, 32 + 14, &800u16.to_ne_bytes());
        // The second follows the 24 bytes and the output of the first.
        put(&mut reply, 60 + 8, &1280i16.to_ne_bytes());
        put(&mut reply, 60 + 12, &640u16.to_ne_bytes());
        put(&mut reply, 60 + 14, &480u16.to_ne_bytes());
        put(&mut reply, 60 + 23, &[0]);
        assert_eq!(
            monitors(&Bytes::from(reply)),
            vec![
                Rect::new(Point::new(0.0, 0.0), Size::new(1280.0, 800.0)),
                Rect::new(Point::new(1280.0, 0.0), Size::new(640.0, 480.0)),
            ]
        );
    }

    #[test]
    fn popups_open_below_the_anchor_then_flip_and_slide() {
        let screens = [
            Rect::from_xywh(0.0, 0.0, 1280.0, 800.0),
            Rect::from_xywh(1280.0, 0.0, 640.0, 480.0),
        ];
        let size = Size::new(200.0, 300.0);
        let anchor = |x, y| Rect::from_xywh(x, y, 1.0, 1.0);
        // Room below and to the right.
        assert_eq!(
            place_popup(anchor(100.0, 50.0), size, &screens),
            Point::new(100.0, 51.0)
        );
        // At the bottom edge, the popup flips above the anchor.
        assert_eq!(
            place_popup(anchor(100.0, 700.0), size, &screens),
            Point::new(100.0, 400.0)
        );
        // At the right edge of the first monitor, it flips to the left.
        assert_eq!(
            place_popup(anchor(1200.0, 50.0), size, &screens),
            Point::new(1001.0, 51.0)
        );
        // The second monitor is shorter: neither side fits, so the popup
        // slides up onto it.
        assert_eq!(
            place_popup(anchor(1300.0, 250.0), size, &screens),
            Point::new(1300.0, 180.0)
        );
        // A popup wider than the anchor's side flips only when the flip
        // fits; else it slides.
        assert_eq!(
            place_popup(anchor(50.0, 50.0), Size::new(1300.0, 10.0), &screens),
            Point::new(0.0, 51.0)
        );
        // Without monitors, the popup stays below the anchor.
        assert_eq!(
            place_popup(anchor(5.0, 6.0), size, &[]),
            Point::new(5.0, 7.0)
        );
    }

    #[test]
    fn the_argb32_format_is_found_by_its_channels() {
        let mut reply = vec![0u8; 32];
        put(&mut reply, 8, &2u32.to_ne_bytes());
        // An RGB24 format first.
        put(&mut reply, 32, &7u32.to_ne_bytes());
        put(&mut reply, 36, &[1, 24]);
        // Then ARGB32.
        let at = 60;
        put(&mut reply, at, &9u32.to_ne_bytes());
        put(&mut reply, at + 4, &[1, 32]);
        for (offset, shift) in [(8, 16u16), (12, 8), (16, 0), (20, 24)] {
            put(&mut reply, at + offset, &shift.to_ne_bytes());
            put(&mut reply, at + offset + 2, &0xffu16.to_ne_bytes());
        }
        put(&mut reply, at + 27, &[0]);
        assert_eq!(argb32_format(&Bytes::from(reply)), Some(9));
    }

    #[test]
    fn hints_hold_the_minimum_and_the_gravity() {
        let hints = size_hints((200, 100), false);
        assert_eq!(hints[0], P_MIN_SIZE | P_WIN_GRAVITY);
        assert_eq!((hints[5], hints[6], hints[17]), (200, 100, NORTH_WEST));
        assert_eq!(size_hints((1, 1), true)[0] & US_POSITION, US_POSITION);
    }

    #[test]
    fn buttons_map_to_presses_and_wheel_lines() {
        assert_eq!(button(1), Button::Press(PointerButton::Primary));
        assert_eq!(button(2), Button::Press(PointerButton::Middle));
        assert_eq!(button(3), Button::Press(PointerButton::Secondary));
        assert_eq!(button(5), Button::Wheel(Point::new(0.0, 1.0)));
        assert_eq!(button(8), Button::Press(PointerButton::Other(3)));
    }

    #[test]
    fn client_messages_hold_their_data() {
        let event = client_message(5, 7, [1, 2, 3, 4, 5]);
        let bytes = Bytes::from(event.to_vec());
        assert_eq!((bytes.u8(0), bytes.u8(1)), (XCB_CLIENT_MESSAGE, 32));
        assert_eq!((bytes.u32(4), bytes.u32(8), bytes.u32(28)), (5, 7, 5));
    }

    /// An `XIQueryDevice` reply with one device: a valuator and a
    /// vertical scroll class for valuator 3, with increment 120.
    fn device_reply() -> Bytes {
        let mut reply = vec![0u8; 32];
        put(&mut reply, 8, &1u16.to_ne_bytes());
        let at = 32;
        put(&mut reply, at, &2u16.to_ne_bytes());
        put(&mut reply, at + 6, &2u16.to_ne_bytes());
        put(&mut reply, at + 8, &5u16.to_ne_bytes());
        put(&mut reply, at + 12, b"mouse\0\0\0");
        let valuator = at + 20;
        put(
            &mut reply,
            valuator,
            &XCB_INPUT_DEVICE_CLASS_TYPE_VALUATOR.to_ne_bytes(),
        );
        put(&mut reply, valuator + 2, &11u16.to_ne_bytes());
        put(&mut reply, valuator + 6, &3u16.to_ne_bytes());
        put(&mut reply, valuator + 28, &240i32.to_ne_bytes());
        let scroll = valuator + 44;
        put(
            &mut reply,
            scroll,
            &XCB_INPUT_DEVICE_CLASS_TYPE_SCROLL.to_ne_bytes(),
        );
        put(&mut reply, scroll + 2, &6u16.to_ne_bytes());
        put(&mut reply, scroll + 6, &3u16.to_ne_bytes());
        put(
            &mut reply,
            scroll + 8,
            &XCB_INPUT_SCROLL_TYPE_VERTICAL.to_ne_bytes(),
        );
        put(&mut reply, scroll + 16, &120i32.to_ne_bytes());
        put(&mut reply, scroll + 23, &[0]);
        Bytes::from(reply)
    }

    #[test]
    fn scroll_valuators_turn_into_increments() {
        let mut scrolling = Scrolling::from_reply(&device_reply());
        assert_eq!(scrolling.motion(3, &[(3, 360.0)]), None);
        assert_eq!(
            scrolling.motion(2, &[(3, 360.0)]),
            Some(Point::new(0.0, 1.0))
        );
        assert_eq!(
            scrolling.motion(2, &[(3, 300.0)]),
            Some(Point::new(0.0, -0.5))
        );
        // Valuators that do not scroll are ignored.
        assert_eq!(scrolling.motion(2, &[(0, 10.0)]), None);
    }

    #[test]
    fn valuators_follow_the_mask() {
        let mut event = vec![0u8; 80];
        put(&mut event, 48, &1u16.to_ne_bytes());
        put(&mut event, 50, &1u16.to_ne_bytes());
        // Valuators 1 and 3 are set.
        put(&mut event, 84, &0b1010u32.to_ne_bytes());
        put(&mut event, 88, &5i32.to_ne_bytes());
        put(&mut event, 92, &0x8000_0000u32.to_ne_bytes());
        put(&mut event, 96, &(-2i32).to_ne_bytes());
        put(&mut event, 100, &0u32.to_ne_bytes());
        assert_eq!(valuators(&Bytes::from(event)), vec![(1, 5.5), (3, -2.0)]);
    }
}
