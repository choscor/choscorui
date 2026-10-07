//! Visual recipes: swappable paint for a control's slots. A recipe decides
//! geometry and appearance of the background, icon, content, indicator,
//! and overlay slots. It never sees input, focus, commands, or semantics,
//! so replacing a recipe at runtime cannot change behavior.

use crate::{ButtonDecoration, ButtonStyle, ButtonVisualState};
use choscorui_core::{Color, DesignTokens, DisplayList, Point, Rect, Shadow, Size};

/// Paint layers in drawing order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Slot {
    Background,
    Icon,
    Content,
    Indicator,
    Overlay,
}

impl Slot {
    pub const ORDER: [Slot; 5] = [
        Slot::Background,
        Slot::Icon,
        Slot::Content,
        Slot::Indicator,
        Slot::Overlay,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentAlignment {
    Leading,
    Center,
}

/// Where a recipe places its slots inside the control bounds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotLayout {
    pub content: Rect,
    pub alignment: ContentAlignment,
    pub icon: Option<Rect>,
    /// Content color for the current state.
    pub content_color: Color,
}

/// Everything a recipe may read while painting one slot.
#[derive(Clone, Copy, Debug)]
pub struct SlotContext<'a> {
    pub bounds: Rect,
    pub state: ButtonVisualState,
    pub tokens: &'a DesignTokens,
    pub layout: SlotLayout,
}

pub trait ButtonRecipe {
    fn layout(&self, bounds: Rect, state: ButtonVisualState, tokens: &DesignTokens) -> SlotLayout;
    /// Paints one slot. The content slot paints behind the caller's label.
    fn paint_slot(&self, slot: Slot, list: &mut DisplayList, context: &SlotContext<'_>);
}

/// Paints every slot in [`Slot::ORDER`]; `content` draws the label into
/// the content rect right after the content slot.
pub fn paint_button(
    recipe: &dyn ButtonRecipe,
    list: &mut DisplayList,
    bounds: Rect,
    state: ButtonVisualState,
    tokens: &DesignTokens,
    content: impl FnOnce(&mut DisplayList, &SlotLayout),
) -> SlotLayout {
    let context = SlotContext {
        bounds,
        state,
        tokens,
        layout: recipe.layout(bounds, state, tokens),
    };
    let mut content = Some(content);
    for slot in Slot::ORDER {
        recipe.paint_slot(slot, list, &context);
        if slot == Slot::Content
            && let Some(draw) = content.take()
        {
            draw(list, &context.layout);
        }
    }
    context.layout
}

/// The flat style as a recipe: square fill, optional leading rail,
/// top-and-bottom focus bars, leading content.
impl ButtonRecipe for ButtonStyle {
    fn layout(&self, bounds: Rect, _: ButtonVisualState, tokens: &DesignTokens) -> SlotLayout {
        let rail = match self.decoration {
            ButtonDecoration::LeadingRail { width, .. } if width.is_finite() && width > 0.0 => {
                width
            }
            _ => 0.0,
        };
        SlotLayout {
            content: inset(bounds, rail + tokens.spacing * 2.0, tokens.spacing),
            alignment: ContentAlignment::Leading,
            icon: None,
            content_color: tokens.content,
        }
    }

