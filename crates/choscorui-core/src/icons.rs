//! Line icons drawn from SVG source: a 24-unit view box, a stroke width of
//! 2, and round caps and joins. The geometry comes from the Lucide icon set
//! (ISC license; see `docs/licenses.md`).

use crate::{Color, DisplayList, DisplayListError, Rect, SvgDocument};
use std::sync::OnceLock;

/// A bundled line icon.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Icon {
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    Bell,
    Bold,
    Calendar,
    Check,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ChevronsLeft,
    ChevronsRight,
    ChevronsUpDown,
    Circle,
    CircleAlert,
    CircleCheck,
    CircleDashed,
    CircleX,
    Copy,
    Dot,
    Ellipsis,
    EllipsisVertical,
    Eye,
    File,
    GripVertical,
    Heart,
    House,
    Image,
    Inbox,
    Info,
    Italic,
    Loader,
    LoaderCircle,
    Mail,
    Menu,
    Minus,
    Moon,
    OctagonX,
    PanelLeft,
    Paperclip,
    Plus,
    Search,
    Send,
    Settings,
    Star,
    Sun,
    TextAlignCenter,
    TextAlignEnd,
    TextAlignStart,
    Trash,
    TriangleAlert,
    Underline,
    Upload,
    User,
    X,
}

impl Icon {
    pub const ALL: [Icon; 57] = [
        Self::ArrowDown,
        Self::ArrowLeft,
        Self::ArrowRight,
        Self::ArrowUp,
        Self::Bell,
        Self::Bold,
        Self::Calendar,
        Self::Check,
        Self::ChevronDown,
        Self::ChevronLeft,
        Self::ChevronRight,
        Self::ChevronUp,
        Self::ChevronsLeft,
        Self::ChevronsRight,
        Self::ChevronsUpDown,
        Self::Circle,
        Self::CircleAlert,
        Self::CircleCheck,
        Self::CircleDashed,
        Self::CircleX,
        Self::Copy,
        Self::Dot,
        Self::Ellipsis,
        Self::EllipsisVertical,
        Self::Eye,
        Self::File,
        Self::GripVertical,
        Self::Heart,
        Self::House,
        Self::Image,
        Self::Inbox,
        Self::Info,
        Self::Italic,
        Self::Loader,
        Self::LoaderCircle,
        Self::Mail,
        Self::Menu,
        Self::Minus,
        Self::Moon,
        Self::OctagonX,
        Self::PanelLeft,
        Self::Paperclip,
        Self::Plus,
        Self::Search,
        Self::Send,
        Self::Settings,
        Self::Star,
        Self::Sun,
        Self::TextAlignCenter,
        Self::TextAlignEnd,
        Self::TextAlignStart,
        Self::Trash,
        Self::TriangleAlert,
        Self::Underline,
        Self::Upload,
        Self::User,
        Self::X,
    ];

