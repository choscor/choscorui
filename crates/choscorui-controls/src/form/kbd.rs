//! Keyboard key caps, alone or in a group.

use crate::custom::{ControlContext, ControlEvent, CustomControl, MeasureContext};
use crate::hosted;
use crate::style::{
    Content, ControlKind, KbdVariant, PaintContext, ResolvedStyle, Slot, StyleQuery, TextAlign,
    TextMeasure, content_rect,
};
use choscorui_core::{
    EventReply, Icon, Localizer, Measured, Rect, SemanticNode, SemanticRole, Size,
};

/// The key symbols that assistive technology reads by name, with the id
/// of the name in the built-in messages.
const KEY_NAMES: [(char, &str); 20] = [
    ('⌘', "choscorui.key.command"),
    ('⇧', "choscorui.key.shift"),
    ('⌥', "choscorui.key.option"),
    ('⌃', "choscorui.key.control"),
    ('⌫', "choscorui.key.delete"),
    ('⌦', "choscorui.key.forward_delete"),
    ('⏎', "choscorui.key.return"),
    ('↩', "choscorui.key.return"),
    ('↵', "choscorui.key.return"),
    ('⎋', "choscorui.key.escape"),
    ('⇥', "choscorui.key.tab"),
    ('⇪', "choscorui.key.caps_lock"),
    ('␣', "choscorui.key.space"),
    ('←', "choscorui.key.left"),
    ('→', "choscorui.key.right"),
    ('↑', "choscorui.key.up"),
    ('↓', "choscorui.key.down"),
    ('⇞', "choscorui.key.page_up"),
    ('⇟', "choscorui.key.page_down"),
    ('⏏', "choscorui.key.eject"),
];

/// `key` as assistive technology reads it: each key symbol by its name,
/// and the other text as it is, such as "Command K" for "⌘K".
fn spoken(key: &str, strings: &Localizer) -> String {
    let mut words: Vec<String> = Vec::new();
    let mut plain = String::new();
    for ch in key.chars() {
        match KEY_NAMES.iter().find(|(symbol, _)| *symbol == ch) {
            Some((_, id)) => {
                if !plain.trim().is_empty() {
                    words.push(plain.trim().to_owned());
                }
                plain.clear();
                words.push(strings.message(id, &[]));
            }
            None => plain.push(ch),
        }
    }
    if !plain.trim().is_empty() {
        words.push(plain.trim().to_owned());
    }
    words.join(" ")
}

/// One or more key caps, such as `⌘` `K`. Caps sit in reading order with
/// the group gap between them. The control does not take focus.
#[derive(Clone, Debug, PartialEq)]
pub struct Kbd {
    keys: Vec<String>,
    variant: KbdVariant,
    icon: Option<Icon>,
}

impl Kbd {
    /// One key cap.
    pub fn new(key: impl Into<String>) -> Self {
        Self::group([key.into()])
    }

    /// A group of key caps.
    pub fn group(keys: impl IntoIterator<Item = String>) -> Self {
        Self {
            keys: keys.into_iter().collect(),
            variant: KbdVariant::Default,
            icon: None,
        }
    }

    /// Shows `icon` in place of the text of the first cap, such as an
    /// arrow for `↑`. The text of the cap stays its name.
    #[must_use]
    pub fn with_icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    #[must_use]
    pub fn with_variant(mut self, variant: KbdVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn keys(&self) -> &[String] {
        &self.keys
    }

    fn styles(&self, resolve: impl Fn(StyleQuery) -> ResolvedStyle) -> (ResolvedStyle, f64) {
        let cap = resolve(StyleQuery::new(ControlKind::Kbd, Slot::Root).variant(self.variant));
        let group = resolve(StyleQuery::new(ControlKind::Kbd, Slot::Group));
        (cap, group.gap)
    }

    /// The content of each cap: the icon replaces the first text.
    fn contents(&self) -> impl Iterator<Item = Content<'_>> {
        self.keys
            .iter()
            .enumerate()
            .map(|(index, key)| match self.icon.filter(|_| index == 0) {
                Some(icon) => Content {
                    leading: Some(icon),
                    ..Content::default()
                },
                None => Content::label(key),
            })
    }

    fn cap_sizes(&self, cap: &ResolvedStyle, text: &dyn TextMeasure) -> Vec<Size> {
        self.contents()
            .map(|content| content.size(cap, text))
            .collect()
    }
}

impl CustomControl for Kbd {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let (cap, gap) = self.styles(|query| cx.style(query));
        let sizes = self.cap_sizes(&cap, cx.text);
        let width = sizes.iter().map(|size| size.width).sum::<f64>()
            + gap * sizes.len().saturating_sub(1) as f64;
        let height = sizes.iter().map(|size| size.height).fold(0.0, f64::max);
        // The baseline of the text in the tallest cap, which is centered.
        let baseline = sizes.iter().find(|size| size.height == height).map(|size| {
            let inner = content_rect(Rect::from_xywh(0.0, 0.0, size.width, height), &cap, false);
            let spec = &cap.text;
            inner.origin.y + (inner.size.height - spec.line_height) / 2.0 + spec.baseline()
        });
        Measured {
            size: Size::new(width, height),
            baseline,
        }
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let (cap, gap) = self.styles(|query| cx.style(query));
        let bounds = cx.bounds;
        let mut x = if cx.rtl {
            bounds.max_x()
        } else {
            bounds.origin.x
        };
        for (content, size) in self.contents().zip(self.cap_sizes(&cap, cx.text)) {
            let left = if cx.rtl { x - size.width } else { x };
            let y = bounds.origin.y + (bounds.size.height - size.height) / 2.0;
            let rect = Rect::from_xywh(left, y, size.width, size.height);
            cx.slot(rect, &cap, |cx, inner| {
                content.draw(cx, inner, &cap, TextAlign::Center);
            });
            let step = size.width + gap;
            x += if cx.rtl { -step } else { step };
        }
    }

    fn semantics(&self) -> SemanticNode {
        self.semantics_in(Localizer::builtin())
    }

    /// The name reads each key, with a key symbol by its name.
    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        let keys: Vec<String> = self.keys.iter().map(|key| spoken(key, strings)).collect();
        hosted::node(SemanticRole::StaticText, keys.join(" "), Vec::new())
    }

    fn focusable(&self) -> bool {
        false
    }
}
