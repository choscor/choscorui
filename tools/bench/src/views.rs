//! The view-update scenario: one state change in a large view tree. The
//! work and the number of changed nodes must not grow with the tree.

use crate::scenarios::{Config, ms, present, settle};
use crate::stats::Results;
use choscorui_compose::view::{
    ForEach, HStack, HorizontalAlignment, ScrollView, Spacer, Text, VStack, ViewExt, ViewHost,
};
use choscorui_compose::{Property, Runtime};
use choscorui_controls::ShapedText;
use choscorui_core::{Catalog, DesignTokens, Locale, Size, Space};
use choscorui_macos::{Application, CoreTextShaper, Window};
use std::rc::Rc;
use std::time::Instant;

static SHAPER: CoreTextShaper = CoreTextShaper;

/// The tree size that the dirty count of the large tree is compared with.
const SMALL_ROWS: usize = 1_000;

/// A scrolling column of `rows` rows. Each row has a fixed label and a
/// value that reads its own property.
fn mount(size: Size, rows: usize) -> (ViewHost, Vec<Property<u32>>) {
    let runtime = Runtime::new();
    let values: Vec<Property<u32>> = (0..rows).map(|_| runtime.property(0)).collect();
    let env = choscorui_compose::view::Env::new(
        Rc::new(Catalog::new()),
        Locale::parse("en").expect("valid tag"),
    );
    let mut host = ViewHost::new(
        size,
        DesignTokens::light(),
        env,
        runtime,
        Rc::new(ShapedText::new(&SHAPER)),
    );
    let shown = values.clone();
    host.mount(ScrollView::new(
        VStack::new(ForEach::new(
            (0..rows).collect::<Vec<_>>(),
            |row| *row,
            move |row: &usize| {
                let value = shown[*row].clone();
                HStack::new((
                    Text::new(format!("Row {row}")),
                    Spacer::new(),
                    Text::new(move || format!("{} edits", value.get())),
                ))
                .padding(Space::S1)
            },
        ))
        .alignment(HorizontalAlignment::Fill),
    ));
    host.layout();
    (host, values)
}

/// Changes one visible row and returns how many nodes changed.
fn change(host: &mut ViewHost, values: &[Property<u32>], step: usize) -> usize {
    let value = &values[step % 20];
    value.set(value.get() + 1);
    host.flush();
    host.take_dirty_count()
}

pub fn views(app: &Application, window: &mut Window, config: &Config, results: &mut Results) {
    let size = window.content_size();
    let started = Instant::now();
    let (mut small, small_values) = mount(size, SMALL_ROWS);
    let mounted_small = ms(started.elapsed());
    small.take_dirty_count();
    let small_dirty = change(&mut small, &small_values, 0);
    drop(small);

    let started = Instant::now();
    let (mut host, values) = mount(size, config.view_rows);
    results.metric("views.mount_ms", ms(started.elapsed()));
    results.metric("views.mount_small_ms", mounted_small);
    window.set_display_list(host.paint());
    settle(app, window, std::time::Duration::from_millis(300));
    host.take_dirty_count();

    let (mut update, mut latency, mut dirty) = (Vec::new(), Vec::new(), Vec::new());
    let mut missed = 0;
    for step in 0..config.view_updates {
        let paints = window.paint_count();
        let started = Instant::now();
        dirty.push(change(&mut host, &values, step) as f64);
        window.set_display_list(host.paint());
        update.push(ms(started.elapsed()));
        match present(app, window, paints) {
            Some(paint) => latency.push(ms(paint.finished - started)),
            None => missed += 1,
        }
        window.take_events();
    }
    let peak = dirty.iter().copied().fold(0.0, f64::max);
    results.metric("views.rows", config.view_rows as f64);
    results.metric("views.dirty_nodes", peak);
    results.metric("views.dirty_growth", peak - small_dirty as f64);
    results.samples("views.update_ms", update);
    results.samples("views.present_ms", latency);
    results.metric("views.missed_present", missed as f64);
    results.metric("views.errors", host.take_errors().len() as f64);
}
