//! The scale policy of X11 windows, as plain data.
//!
//! X11 has no standard scale property. A monitor has the scale of its
//! physical RandR DPI. A monitor without a usable physical size has the
//! scale of `Xft.dpi` from the resource database, else 1. A window has the
//! scale of the monitor that holds the largest part of it.

use choscorui_core::{Rect, Size};

/// The DPI of scale 1.
const BASE_DPI: f64 = 96.0;
/// A physical scale is a multiple of this step.
const STEP: f64 = 0.25;
/// Above this DPI, the physical size of a monitor is taken as wrong,
/// as the aspect-ratio sizes of some projectors are.
const MAX_DPI: f64 = 480.0;
const MILLIMETERS_PER_INCH: f64 = 25.4;

/// A RandR monitor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Monitor {
    /// The rectangle in root pixels.
    pub(crate) rect: Rect,
    /// The physical width and height in millimeters, 0 when unknown.
    pub(crate) millimeters: (u32, u32),
    pub(crate) primary: bool,
}

/// The scale of the physical DPI of `monitor`, or None when the monitor
/// gives no usable physical size.
///
/// The DPI is the width in pixels over the width in inches. The scale
/// is the DPI over 96, rounded to the nearest multiple of 0.25, and at
/// least 1. A DPI above 480 is not usable.
pub(crate) fn physical_scale(monitor: &Monitor) -> Option<f64> {
    let (width, height) = monitor.millimeters;
    if width == 0 || height == 0 {
        return None;
    }
    let dpi = monitor.rect.size.width * MILLIMETERS_PER_INCH / f64::from(width);
    if !dpi.is_finite() || dpi <= 0.0 || dpi > MAX_DPI {
        return None;
    }
    Some(((dpi / BASE_DPI / STEP).round() * STEP).max(1.0))
}

/// The scale of `monitor`: its physical scale, else `xft`, the scale of
/// `Xft.dpi`, else 1.
pub(crate) fn monitor_scale(xft: Option<f64>, monitor: &Monitor) -> f64 {
    physical_scale(monitor).or(xft).unwrap_or(1.0)
}

/// The monitor that holds the largest part of `window`, in root pixels.
/// The first one wins a tie. When no monitor holds a part of it, the
/// primary monitor applies, else the first one.
pub(crate) fn monitor_of(window: Rect, monitors: &[Monitor]) -> Option<&Monitor> {
    let area = |monitor: &Monitor| {
        monitor
            .rect
            .intersection(window)
            .map_or(0.0, |part| part.size.width * part.size.height)
    };
    let mut best: Option<(&Monitor, f64)> = None;
    for monitor in monitors {
        let overlap = area(monitor);
        if overlap > 0.0 && best.is_none_or(|(_, largest)| overlap > largest) {
            best = Some((monitor, overlap));
        }
    }
    best.map(|(monitor, _)| monitor)
        .or_else(|| monitors.iter().find(|monitor| monitor.primary))
        .or_else(|| monitors.first())
}

/// The scale of a new window, before it has a position: the scale of the
/// primary monitor, else of the first one, else 1.
pub(crate) fn initial_scale(xft: Option<f64>, monitors: &[Monitor]) -> f64 {
    monitor_of(Rect::from_xywh(0.0, 0.0, 0.0, 0.0), monitors)
        .map_or_else(|| xft.unwrap_or(1.0), |monitor| monitor_scale(xft, monitor))
}

