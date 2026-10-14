//! The size grip page: a pane that the real size grip resizes.

use crate::ui::page::{PageCx, column, stateful};
use choscorui::compose::view::{AnyView, Frame, HStack, SizeGrip, Spacer, Text, VStack, ViewExt};
use choscorui::controls::ControlValue;
use choscorui::core::{ColorRole, RadiusScale, Size, Space};

/// A pane that the size grip in its bottom trailing corner resizes
/// (bottom left in right-to-left layouts), inside a minimum and a maximum
/// size. Escape during a drag puts the size back.
pub fn size_grip(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let size = cx.state((360.0, 220.0));
        let env = cx.env().clone();
        let grip = SizeGrip::new(
            Size::new(220.0, 140.0),
            Size::new(620.0, 400.0),
            move |new: Size| size.set((new.width, new.height)),
        )
        .value(move || {
            let (width, height) = size.get();
            ControlValue::Range(width, height)
        });
        let label = move || {
            let (width, height) = size.get();
            let (width, height) = (width.to_string(), height.to_string());
            env.text("size-grip.size", &[("width", &width), ("height", &height)])
        };
        // The size shows in the middle, and the grip sits in the corner.
        let pane = VStack::new((
            Spacer::new(),
            Text::new(label).foreground(ColorRole::MutedForeground),
            Spacer::new(),
            HStack::new((Spacer::new(), grip)),
        ))
        .padding(Space::S1)
        .background(ColorRole::Card)
        .border(ColorRole::Border, 1.0)
        .corner_radius(RadiusScale::Lg)
        .frame(move || {
            let (width, height) = size.get();
            Frame::size(width, height)
        });
        let name = cx.env().text("size-grip.window", &[]);
        column(pane.accessibility_label(name))
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, PointerButton, PointerEvent};

    #[test]
    fn the_real_grip_resizes_the_pane_within_the_limits_and_escape_cancels() {
        let mut page = Harness::new("size-grip");
        let pane = page.node("Mock window");
        let bounds = page.host.tree().bounds(pane).unwrap();
        assert!(page.host.find("360 by 220").is_some());
        let modifiers = ShortcutModifiers::default();
        let start = bounds
            .origin
            .offset(bounds.size.width - 8.0, bounds.size.height - 8.0);
        let press = PointerEvent::Down {
            position: start,
            button: PointerButton::Primary,
        };
        page.host.pointer(press, modifiers);
        let near = start.offset(20.0, 10.0);
        page.host
            .pointer(PointerEvent::Move { position: near }, modifiers);
        page.host.layout();
        assert!(page.host.find("380 by 230").is_some());
        // Escape puts the size back where the drag started.
        let escape = KeyEvent::Down {
            key: Key::Escape,
            repeat: false,
        };
        page.host.key(escape, modifiers);
        page.host.layout();
        assert!(page.host.find("360 by 220").is_some());
        // A new drag far out stops at the maximum.
        page.host.pointer(press, modifiers);
        let far = start.offset(1000.0, 1000.0);
        page.host
            .pointer(PointerEvent::Move { position: far }, modifiers);
        let release = PointerEvent::Up {
            position: far,
            button: PointerButton::Primary,
        };
        page.host.pointer(release, modifiers);
        page.host.layout();
        assert!(page.host.find("620 by 400").is_some());
        let node = page.node("Mock window");
        let pane = page.host.tree().bounds(node).unwrap();
        // The live frame resizes the pane from 360 by 220.
        let grown = (
            pane.size.width - bounds.size.width,
            pane.size.height - bounds.size.height,
        );
        assert_eq!(grown, (260.0, 180.0));
    }
}
