use crate::{Point, Rect};

/// Straight-alpha sRGB color, stored as eight-bit channels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Color {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Color {
    pub const fn rgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }
}

/// Renderer-neutral commands in logical points, emitted in painter's order.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayCommand {
    FillRect {
        rect: Rect,
        color: Color,
    },
    StrokeRect {
        rect: Rect,
        color: Color,
        width: f64,
    },
    FillRoundedRect {
        rect: Rect,
        radius: f64,
        color: Color,
    },
    StrokeRoundedRect {
        rect: Rect,
        radius: f64,
        color: Color,
        width: f64,
    },
    DrawText {
        baseline: Point,
        text: String,
        font_family: String,
        font_size: f64,
        color: Color,
    },
    PushClip(Rect),
    PopClip,
    PushOpacity(f64),
    PopOpacity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DisplayListError {
    UnbalancedClip,
    InvalidText,
    InvalidGeometry,
    InvalidOpacity,
    UnbalancedOpacity,
}

/// An ordered display list. Clipping is nested and applies to later commands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    commands: Vec<DisplayCommand>,
    clip_depth: usize,
    opacity_depth: usize,
}

impl DisplayList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.commands.push(DisplayCommand::FillRect { rect, color });
    }

    pub fn stroke_rect(
        &mut self,
        rect: Rect,
        color: Color,
        width: f64,
    ) -> Result<(), DisplayListError> {
        validate_positive(width)?;
        self.commands
            .push(DisplayCommand::StrokeRect { rect, color, width });
        Ok(())
    }

    pub fn fill_rounded_rect(
        &mut self,
        rect: Rect,
        radius: f64,
        color: Color,
    ) -> Result<(), DisplayListError> {
        validate_radius(radius)?;
        self.commands.push(DisplayCommand::FillRoundedRect {
            rect,
            radius,
            color,
        });
        Ok(())
    }

    pub fn stroke_rounded_rect(
        &mut self,
        rect: Rect,
        radius: f64,
        color: Color,
        width: f64,
    ) -> Result<(), DisplayListError> {
        validate_radius(radius)?;
        validate_positive(width)?;
        self.commands.push(DisplayCommand::StrokeRoundedRect {
            rect,
            radius,
            color,
            width,
        });
        Ok(())
    }

    /// Stores a single unwrapped text line and its baseline in logical points.
    pub fn draw_text(
        &mut self,
        baseline: Point,
        text: &str,
        font_family: &str,
        font_size: f64,
        color: Color,
    ) -> Result<(), DisplayListError> {
        if !baseline.x.is_finite()
            || !baseline.y.is_finite()
            || !font_size.is_finite()
            || font_size <= 0.0
            || font_family.is_empty()
        {
            return Err(DisplayListError::InvalidText);
        }
        self.commands.push(DisplayCommand::DrawText {
            baseline,
            text: text.to_owned(),
            font_family: font_family.to_owned(),
            font_size,
            color,
        });
        Ok(())
    }

    pub fn push_clip(&mut self, rect: Rect) {
        self.commands.push(DisplayCommand::PushClip(rect));
        self.clip_depth += 1;
    }

    pub fn pop_clip(&mut self) -> Result<(), DisplayListError> {
        if self.clip_depth == 0 {
            return Err(DisplayListError::UnbalancedClip);
        }
        self.clip_depth -= 1;
        self.commands.push(DisplayCommand::PopClip);
        Ok(())
    }

    pub fn push_opacity(&mut self, opacity: f64) -> Result<(), DisplayListError> {
        if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
            return Err(DisplayListError::InvalidOpacity);
        }
        self.commands.push(DisplayCommand::PushOpacity(opacity));
        self.opacity_depth += 1;
        Ok(())
    }

    pub fn pop_opacity(&mut self) -> Result<(), DisplayListError> {
        if self.opacity_depth == 0 {
            return Err(DisplayListError::UnbalancedOpacity);
        }
        self.opacity_depth -= 1;
        self.commands.push(DisplayCommand::PopOpacity);
        Ok(())
    }

    pub fn commands(&self) -> &[DisplayCommand] {
        &self.commands
    }
}

fn validate_positive(value: f64) -> Result<(), DisplayListError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(DisplayListError::InvalidGeometry)
    }
}

fn validate_radius(radius: f64) -> Result<(), DisplayListError> {
    if radius.is_finite() && radius >= 0.0 {
        Ok(())
    } else {
        Err(DisplayListError::InvalidGeometry)
    }
}