/// The scale of a window at `window` in root pixels, which has the scale
/// `current` now.
///
/// The window keeps its logical size, so a new scale also changes its
/// pixel size and thus its monitor. The new scale applies only when the
/// monitor of the resized window has the same scale. Otherwise the
/// window keeps `current`, so the scale does not change back and forth.
pub(crate) fn window_scale(
    xft: Option<f64>,
    monitors: &[Monitor],
    window: Rect,
    current: f64,
) -> f64 {
    let scale_at = |rect: Rect| {
        monitor_of(rect, monitors).map_or(xft.unwrap_or(1.0), |monitor| monitor_scale(xft, monitor))
    };
    let target = scale_at(window);
    if target == current || current <= 0.0 || !current.is_finite() {
        return target;
    }
    let factor = target / current;
    let resized = Rect::new(
        window.origin,
        Size::new(window.size.width * factor, window.size.height * factor),
    );
    if scale_at(resized) == target {
        target
    } else {
        current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(x: f64, width: f64, millimeters: (u32, u32)) -> Monitor {
        Monitor {
            rect: Rect::from_xywh(x, 0.0, width, 1000.0),
            millimeters,
            primary: false,
        }
    }

    #[test]
    fn physical_dpi_rounds_to_quarter_steps() {
        // 1280 pixels on 339 mm is 95.9 DPI.
        assert_eq!(physical_scale(&monitor(0.0, 1280.0, (339, 212))), Some(1.0));
        // 3840 pixels on 597 mm is 163 DPI: 1.70, so 1.75.
        assert_eq!(
            physical_scale(&monitor(0.0, 3840.0, (597, 336))),
            Some(1.75)
        );
        // 2560 pixels on 339 mm is 192 DPI.
        assert_eq!(physical_scale(&monitor(0.0, 2560.0, (339, 212))), Some(2.0));
        // 108 DPI is 1.125, halfway, and it rounds away from zero.
        assert_eq!(physical_scale(&monitor(0.0, 1080.0, (254, 1))), Some(1.25));
    }

    #[test]
    fn low_dpi_gives_one() {
        // 1024 pixels on 400 mm is 65 DPI.
        assert_eq!(physical_scale(&monitor(0.0, 1024.0, (400, 300))), Some(1.0));
    }

    #[test]
    fn unknown_or_wrong_sizes_give_no_physical_scale() {
        assert_eq!(physical_scale(&monitor(0.0, 1920.0, (0, 0))), None);
        assert_eq!(physical_scale(&monitor(0.0, 1920.0, (527, 0))), None);
        // A projector that gives its aspect ratio as 16 by 9 mm.
        assert_eq!(physical_scale(&monitor(0.0, 1920.0, (16, 9))), None);
        assert_eq!(monitor_scale(None, &monitor(0.0, 1920.0, (16, 9))), 1.0);
    }

    #[test]
    fn the_physical_size_wins_over_xft_dpi() {
        let high = monitor(0.0, 2560.0, (339, 212));
        assert_eq!(monitor_scale(Some(1.5), &high), 2.0);
        assert_eq!(monitor_scale(None, &high), 2.0);
        let monitors = [high, monitor(2560.0, 1280.0, (339, 212))];
        let window = Rect::from_xywh(3000.0, 0.0, 400.0, 300.0);
        assert_eq!(window_scale(Some(1.5), &monitors, window, 2.0), 1.0);
        assert_eq!(window_scale(None, &monitors, window, 2.0), 1.0);
    }

    #[test]
    fn xft_dpi_applies_without_a_usable_physical_size() {
        assert_eq!(monitor_scale(Some(1.5), &monitor(0.0, 1920.0, (0, 0))), 1.5);
        // A projector that gives its aspect ratio as 16 by 9 mm.
        let projector = monitor(0.0, 1920.0, (16, 9));
        assert_eq!(monitor_scale(Some(1.25), &projector), 1.25);
        let monitors = [projector, monitor(1920.0, 2560.0, (339, 212))];
        let window = Rect::from_xywh(100.0, 0.0, 400.0, 300.0);
        assert_eq!(window_scale(Some(1.25), &monitors, window, 1.0), 1.25);
    }

    #[test]
    fn the_largest_overlap_selects_the_monitor() {
        let monitors = [
            monitor(0.0, 1000.0, (0, 0)),
            monitor(1000.0, 1000.0, (0, 0)),
        ];
        let mostly_right = Rect::from_xywh(900.0, 0.0, 400.0, 100.0);
        assert_eq!(monitor_of(mostly_right, &monitors), Some(&monitors[1]));
        let mostly_left = Rect::from_xywh(700.0, 0.0, 400.0, 100.0);
        assert_eq!(monitor_of(mostly_left, &monitors), Some(&monitors[0]));
        // A tie goes to the first monitor.
        let middle = Rect::from_xywh(800.0, 0.0, 400.0, 100.0);
        assert_eq!(monitor_of(middle, &monitors), Some(&monitors[0]));
    }

    #[test]
    fn a_window_off_every_monitor_uses_the_primary_one() {
        let mut monitors = [
            monitor(0.0, 1000.0, (0, 0)),
            monitor(1000.0, 1000.0, (0, 0)),
        ];
        let away = Rect::from_xywh(5000.0, 0.0, 10.0, 10.0);
        assert_eq!(monitor_of(away, &monitors), Some(&monitors[0]));
        monitors[1].primary = true;
        assert_eq!(monitor_of(away, &monitors), Some(&monitors[1]));
        assert_eq!(monitor_of(away, &[]), None);
    }

    #[test]
    fn a_new_window_takes_the_primary_scale() {
        let mut high = monitor(1280.0, 2560.0, (339, 212));
        high.primary = true;
        let monitors = [monitor(0.0, 1280.0, (339, 212)), high];
        assert_eq!(initial_scale(None, &monitors), 2.0);
        assert_eq!(initial_scale(Some(1.25), &monitors), 2.0);
        assert_eq!(initial_scale(None, &[]), 1.0);
        assert_eq!(initial_scale(Some(1.5), &[]), 1.5);
    }

    #[test]
    fn a_move_to_another_monitor_changes_the_scale() {
        // A 96 DPI monitor, and a 192 DPI monitor on its right.
        let monitors = [
            monitor(0.0, 1280.0, (339, 212)),
            monitor(1280.0, 2560.0, (339, 212)),
        ];
        let window = Rect::from_xywh(100.0, 0.0, 400.0, 300.0);
        assert_eq!(window_scale(None, &monitors, window, 1.0), 1.0);
        let moved = Rect::from_xywh(2000.0, 0.0, 400.0, 300.0);
        assert_eq!(window_scale(None, &monitors, moved, 1.0), 2.0);
        let back = Rect::from_xywh(100.0, 0.0, 800.0, 600.0);
        assert_eq!(window_scale(None, &monitors, back, 2.0), 1.0);
    }

    #[test]
    fn a_window_on_the_edge_keeps_its_scale() {
        // A 192 DPI monitor, and a 96 DPI monitor on its right. At scale
        // 2, the window is mostly on the right monitor. At scale 1, it
        // would be mostly on the left monitor again.
        let monitors = [
            monitor(0.0, 1000.0, (127, 100)),
            monitor(1000.0, 1000.0, (254, 100)),
        ];
        let window = Rect::from_xywh(900.0, 0.0, 300.0, 100.0);
        assert_eq!(window_scale(None, &monitors, window, 2.0), 2.0);
        // At scale 1, the same logical size keeps scale 1 for the same
        // reason.
        let half = Rect::from_xywh(900.0, 0.0, 150.0, 50.0);
        assert_eq!(window_scale(None, &monitors, half, 1.0), 1.0);
    }
}
