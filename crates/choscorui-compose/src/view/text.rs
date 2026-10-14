//! Text: catalog strings through the environment's localizer, or literal
//! user data, drawn with the inherited text appearance.

use super::View;
use super::env::{Env, Fill};
use super::host::Cx;
use super::modifier::update_style;
use super::state::{IntoLive, Live, State};
use choscorui_controls::{
    ControlContext, ControlEvent, CustomControl, MeasureContext, PaintContext, TextAlign, TextLine,
    TextMeasure, TextOverflow, draw_text,
};
use choscorui_core::{
    Color, ColorRole, DesignTokens, Dirty, EventReply, FontWeight, Measured, Rect, SemanticNode,
    SemanticRole, SemanticState, Size, TextPreset, TextSize, Tracking, TypeSpec,
};
use std::cell::RefCell;
use std::fmt::Display;
use std::rc::Rc;

/// A catalog string with its arguments, resolved through the
/// environment's catalog and locale. Build one with [`crate::t!`].
#[derive(Clone)]
pub struct Localized {
    key: String,
    args: Vec<(String, Live<String>)>,
    count: Option<Live<u64>>,
}

impl Localized {
    /// The catalog string `key`, without arguments.
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            args: Vec::new(),
            count: None,
        }
    }

    /// Adds the argument `name`: a value, a state handle, or a closure.
    #[must_use]
    pub fn arg<M>(mut self, name: impl Into<String>, value: impl IntoTextArg<M>) -> Self {
        self.args.push((name.into(), value.into_arg()));
        self
    }

    /// Selects the plural form for `count`.
    #[must_use]
    pub fn count<M>(mut self, count: impl IntoLive<u64, M>) -> Self {
        self.count = Some(count.into_live());
        self
    }

    /// The translation in `env`. Inside a live property, the locale and
    /// argument reads are recorded.
    pub fn resolve(&self, env: &Env) -> String {
        let values: Vec<(String, String)> = self
            .args
            .iter()
            .map(|(name, value)| (name.clone(), value.get()))
            .collect();
        let args: Vec<(&str, &str)> = values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_str()))
            .collect();
        match &self.count {
            Some(count) => env.plural(&self.key, count.get(), &args),
            None => env.text(&self.key, &args),
        }
    }
}

impl std::fmt::Debug for Localized {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Localized").field("key", &self.key).finish()
    }
}

/// Converts a text argument: a displayable value, a state handle, or a
/// closure. `M` only tells the conversions apart.
pub trait IntoTextArg<M> {
    /// The argument as a live string.
    fn into_arg(self) -> Live<String>;
}

impl<T: Display> IntoTextArg<super::state::Fixed> for T {
    fn into_arg(self) -> Live<String> {
        Live::Value(self.to_string())
    }
}

impl<R: Display, F: Fn() -> R + 'static> IntoTextArg<super::state::Reading> for F {
    fn into_arg(self) -> Live<String> {
        Live::Computed(Rc::new(move || self().to_string()))
    }
}

impl<T> IntoTextArg<super::state::Handle> for State<T>
where
    T: Display + Clone + PartialEq + Default + 'static,
{
    fn into_arg(self) -> Live<String> {
        Live::Computed(Rc::new(move || self.get().to_string()))
    }
}

/// A catalog string with arguments: `t!("count", n = count)`. Each
/// argument is a value, a state handle, or a closure, and the text
/// updates when one of them or the locale changes.
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::view::Localized::new($key)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::view::Localized::new($key)$(.arg(stringify!($name), $value))+
    };
}

/// What a [`Text`] shows: literal user data, or a catalog string.
#[derive(Clone, Debug)]
pub enum TextContent {
    Plain(String),
    Localized(Localized),
}

impl TextContent {
    /// The text in `env`: the literal text, or the translation.
    pub fn resolve(&self, env: &Env) -> String {
        match self {
            Self::Plain(text) => text.clone(),
            Self::Localized(localized) => localized.resolve(env),
        }
    }
}

impl From<&str> for TextContent {
    fn from(text: &str) -> Self {
        Self::Plain(text.to_owned())
    }
}

impl From<String> for TextContent {
    fn from(text: String) -> Self {
        Self::Plain(text)
    }
}

impl From<Localized> for TextContent {
    fn from(localized: Localized) -> Self {
        Self::Localized(localized)
    }
}

/// A run of text. It takes a catalog string (`Text::new(t!("title"))`),
/// or literal text for user data. The text wraps to the width it gets
/// and mirrors in right-to-left layouts.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_compose::t;
/// struct Greeting;
///
/// impl View for Greeting {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let name = cx.state(String::from("Ada"));
///         VStack::new((
///             Text::new(t!("greeting.title")),
///             Text::new(move || format!("Hello, {}", name.get())).single_line(),
///         ))
///     }
/// }
/// ```
pub struct Text {
    content: Live<TextContent>,
    align: TextAlign,
    wrap: bool,
}

impl Text {
    /// Text that shows `content`: a string, a catalog string, a state
    /// handle, or a closure.
    pub fn new<M>(content: impl IntoLive<TextContent, M>) -> Self {
        Self {
            content: content.into_live(),
            align: TextAlign::Start,
            wrap: true,
        }
    }

    /// Aligns each line in the width the text gets.
    #[must_use]
    pub fn align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Keeps the text on one line.
    #[must_use]
    pub fn single_line(mut self) -> Self {
        self.wrap = false;
        self
    }
}

