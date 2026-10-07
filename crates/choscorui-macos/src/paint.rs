use super::*;

/// Paints a logical top-left display list into a context already configured
/// with top-left, y-down coordinates. Always balances graphics state.
pub(super) unsafe fn paint_commands(context: Id, dirty: NSRect, list: &DisplayList) -> bool {
    unsafe {
        CGContextSaveGState(context);
        CGContextClipToRect(context, dirty);
        let mut clip_depth = 0;
        let mut opacity_depth = 0;
        let mut opacity_stack = vec![1.0];
        let mut success = true;
        for command in list.commands() {
            match command {
                DisplayCommand::FillRect { rect, color } => {
                    set_fill_color(context, *color);
                    CGContextFillRect(context, native_rect(*rect));
                }
                DisplayCommand::StrokeRect { rect, color, width } => {
                    set_stroke_color(context, *color);
                    CGContextStrokeRectWithWidth(context, native_rect(*rect), *width);
                }
                DisplayCommand::FillRoundedRect {
                    rect,
                    radius,
                    color,
                } => {
                    set_fill_color(context, *color);
                    rounded_rect_path(context, *rect, *radius);
                    CGContextFillPath(context);
                }
                DisplayCommand::StrokeRoundedRect {
                    rect,
                    radius,
                    color,
                    width,
                } => {
                    set_stroke_color(context, *color);
                    CGContextSetLineWidth(context, *width);
                    rounded_rect_path(context, *rect, *radius);
                    CGContextStrokePath(context);
                }
                DisplayCommand::DrawText {
                    baseline,
                    text,
                    font_family,
                    font_size,
                    color,
                } => {
                    if !draw_text_run(context, *baseline, text, font_family, *font_size, *color) {
                        success = false;
                        break;
                    }
                }
                DisplayCommand::PushClip(rect) => {
                    CGContextSaveGState(context);
                    CGContextClipToRect(context, native_rect(*rect));
                    clip_depth += 1;
                }
                DisplayCommand::PopClip if clip_depth > 0 => {
                    CGContextRestoreGState(context);
                    clip_depth -= 1;
                }
                DisplayCommand::PopClip => {}
                DisplayCommand::PushOpacity(opacity) => {
                    CGContextSaveGState(context);
                    let combined = opacity_stack.last().copied().unwrap_or(1.0) * opacity;
                    opacity_stack.push(combined);
                    CGContextSetAlpha(context, combined);
                    opacity_depth += 1;
                }
                DisplayCommand::PopOpacity if opacity_depth > 0 => {
                    CGContextRestoreGState(context);
                    opacity_stack.pop();
                    opacity_depth -= 1;
                }
                DisplayCommand::PopOpacity => {}
            }
        }
        for _ in 0..clip_depth + opacity_depth {
            CGContextRestoreGState(context);
        }
        CGContextRestoreGState(context);
        success
    }
}

unsafe fn set_fill_color(context: Id, color: Color) {
    unsafe {
        CGContextSetRGBFillColor(
            context,
            channel(color.red),
            channel(color.green),
            channel(color.blue),
            channel(color.alpha),
        );
    }
}

unsafe fn set_stroke_color(context: Id, color: Color) {
    unsafe {
        CGContextSetRGBStrokeColor(
            context,
            channel(color.red),
            channel(color.green),
            channel(color.blue),
            channel(color.alpha),
        );
    }
}

fn channel(value: u8) -> f64 {
    f64::from(value) / 255.0
}

unsafe fn rounded_rect_path(context: Id, rect: Rect, radius: f64) {
    unsafe {
        let left = rect.origin.x;
        let top = rect.origin.y;
        let right = left + rect.size.width;
        let bottom = top + rect.size.height;
        let radius = radius
            .min(rect.size.width / 2.0)
            .min(rect.size.height / 2.0)
            .max(0.0);
        CGContextBeginPath(context);
        CGContextMoveToPoint(context, left + radius, top);
        CGContextAddArcToPoint(context, right, top, right, bottom, radius);
        CGContextAddArcToPoint(context, right, bottom, left, bottom, radius);
        CGContextAddArcToPoint(context, left, bottom, left, top, radius);
        CGContextAddArcToPoint(context, left, top, right, top, radius);
        CGContextClosePath(context);
    }
}
