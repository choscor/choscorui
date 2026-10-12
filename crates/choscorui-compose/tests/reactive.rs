use choscorui_compose::{ReactiveError, Runtime};
use std::{cell::RefCell, rc::Rc};

#[test]
fn derived_values_and_bindings_update_from_observables() {
    let runtime = Runtime::new();
    let quantity = runtime.property(2_i32);
    let price = runtime.property(5_i32);
    let total = runtime
        .derive2(&quantity, &price, |quantity, price| Ok(quantity * price))
        .unwrap();
    let label = runtime.property(String::new());
    runtime
        .bind(&total, &label, |value| Ok(format!("Total: {value}")))
        .unwrap();

    assert_eq!(total.get(), 10);
    assert_eq!(label.get(), "Total: 10");
    quantity.set(3);
    assert_eq!(total.get(), 15);
    assert_eq!(label.get(), "Total: 15");
}

#[test]
fn a_batch_notifies_each_changed_property_once_with_its_final_value() {
    let runtime = Runtime::new();
    let source = runtime.property(1_i32);
    let doubled = runtime.derive1(&source, |value| Ok(value * 2)).unwrap();
    let seen = Rc::new(RefCell::new(Vec::new()));
    let observer = seen.clone();
    doubled.subscribe(move |value| observer.borrow_mut().push(*value));

    runtime.batch(|| {
        source.set(2);
        source.set(3);
        source.set(4);
    });

    assert_eq!(doubled.get(), 8);
    assert_eq!(&*seen.borrow(), &[8]);
}

#[test]
fn cycles_are_rejected_without_changing_the_existing_binding_graph() {
    let runtime = Runtime::new();
    let first = runtime.property(1_i32);
    let second = runtime.property(0_i32);
    runtime.bind(&first, &second, |value| Ok(*value)).unwrap();

    assert_eq!(
        runtime.bind(&second, &first, |value| Ok(*value)),
        Err(ReactiveError::Cycle)
    );
    first.set(7);
    assert_eq!(second.get(), 7);
}

#[test]
fn failed_recomputation_keeps_last_good_value_and_reports_error() {
    let runtime = Runtime::new();
    let divisor = runtime.property(2_i32);
    let quotient = runtime
        .derive1(&divisor, |value| {
            if *value == 0 {
                Err(ReactiveError::Compute("division by zero".into()))
            } else {
                Ok(12 / value)
            }
        })
        .unwrap();

    divisor.set(0);

    assert_eq!(quotient.get(), 6);
    assert_eq!(
        runtime.take_errors(),
        vec![ReactiveError::Compute("division by zero".into())]
    );
}

#[test]
fn properties_from_different_runtimes_cannot_be_connected() {
    let first_runtime = Runtime::new();
    let second_runtime = Runtime::new();
    let source = first_runtime.property(1_i32);
    let target = second_runtime.property(2_i32);

    assert_eq!(
        first_runtime.bind(&source, &target, |value| Ok(*value)),
        Err(ReactiveError::DifferentRuntime)
    );
    assert_eq!(target.get(), 2);
}

#[test]
fn a_panicking_batch_does_not_disable_later_propagation() {
    let runtime = Runtime::new();
    let source = runtime.property(1_i32);
    let derived = runtime.derive1(&source, |value| Ok(value * 2)).unwrap();

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.batch(|| {
            source.set(2);
            panic!("abort transaction");
        });
    }));
    assert!(result.is_err());

    source.set(3);
    assert_eq!(derived.get(), 6);
}

#[test]
fn track_records_the_properties_read_once_each() {
    let runtime = Runtime::new();
    let a = runtime.property(1);
    let b = runtime.property(2);
    let other = Runtime::new().property(3);
    let (sum, reads) = runtime.track(|| a.get() + b.get() + a.get() + other.get());
    assert_eq!(sum, 7);
    assert_eq!(reads, vec![a.identity(), b.identity()]);
    let (_, outer) = runtime.track(|| {
        let (_, inner) = runtime.track(|| b.get());
        assert_eq!(inner, vec![b.identity()]);
        a.get_untracked()
    });
    assert!(outer.is_empty());
}

#[test]
fn computed_follows_the_branch_it_last_read() {
    let runtime = Runtime::new();
    let flag = runtime.property(true);
    let left = runtime.property(1);
    let right = runtime.property(10);
    let runs = Rc::new(RefCell::new(0));
    let counter = runs.clone();
    let (f, l, r) = (flag.clone(), left.clone(), right.clone());
    let value = runtime.computed(move || {
        *counter.borrow_mut() += 1;
        if f.get() { l.get() } else { r.get() }
    });
    assert_eq!(value.get(), 1);
    right.set(11);
    assert_eq!(*runs.borrow(), 1, "an unread property does not recompute");
    left.set(2);
    assert_eq!(value.get(), 2);
    flag.set(false);
    assert_eq!(value.get(), 11);
    left.set(3);
    assert_eq!(*runs.borrow(), 3, "the old branch no longer recomputes");
    right.set(12);
    assert_eq!(value.get(), 12);
}

#[test]
fn computed_values_are_current_inside_observers() {
    let runtime = Runtime::new();
    let count = runtime.property(1);
    let source = count.clone();
    let doubled = runtime.computed(move || source.get() * 2);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    let reader = doubled.clone();
    count.subscribe(move |_| sink.borrow_mut().push(reader.get()));
    count.set(5);
    assert_eq!(*seen.borrow(), vec![10]);
}

#[test]
fn effects_rerun_after_each_flush_until_unsubscribed() {
    let runtime = Runtime::new();
    let a = runtime.property(0);
    let b = runtime.property(0);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    let (ra, rb) = (a.clone(), b.clone());
    let effect = runtime.effect(move || sink.borrow_mut().push((ra.get(), rb.get())));
    runtime.batch(|| {
        a.set(1);
        b.set(2);
    });
    assert_eq!(*seen.borrow(), vec![(0, 0), (1, 2)], "one run per flush");
    runtime.unsubscribe(effect);
    a.set(3);
    assert_eq!(seen.borrow().len(), 2);
}

#[test]
fn disposed_properties_stop_notifying() {
    let runtime = Runtime::new();
    let a = runtime.property(0);
    let seen = Rc::new(RefCell::new(0));
    let sink = seen.clone();
    a.subscribe(move |_| *sink.borrow_mut() += 1);
    a.set(1);
    runtime.dispose(a.identity());
    a.set(2);
    assert_eq!(*seen.borrow(), 1);
    assert_eq!(a.get(), 2);
}

#[test]
fn values_that_keep_changing_each_other_report_a_cycle() {
    let runtime = Runtime::new();
    let a = runtime.property(0u64);
    let writer = a.clone();
    let _effect = runtime.effect(move || {
        let value = writer.get();
        writer.set(value + 1);
    });
    a.set(100);
    assert!(runtime.take_errors().contains(&ReactiveError::Cycle));
    let b = runtime.property(0);
    let seen = Rc::new(RefCell::new(0));
    let sink = seen.clone();
    b.subscribe(move |_| *sink.borrow_mut() += 1);
    b.set(1);
    assert_eq!(*seen.borrow(), 1, "propagation works after a cycle");
}