    fn paint_slot(&self, slot: Slot, list: &mut DisplayList, context: &SlotContext<'_>) {
        match slot {
            Slot::Background => {
                let mut flat = *self;
                flat.focus = Color::TRANSPARENT;
                // Focus belongs to the indicator slot.
                let state = ButtonVisualState {
                    focused: false,
                    ..context.state
                };
                flat.paint(list, context.bounds, state);
            }
            Slot::Indicator if context.state.focused => {
                let bounds = context.bounds;
                let bar = Size::new(bounds.size.width, 2.0);
                list.fill_rect(Rect::new(bounds.origin, bar), self.focus);
                list.fill_rect(
                    Rect::new(
                        Point::new(bounds.origin.x, bounds.origin.y + bounds.size.height - 2.0),
                        bar,
                    ),
                    self.focus,
                );
            }
            _ => {}
        }
    }
}

/// A rounded, elevated recipe with a trailing icon badge, centered content,
/// an outset focus ring, and a pressed/disabled overlay. Colors and metrics
/// come from the design tokens.
#[derive(Clone, Copy, Debug, Default)]
pub struct PillButtonRecipe {
    /// Uses `danger` instead of `accent` (for example a toggled state).
    pub emphasized: bool,
}

impl PillButtonRecipe {
    fn radius(bounds: Rect, tokens: &DesignTokens) -> f64 {
        tokens.radius.min(bounds.size.height / 2.0).max(0.0)
    }
}

impl ButtonRecipe for PillButtonRecipe {
    fn layout(&self, bounds: Rect, _: ButtonVisualState, tokens: &DesignTokens) -> SlotLayout {
        let badge = (bounds.size.height - tokens.spacing * 2.0).clamp(0.0, 28.0);
        let icon = Rect::new(
            Point::new(
                bounds.origin.x + bounds.size.width - tokens.spacing * 2.0 - badge,
                bounds.origin.y + (bounds.size.height - badge) / 2.0,
            ),
            Size::new(badge, badge),
        );
        let content = Rect::new(
            Point::new(bounds.origin.x + tokens.spacing * 2.0, bounds.origin.y),
            Size::new(
                (icon.origin.x - bounds.origin.x - tokens.spacing * 3.0).max(0.0),
                bounds.size.height,
            ),
        );
        SlotLayout {
            content,
            alignment: ContentAlignment::Center,
            icon: Some(icon),
            content_color: tokens.background,
        }
    }

    fn paint_slot(&self, slot: Slot, list: &mut DisplayList, context: &SlotContext<'_>) {
        let tokens = context.tokens;
        let bounds = context.bounds;
        let radius = Self::radius(bounds, tokens);
        let fill = if self.emphasized {
            tokens.danger
        } else {
            tokens.accent
        };
        match slot {
            Slot::Background => {
                if tokens.elevation > 0.0 && context.state.enabled {
                    let lift = if context.state.pressed {
                        tokens.elevation / 2.0
                    } else {
                        tokens.elevation
                    };
                    let _ = list.draw_shadow(
                        bounds,
                        radius,
                        Shadow {
                            offset: Point::new(0.0, lift),
                            blur: lift * 3.0,
                            color: Color::rgba8(0, 0, 0, 90),
                        },
                    );
                }
                let fill = if context.state.hovered && !context.state.pressed {
                    mix(fill, tokens.surface, 0.2)
                } else {
                    fill
                };
                let _ = list.fill_rounded_rect(bounds, radius, fill);
            }
            Slot::Icon => {
                if let Some(icon) = context.layout.icon {
                    let _ = list.fill_rounded_rect(icon, icon.size.height / 2.0, tokens.surface);
                    let dot = inset(icon, icon.size.width * 0.3, icon.size.height * 0.3);
                    let _ = list.fill_rounded_rect(dot, dot.size.height / 2.0, fill);
                }
            }
            Slot::Content => {}
            Slot::Indicator => {
                if context.state.focused {
                    let gap = tokens.border_width.max(1.0) + 2.0;
                    let ring = inset(bounds, -gap, -gap);
                    let _ = list.stroke_rounded_rect(
                        ring,
                        radius + gap,
                        tokens.content,
                        tokens.border_width.max(1.0) * 2.0,
                    );
                }
            }
            Slot::Overlay => {
                let shade = if !context.state.enabled {
                    Some(tokens.surface.with_alpha(0.6))
                } else if context.state.pressed {
                    Some(Color::rgba8(0, 0, 0, 50))
                } else {
                    None
                };
                if let Some(shade) = shade {
                    let _ = list.fill_rounded_rect(bounds, radius, shade);
                }
            }
        }
    }
}

fn inset(rect: Rect, dx: f64, dy: f64) -> Rect {
    Rect::new(
        Point::new(rect.origin.x + dx, rect.origin.y + dy),
        Size::new(
            (rect.size.width - dx * 2.0).max(0.0),
            (rect.size.height - dy * 2.0).max(0.0),
        ),
    )
}

fn mix(a: Color, b: Color, t: f64) -> Color {
    let channel = |x: u8, y: u8| (f64::from(x) + (f64::from(y) - f64::from(x)) * t).round() as u8;
    Color::rgba8(
        channel(a.red, b.red),
        channel(a.green, b.green),
        channel(a.blue, b.blue),
        channel(a.alpha, b.alpha),
    )
}
