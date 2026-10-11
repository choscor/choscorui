use choscorui_core::{Color, DisplayCommand, DisplayList, Point, Rect};

/// A validated, fully owned sequence ready for a Windows drawing backend.
#[derive(Clone, Debug, PartialEq)]
pub struct RenderPlan {
    operations: Vec<RenderOp>,
    max_clip_depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum RenderOp {
    FillRect {
        rect: Rect,
        color: Color,
    },
    DrawText {
        baseline: Point,
        text: String,
        font_family: String,
        font_size: f32,
        color: Color,
    },
    PushClip(Rect),
    PopClip,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderPlanError {
    InvalidGeometry,
    UnbalancedClip,
    UnsupportedCommand,
}

impl RenderPlan {
    pub fn compile(source: &DisplayList) -> Result<Self, RenderPlanError> {
        let mut operations = Vec::with_capacity(source.commands().len());
        let mut clip_depth = 0usize;
        let mut max_clip_depth = 0usize;
        for command in source.commands() {
            let operation = match command {
                DisplayCommand::FillRect { rect, color } => {
                    validate_rect(*rect)?;
                    RenderOp::FillRect {
                        rect: *rect,
                        color: *color,
                    }
                }
                DisplayCommand::DrawText {
                    baseline,
                    text,
                    font_family,
                    font_size,
                    color,
                    ..
                } => {
                    if !baseline.x.is_finite()
                        || !baseline.y.is_finite()
                        || !font_size.is_finite()
                        || *font_size <= 0.0
                    {
                        return Err(RenderPlanError::InvalidGeometry);
                    }
                    RenderOp::DrawText {
                        baseline: *baseline,
                        text: text.clone(),
                        font_family: font_family.clone(),
                        font_size: *font_size as f32,
                        color: *color,
                    }
                }
                DisplayCommand::PushClip(rect) => {
                    validate_rect(*rect)?;
                    clip_depth += 1;
                    max_clip_depth = max_clip_depth.max(clip_depth);
                    RenderOp::PushClip(*rect)
                }
                DisplayCommand::PopClip => {
                    clip_depth = clip_depth
                        .checked_sub(1)
                        .ok_or(RenderPlanError::UnbalancedClip)?;
                    RenderOp::PopClip
                }
                _ => return Err(RenderPlanError::UnsupportedCommand),
            };
            operations.push(operation);
        }
        if clip_depth != 0 {
            return Err(RenderPlanError::UnbalancedClip);
        }
        Ok(Self {
            operations,
            max_clip_depth,
        })
    }

    pub fn operations(&self) -> &[RenderOp] {
        &self.operations
    }

    pub fn max_clip_depth(&self) -> usize {
        self.max_clip_depth
    }
}

fn validate_rect(rect: Rect) -> Result<(), RenderPlanError> {
    if rect.origin.x.is_finite()
        && rect.origin.y.is_finite()
        && rect.size.width.is_finite()
        && rect.size.height.is_finite()
        && rect.size.width >= 0.0
        && rect.size.height >= 0.0
    {
        Ok(())
    } else {
        Err(RenderPlanError::InvalidGeometry)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceEvent {
    CreateTarget,
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameResult {
    Presented,
    TargetLost,
}

/// Tracks device-dependent target ownership separately from device-independent
/// Direct2D and DirectWrite factories.
#[derive(Debug, Default)]
pub struct DeviceResources {
    has_target: bool,
}

impl DeviceResources {
    pub const fn new() -> Self {
        Self { has_target: false }
    }

    pub const fn has_target(&self) -> bool {
        self.has_target
    }

    pub const fn begin_frame(&self) -> DeviceEvent {
        if self.has_target {
            DeviceEvent::Ready
        } else {
            DeviceEvent::CreateTarget
        }
    }

    pub fn target_created(&mut self) {
        self.has_target = true;
    }

    pub fn end_frame(&mut self, result: FrameResult) -> DeviceEvent {
        match result {
            FrameResult::Presented => self.begin_frame(),
            FrameResult::TargetLost => {
                self.has_target = false;
                DeviceEvent::CreateTarget
            }
        }
    }

    pub fn release(&mut self) {
        self.has_target = false;
    }
}
