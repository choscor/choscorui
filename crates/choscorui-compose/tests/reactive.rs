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