    /// The source icon name, in kebab case.
    pub const fn name(self) -> &'static str {
        match self {
            Self::ArrowDown => "arrow-down",
            Self::ArrowLeft => "arrow-left",
            Self::ArrowRight => "arrow-right",
            Self::ArrowUp => "arrow-up",
            Self::Bell => "bell",
            Self::Bold => "bold",
            Self::Calendar => "calendar",
            Self::Check => "check",
            Self::ChevronDown => "chevron-down",
            Self::ChevronLeft => "chevron-left",
            Self::ChevronRight => "chevron-right",
            Self::ChevronUp => "chevron-up",
            Self::ChevronsLeft => "chevrons-left",
            Self::ChevronsRight => "chevrons-right",
            Self::ChevronsUpDown => "chevrons-up-down",
            Self::Circle => "circle",
            Self::CircleAlert => "circle-alert",
            Self::CircleCheck => "circle-check",
            Self::CircleDashed => "circle-dashed",
            Self::CircleX => "circle-x",
            Self::Copy => "copy",
            Self::Dot => "dot",
            Self::Ellipsis => "ellipsis",
            Self::EllipsisVertical => "ellipsis-vertical",
            Self::Eye => "eye",
            Self::File => "file",
            Self::GripVertical => "grip-vertical",
            Self::Heart => "heart",
            Self::House => "house",
            Self::Image => "image",
            Self::Inbox => "inbox",
            Self::Info => "info",
            Self::Italic => "italic",
            Self::Loader => "loader",
            Self::LoaderCircle => "loader-circle",
            Self::Mail => "mail",
            Self::Menu => "menu",
            Self::Minus => "minus",
            Self::Moon => "moon",
            Self::OctagonX => "octagon-x",
            Self::PanelLeft => "panel-left",
            Self::Paperclip => "paperclip",
            Self::Plus => "plus",
            Self::Search => "search",
            Self::Send => "send",
            Self::Settings => "settings",
            Self::Star => "star",
            Self::Sun => "sun",
            Self::TextAlignCenter => "text-align-center",
            Self::TextAlignEnd => "text-align-end",
            Self::TextAlignStart => "text-align-start",
            Self::Trash => "trash",
            Self::TriangleAlert => "triangle-alert",
            Self::Underline => "underline",
            Self::Upload => "upload",
            Self::User => "user",
            Self::X => "x",
        }
    }

    /// The shapes inside the icon's `<svg>` element.
    const fn body(self) -> &'static str {
        match self {
            Self::ArrowDown => r#"<path d="M12 5v14"/> <path d="m19 12-7 7-7-7"/>"#,
            Self::ArrowLeft => r#"<path d="m12 19-7-7 7-7"/> <path d="M19 12H5"/>"#,
            Self::ArrowRight => r#"<path d="M5 12h14"/> <path d="m12 5 7 7-7 7"/>"#,
            Self::ArrowUp => r#"<path d="m5 12 7-7 7 7"/> <path d="M12 19V5"/>"#,
            Self::Bell => {
                r#"<path d="M10.268 21a2 2 0 0 0 3.464 0"/> <path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/>"#
            }
            Self::Bold => {
                r#"<path d="M6 12h9a4 4 0 0 1 0 8H7a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h7a4 4 0 0 1 0 8"/>"#
            }
            Self::Calendar => {
                r#"<path d="M8 2v3"/> <path d="M16 2v3"/> <rect x="3" y="3" width="18" height="18" rx="2"/> <path d="M3 9h18"/>"#
            }
            Self::Check => r#"<path d="M20 6 9 17l-5-5"/>"#,
            Self::ChevronDown => r#"<path d="m6 9 6 6 6-6"/>"#,
            Self::ChevronLeft => r#"<path d="m15 18-6-6 6-6"/>"#,
            Self::ChevronRight => r#"<path d="m9 18 6-6-6-6"/>"#,
            Self::ChevronUp => r#"<path d="m18 15-6-6-6 6"/>"#,
            Self::ChevronsLeft => r#"<path d="m11 17-5-5 5-5"/> <path d="m18 17-5-5 5-5"/>"#,
            Self::ChevronsRight => r#"<path d="m6 17 5-5-5-5"/> <path d="m13 17 5-5-5-5"/>"#,
            Self::ChevronsUpDown => r#"<path d="m7 15 5 5 5-5"/> <path d="m7 9 5-5 5 5"/>"#,
            Self::Circle => r#"<circle cx="12" cy="12" r="10"/>"#,
            Self::CircleAlert => {
                r#"<circle cx="12" cy="12" r="10"/> <line x1="12" x2="12" y1="8" y2="12"/> <line x1="12" x2="12.01" y1="16" y2="16"/>"#
            }
            Self::CircleCheck => {
                r#"<circle cx="12" cy="12" r="10"/> <path d="m16 9-5.5 5.5L8 12"/>"#
            }
            Self::CircleDashed => {
                r#"<path d="M10.1 2.182a10 10 0 0 1 3.8 0"/> <path d="M13.9 21.818a10 10 0 0 1-3.8 0"/> <path d="M17.609 3.721a10 10 0 0 1 2.69 2.7"/> <path d="M2.182 13.9a10 10 0 0 1 0-3.8"/> <path d="M20.279 17.609a10 10 0 0 1-2.7 2.69"/> <path d="M21.818 10.1a10 10 0 0 1 0 3.8"/> <path d="M3.721 6.391a10 10 0 0 1 2.7-2.69"/> <path d="M6.391 20.279a10 10 0 0 1-2.69-2.7"/>"#
            }
            Self::CircleX => {
                r#"<circle cx="12" cy="12" r="10"/> <path d="m15 9-6 6"/> <path d="m9 9 6 6"/>"#
            }
            Self::Copy => {
                r#"<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/> <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>"#
            }
            Self::Dot => r#"<circle cx="12" cy="12" r="1"/>"#,
            Self::Ellipsis => {
                r#"<circle cx="12" cy="12" r="1"/> <circle cx="19" cy="12" r="1"/> <circle cx="5" cy="12" r="1"/>"#
            }
            Self::EllipsisVertical => {
                r#"<circle cx="12" cy="12" r="1"/> <circle cx="12" cy="5" r="1"/> <circle cx="12" cy="19" r="1"/>"#
            }
            Self::Eye => {
                r#"<path d="M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0"/> <circle cx="12" cy="12" r="3"/>"#
            }
            Self::File => {
                r#"<path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"/> <path d="M14 2v5a1 1 0 0 0 1 1h5"/>"#
            }
            Self::GripVertical => {
                r#"<circle cx="9" cy="12" r="1"/> <circle cx="9" cy="5" r="1"/> <circle cx="9" cy="19" r="1"/> <circle cx="15" cy="12" r="1"/> <circle cx="15" cy="5" r="1"/> <circle cx="15" cy="19" r="1"/>"#
            }
            Self::Heart => {
                r#"<path d="M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5"/>"#
            }
            Self::House => {
                r#"<path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"/> <path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>"#
            }
            Self::Image => {
                r#"<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/> <circle cx="9" cy="9" r="2"/> <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>"#
            }
            Self::Inbox => {
                r#"<polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/> <path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>"#
            }
            Self::Info => {
                r#"<circle cx="12" cy="12" r="10"/> <path d="M12 16v-4"/> <path d="M12 8h.01"/>"#
            }
            Self::Italic => {
                r#"<line x1="19" x2="10" y1="4" y2="4"/> <line x1="14" x2="5" y1="20" y2="20"/> <line x1="15" x2="9" y1="4" y2="20"/>"#
            }
            Self::Loader => {
                r#"<path d="M12 2v4"/> <path d="m16.2 7.8 2.9-2.9"/> <path d="M18 12h4"/> <path d="m16.2 16.2 2.9 2.9"/> <path d="M12 18v4"/> <path d="m4.9 19.1 2.9-2.9"/> <path d="M2 12h4"/> <path d="m4.9 4.9 2.9 2.9"/>"#
            }
            Self::LoaderCircle => r#"<path d="M21 12a9 9 0 1 1-6.219-8.56"/>"#,
            Self::Mail => {
                r#"<path d="m22 7-8.991 5.727a2 2 0 0 1-2.009 0L2 7"/> <rect x="2" y="4" width="20" height="16" rx="2"/>"#
            }
            Self::Menu => r#"<path d="M4 5h16"/> <path d="M4 12h16"/> <path d="M4 19h16"/>"#,
            Self::Minus => r#"<path d="M5 12h14"/>"#,
            Self::Moon => {
                r#"<path d="M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401"/>"#
            }
            Self::OctagonX => {
                r#"<path d="m15 9-6 6"/> <path d="M2.586 16.726A2 2 0 0 1 2 15.312V8.688a2 2 0 0 1 .586-1.414l4.688-4.688A2 2 0 0 1 8.688 2h6.624a2 2 0 0 1 1.414.586l4.688 4.688A2 2 0 0 1 22 8.688v6.624a2 2 0 0 1-.586 1.414l-4.688 4.688a2 2 0 0 1-1.414.586H8.688a2 2 0 0 1-1.414-.586z"/> <path d="m9 9 6 6"/>"#
            }
            Self::PanelLeft => {
                r#"<rect width="18" height="18" x="3" y="3" rx="2"/> <path d="M9 3v18"/>"#
            }
            Self::Paperclip => {
                r#"<path d="m16 6-8.414 8.586a2 2 0 0 0 2.829 2.829l8.414-8.586a4 4 0 1 0-5.657-5.657l-8.379 8.551a6 6 0 1 0 8.485 8.485l8.379-8.551"/>"#
            }
            Self::Plus => r#"<path d="M5 12h14"/> <path d="M12 5v14"/>"#,
            Self::Search => r#"<path d="m21 21-4.34-4.34"/> <circle cx="11" cy="11" r="8"/>"#,
            Self::Send => {
                r#"<path d="M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z"/> <path d="m21.854 2.147-10.94 10.939"/>"#
            }
            Self::Settings => {
                r#"<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/> <circle cx="12" cy="12" r="3"/>"#
            }
            Self::Star => {
                r#"<path d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z"/>"#
            }
            Self::Sun => {
                r#"<circle cx="12" cy="12" r="4"/> <path d="M12 2v2"/> <path d="M12 20v2"/> <path d="m4.93 4.93 1.41 1.41"/> <path d="m17.66 17.66 1.41 1.41"/> <path d="M2 12h2"/> <path d="M20 12h2"/> <path d="m6.34 17.66-1.41 1.41"/> <path d="m19.07 4.93-1.41 1.41"/>"#
            }
            Self::TextAlignCenter => {
                r#"<path d="M21 5H3"/> <path d="M17 12H7"/> <path d="M19 19H5"/>"#
            }
            Self::TextAlignEnd => {
                r#"<path d="M21 5H3"/> <path d="M21 12H9"/> <path d="M21 19H7"/>"#
            }
            Self::TextAlignStart => {
                r#"<path d="M21 5H3"/> <path d="M15 12H3"/> <path d="M17 19H3"/>"#
            }
            Self::Trash => {
                r#"<path d="M10 11v6"/> <path d="M14 11v6"/> <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/> <path d="M3 6h18"/> <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>"#
            }
            Self::TriangleAlert => {
                r#"<path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3"/> <path d="M12 9v4"/> <path d="M12 17h.01"/>"#
            }
            Self::Underline => {
                r#"<path d="M6 4v6a6 6 0 0 0 12 0V4"/> <line x1="4" x2="20" y1="20" y2="20"/>"#
            }
            Self::Upload => {
                r#"<path d="M12 3v12"/> <path d="m17 8-5-5-5 5"/> <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>"#
            }
            Self::User => {
                r#"<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/> <circle cx="12" cy="7" r="4"/>"#
            }
            Self::X => r#"<path d="M18 6 6 18"/> <path d="m6 6 12 12"/>"#,
        }
    }

    /// The full SVG source of the icon.
    pub fn svg(self) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" fill=\"none\" \
             stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" \
             stroke-linejoin=\"round\">{}</svg>",
            self.body()
        )
    }

    /// The parsed icon; parsed once per process. `None` only if the
    /// bundled source is invalid, which the icon tests rule out.
    pub fn document(self) -> Option<&'static SvgDocument> {
        static DOCUMENTS: OnceLock<Vec<Option<SvgDocument>>> = OnceLock::new();
        DOCUMENTS
            .get_or_init(|| {
                Icon::ALL
                    .iter()
                    .map(|icon| SvgDocument::parse(&icon.svg()).ok())
                    .collect()
            })
            .get(self as usize)?
            .as_ref()
    }

    /// Draws the icon scaled into `dest` in `color`.
    pub fn draw(
        self,
        list: &mut DisplayList,
        dest: Rect,
        color: Color,
    ) -> Result<(), DisplayListError> {
        match self.document() {
            Some(document) => document.draw(list, dest, color),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_parses() {
        for icon in Icon::ALL {
            let document = SvgDocument::parse(&icon.svg())
                .unwrap_or_else(|error| panic!("{}: {error}", icon.name()));
            assert!(!document.shapes.is_empty(), "{}", icon.name());
            assert!(icon.document().is_some());
        }
    }

    #[test]
    fn icons_draw_strokes_in_the_given_color() {
        let mut list = DisplayList::new();
        Icon::Check
            .draw(
                &mut list,
                Rect::from_xywh(0.0, 0.0, 16.0, 16.0),
                Color::WHITE,
            )
            .unwrap();
        assert!(!list.commands().is_empty());
    }
}
