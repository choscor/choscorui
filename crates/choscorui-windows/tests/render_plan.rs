use choscorui_core::{Color, DisplayList, Point, Rect, Size};
use choscorui_windows::{
    DeviceEvent, DeviceResources, FrameResult, RenderOp, RenderPlan, RenderPlanError,
};

#[test]
fn plan_preserves_painter_order_nested_clips_and_owned_text() {
    let mut source = DisplayList::new();
    source.push_clip(Rect::new(Point::new(1.0, 2.0), Size::new(80.0, 40.0)));
    source.fill_rect(
        Rect::new(Point::new(3.0, 4.0), Size::new(20.0, 10.0)),
        Color::rgba8(10, 20, 30, 128),
    );
    source
        .draw_text(
            Point::new(7.0, 19.0),
            "Xin chào 👋",
            "Segoe UI",
            14.0,
            Color::rgba8(240, 230, 220, 255),
        )
        .unwrap();
    source.pop_clip().unwrap();

    let plan = RenderPlan::compile(&source).unwrap();
    drop(source);

    assert_eq!(plan.max_clip_depth(), 1);
    assert!(matches!(plan.operations()[0], RenderOp::PushClip(_)));
    assert!(matches!(plan.operations()[1], RenderOp::FillRect { .. }));
    assert!(matches!(
        &plan.operations()[2],
        RenderOp::DrawText { text, font_family, .. }
            if text == "Xin chào 👋" && font_family == "Segoe UI"
    ));
    assert!(matches!(plan.operations()[3], RenderOp::PopClip));
}

#[test]
fn plan_rejects_an_unclosed_clip_before_rendering() {
    let mut source = DisplayList::new();
    source.push_clip(Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0)));
    assert_eq!(
        RenderPlan::compile(&source),
        Err(RenderPlanError::UnbalancedClip)
    );
}

#[test]
fn target_loss_releases_device_resources_and_requests_recreation() {
    let mut resources = DeviceResources::new();
    assert_eq!(resources.begin_frame(), DeviceEvent::CreateTarget);
    resources.target_created();
    assert_eq!(resources.begin_frame(), DeviceEvent::Ready);
    assert_eq!(
        resources.end_frame(FrameResult::TargetLost),
        DeviceEvent::CreateTarget
    );
    assert!(!resources.has_target());
    resources.target_created();
    assert_eq!(
        resources.end_frame(FrameResult::Presented),
        DeviceEvent::Ready
    );
    resources.release();
    assert!(!resources.has_target());
}