/// What the text leaf draws.
#[derive(Clone, Debug)]
struct TextData {
    content: String,
    preset: Option<TextPreset>,
    weight: Option<FontWeight>,
    tracking: Option<Tracking>,
    foreground: Option<Fill>,
    align: TextAlign,
    wrap: bool,
}

impl TextData {
    fn spec(&self, tokens: &DesignTokens) -> TypeSpec {
        let mut spec = match self.preset {
            Some(preset) => tokens.preset(preset),
            None => tokens.text(TextSize::Sm, FontWeight::REGULAR),
        };
        if let Some(weight) = self.weight {
            spec = spec.with_weight(weight);
        }
        if let Some(tracking) = self.tracking {
            spec = spec.with_tracking(tracking);
        }
        spec
    }

    fn color(&self, tokens: &DesignTokens) -> Color {
        match self.foreground {
            Some(fill) => fill.resolve(tokens),
            None => tokens.color(self.preset.map_or(ColorRole::Foreground, TextPreset::color)),
        }
    }
}

/// Breaks `text` into lines that `measure` finds no wider than `width`,
/// at spaces where it can and inside words where it must.
pub(super) fn wrap_lines(text: &str, width: f64, measure: &dyn Fn(&str) -> f64) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split(' ') {
            let candidate = if line.is_empty() {
                word.to_owned()
            } else {
                format!("{line} {word}")
            };
            if line.is_empty() || measure(&candidate) <= width {
                line = candidate;
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_owned();
            }
        }
        lines.push(line);
    }
    lines
}

/// The leaf control that a [`Text`] mounts.
struct TextLeaf {
    data: Rc<RefCell<TextData>>,
}

impl TextLeaf {
    fn lines(&self, spec: &TypeSpec, width: f64, measure: &dyn TextMeasure) -> Vec<String> {
        let data = self.data.borrow();
        if data.wrap && width.is_finite() && measure.width(&data.content, spec) > width {
            wrap_lines(&data.content, width, &|line| measure.width(line, spec))
        } else {
            data.content.split('\n').map(str::to_owned).collect()
        }
    }
}

impl CustomControl for TextLeaf {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let spec = self.data.borrow().spec(cx.tokens);
        let lines = self.lines(&spec, cx.constraints.max.width, cx.text);
        let width = lines
            .iter()
            .map(|line| cx.text.width(line, &spec))
            .fold(0.0, f64::max);
        let count = lines.len().max(1) as f64;
        Measured {
            size: Size::new(width.ceil(), spec.line_height * count),
            baseline: Some(spec.baseline()),
        }
    }

    fn event(&mut self, _cx: &mut ControlContext<'_>, _event: &ControlEvent) -> EventReply {
        EventReply::Continue
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let (spec, color, align) = {
            let data = self.data.borrow();
            (data.spec(cx.tokens), data.color(cx.tokens), data.align)
        };
        let bounds = cx.bounds;
        let lines = self.lines(&spec, bounds.size.width + 0.5, cx.text);
        for (index, line) in lines.iter().enumerate() {
            let rect = Rect::from_xywh(
                bounds.origin.x,
                bounds.origin.y + spec.line_height * index as f64,
                bounds.size.width,
                spec.line_height,
            );
            let line = TextLine {
                text: line,
                spec,
                color,
                align,
                rtl: cx.rtl,
                underline: false,
                mark: None,
                overflow: TextOverflow::Ellipsis,
            };
            draw_text(cx.list, rect, &line, cx.text);
        }
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::StaticText,
            name: self.data.borrow().content.clone(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }

    fn focusable(&self) -> bool {
        false
    }
}

impl View for Text {
    fn mount(self, cx: &mut Cx) {
        let env = cx.env.clone();
        let data = Rc::new(RefCell::new(TextData {
            content: String::new(),
            preset: None,
            weight: None,
            tracking: None,
            foreground: None,
            align: self.align,
            wrap: self.wrap,
        }));
        let node = cx.control(Box::new(TextLeaf { data: data.clone() }));
        cx.inner.data(node).text = true;
        if self.wrap {
            // Wrapping text gives up width to its stack and takes more lines.
            update_style(cx.inner, node, |style| style.shrink = true);
        }
        let content = self.content;
        let shared = data.clone();
        let content_env = env.clone();
        cx.react(move |inner| {
            let text = content.get().resolve(&content_env);
            let changed = shared.borrow().content != text;
            if changed {
                shared.borrow_mut().content = text;
                inner.touch(node, Dirty::LAYOUT.with(Dirty::SEMANTICS));
            }
        });
        let style = env.text_env().clone();
        cx.react(move |inner| {
            let preset = style.preset.as_ref().map(Live::get);
            let weight = style.weight.as_ref().map(Live::get);
            let tracking = style.tracking.as_ref().map(Live::get);
            let foreground = style.foreground.as_ref().map(Live::get);
            let mut current = data.borrow_mut();
            let layout = current.preset != preset
                || current.weight != weight
                || current.tracking != tracking;
            let paint = current.foreground != foreground;
            current.preset = preset;
            current.weight = weight;
            current.tracking = tracking;
            current.foreground = foreground;
            drop(current);
            if layout {
                inner.touch(node, Dirty::LAYOUT);
            } else if paint {
                inner.touch(node, Dirty::PAINT);
            }
        });
    }
}
