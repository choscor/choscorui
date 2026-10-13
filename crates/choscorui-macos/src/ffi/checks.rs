//! Checks of every FFI descriptor and callback against the Objective-C
//! runtime.
//!
//! Each check collects all mismatches and then fails one time, so one run
//! shows the whole scope of an SDK change. A missing class, protocol, or
//! selector is a failure.

use super::callback::{ClassDef, Origin};
use super::class::{ReceiverKind, Sel, lookup_class};
use super::encode::{same_encoding, selector_family};
use super::msg::{Kind, Ownership};
use super::runtime;
use std::ffi::CStr;

/// The data of one message descriptor.
pub(crate) struct MsgInfo {
    pub(crate) receiver: &'static CStr,
    pub(crate) receiver_kind: ReceiverKind,
    pub(crate) kind: Kind,
    pub(crate) selector: &'static CStr,
    pub(crate) ownership: Ownership,
    pub(crate) encoding: fn() -> String,
}

/// Every registered message descriptor.
fn all_messages() -> Vec<&'static MsgInfo> {
    [
        super::foundation::MESSAGES,
        super::appkit::MESSAGES,
        super::user_notifications::MESSAGES,
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Registers every class that this crate defines, so the checks find
/// them, and returns their definitions. A class that fails to register is
/// reported as missing.
fn crate_classes() -> Vec<ClassDef> {
    super::classes::CLASSES
        .iter()
        .map(|class| {
            let _ = (class.register)();
            (class.definition)()
        })
        .collect()
}

/// The type encoding that the runtime has for `message`.
fn runtime_encoding(message: &MsgInfo) -> Result<String, String> {
    let selector = Sel::register(message.selector);
    let instance = message.kind == Kind::Instance;
    let types = match message.receiver_kind {
        ReceiverKind::Class => {
            let class = lookup_class(message.receiver)
                .ok_or_else(|| format!("class {:?} is missing", message.receiver))?;
            runtime::method_types(class, selector, instance)
        }
        ReceiverKind::Protocol => {
            let protocol = runtime::get_protocol(message.receiver)
                .ok_or_else(|| format!("protocol {:?} is missing", message.receiver))?;
            runtime::protocol_method_types(protocol, selector, instance)
        }
    };
    types.ok_or_else(|| {
        format!(
            "{:?} has no {} method {:?}",
            message.receiver,
            if instance { "instance" } else { "class" },
            message.selector
        )
    })
}

/// Compares each message with the runtime and reports every mismatch.
pub(crate) fn check_messages<'a>(
    messages: impl IntoIterator<Item = &'a MsgInfo>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for message in messages {
        let expected = (message.encoding)();
        match runtime_encoding(message) {
            Err(failure) => failures.push(failure),
            Ok(actual) if !same_encoding(&expected, &actual) => failures.push(format!(
                "{:?} {:?}: expected {expected}, actual {actual}",
                message.receiver, message.selector
            )),
            Ok(_) => {}
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} message descriptors do not match the runtime:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

/// Checks that a message returns a +1 object exactly when its selector is
/// in the `alloc`, `new`, `copy`, `mutableCopy`, or `init` family.
pub(crate) fn check_message_ownership<'a>(
    messages: impl IntoIterator<Item = &'a MsgInfo>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for message in messages {
        let selector = message.selector.to_string_lossy();
        let family = selector_family(&selector);
        let owned = message.ownership == Ownership::Owned;
        if family.is_some() != owned {
            failures.push(format!(
                "{:?} {selector:?}: expected {}, actual {:?}",
                message.receiver,
                family.map_or("a result that is not +1".to_owned(), |family| format!(
                    "a +1 result of the {family} family"
                )),
                message.ownership
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} descriptors break the ownership naming rules:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

/// The type encoding of `selector` in its origin, or why there is none.
fn origin_encoding(origin: Origin, selector: Sel) -> Result<String, String> {
    match origin {
        Origin::Super(name) => {
            let class = lookup_class(name).ok_or_else(|| format!("class {name:?} is missing"))?;
            runtime::method_types(class, selector, true)
                .ok_or_else(|| format!("superclass {name:?} has no such instance method"))
        }
        Origin::Protocol(name) => {
            let protocol = runtime::get_protocol(name)
                .ok_or_else(|| format!("protocol {name:?} is missing"))?;
            runtime::protocol_method_types(protocol, selector, true)
                .ok_or_else(|| format!("protocol {name:?} has no such instance method"))
        }
        Origin::Own => Err("an own selector has no origin".to_owned()),
    }
}

/// Where the superclass or an adopted protocol of `class` already
/// declares `selector`.
fn own_collisions(class: &ClassDef, selector: Sel) -> Vec<String> {
    let mut found = Vec::new();
    match lookup_class(class.superclass) {
        None => found.push(format!("class {:?} is missing", class.superclass)),
        Some(superclass) => {
            if let Some(types) = runtime::method_types(superclass, selector, true) {
                found.push(format!(
                    "superclass {:?} declares {types}",
                    class.superclass
                ));
            }
        }
    }
    for &name in class.protocols {
        match runtime::get_protocol(name) {
            None => found.push(format!("protocol {name:?} is missing")),
            Some(protocol) => {
                if let Some(types) = runtime::protocol_method_types(protocol, selector, true) {
                    found.push(format!("protocol {name:?} declares {types}"));
                }
            }
        }
    }
    found
}

/// Compares each callback with the superclass method or the protocol
/// method that it implements, and checks that each own selector is new.
pub(crate) fn check_callbacks<'a>(
    classes: impl IntoIterator<Item = &'a ClassDef>,
) -> Result<(), String> {
    let mut failures = Vec::new();
    for class in classes {
        for method in class.methods {
            let selector = Sel::register(method.selector);
            let expected = method.encoding();
            let place = format!("{:?} {:?}", class.name, method.selector);
            if let Origin::Own = method.origin {
                for collision in own_collisions(class, selector) {
                    failures.push(format!("{place}: own selector, but {collision}"));
                }
                continue;
            }
            match origin_encoding(method.origin, selector) {
                Err(failure) => failures.push(format!("{place}: {failure}")),
                Ok(actual) if !same_encoding(&expected, &actual) => {
                    failures.push(format!("{place}: expected {expected}, actual {actual}"));
                }
                Ok(_) => {}
            }
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{} callbacks do not match the SDK:\n{}",
            failures.len(),
            failures.join("\n")
        ))
    }
}

#[test]
fn callbacks_match_sdk() {
    if let Err(report) = check_callbacks(&crate_classes()) {
        panic!("{report}");
    }
}

#[test]
fn ownership_follows_naming() {
    if let Err(report) = check_message_ownership(all_messages()) {
        panic!("{report}");
    }
}

#[test]
fn messages_match_runtime() {
    crate_classes();
    if let Err(report) = check_messages(all_messages()) {
        panic!("{report}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::callback::{EncodeFn, Method, ReturnedObject, method_encoding};
    use crate::ffi::geometry::NSPoint;

    fn message(selector: &'static CStr, encoding: fn() -> String) -> MsgInfo {
        MsgInfo {
            receiver: c"NSObject",
            receiver_kind: ReceiverKind::Class,
            kind: Kind::Instance,
            selector,
            ownership: Ownership::Value,
            encoding,
        }
    }

    #[test]
    fn a_wrong_argument_type_names_both_encodings() {
        let wrong = message(c"isKindOfClass:", || "B@:q".to_owned());
        let report = check_messages([&wrong]).unwrap_err();
        assert!(report.contains("\"NSObject\""), "{report}");
        assert!(report.contains("isKindOfClass:"), "{report}");
        assert!(report.contains("expected B@:q"), "{report}");
        assert!(report.contains("actual"), "{report}");
        assert!(report.contains('#'), "{report}");
    }

    #[test]
    fn a_missing_selector_is_a_failure() {
        let missing = message(c"choscoruiNoSuchSelector", || "v@:".to_owned());
        let report = check_messages([&missing]).unwrap_err();
        assert!(report.contains("choscoruiNoSuchSelector"), "{report}");
        let class = MsgInfo {
            receiver: c"ChoscoruiNoSuchClass",
            ..message(c"init", || "@@:".to_owned())
        };
        assert!(
            check_messages([&class])
                .unwrap_err()
                .contains("ChoscoruiNoSuchClass")
        );
    }

    #[test]
    fn a_wrong_ownership_family_names_the_selector() {
        let alloc = MsgInfo {
            kind: Kind::Class,
            ..message(c"alloc", || "@@:".to_owned())
        };
        let report = check_message_ownership([&alloc]).unwrap_err();
        assert!(report.contains("\"alloc\""), "{report}");
        assert!(report.contains("alloc family"), "{report}");
        assert!(report.contains("Value"), "{report}");
        let retained = MsgInfo {
            ownership: Ownership::Owned,
            ..message(c"description", || "@@:".to_owned())
        };
        let report = check_message_ownership([&retained]).unwrap_err();
        assert!(report.contains("not +1"), "{report}");
        assert!(report.contains("Owned"), "{report}");
    }

    #[test]
    fn protocol_methods_are_checked() {
        let protocol = |receiver, encoding: fn() -> String| MsgInfo {
            receiver,
            receiver_kind: ReceiverKind::Protocol,
            ..message(c"draggingSourceOperationMask", encoding)
        };
        assert!(check_messages([&protocol(c"NSDraggingInfo", || "Q@:".to_owned())]).is_ok());
        let wrong = protocol(c"NSDraggingInfo", || "q@:".to_owned());
        let report = check_messages([&wrong]).unwrap_err();
        assert!(report.contains("expected q@:"), "{report}");
        let missing = protocol(c"ChoscoruiNoSuchProtocol", || "Q@:".to_owned());
        let report = check_messages([&missing]).unwrap_err();
        assert!(
            report.contains("protocol \"ChoscoruiNoSuchProtocol\" is missing"),
            "{report}"
        );
    }

    fn callback(origin: Origin, selector: &'static CStr, encode: EncodeFn) -> Method {
        Method {
            selector,
            imp: std::ptr::null(),
            encode,
            origin,
        }
    }

    fn class(protocols: Vec<&'static CStr>, methods: Vec<Method>) -> ClassDef {
        ClassDef {
            name: c"ChoscoruiV2CheckedClass",
            superclass: c"NSObject",
            protocols: protocols.leak(),
            methods: methods.leak(),
        }
    }

    #[test]
    fn a_wrong_callback_type_names_both_encodings() {
        let wrong = class(
            Vec::new(),
            vec![callback(
                Origin::Super(c"NSObject"),
                c"isEqual:",
                method_encoding::<(), (isize,)>,
            )],
        );
        let report = check_callbacks([&wrong]).unwrap_err();
        assert!(report.contains("\"ChoscoruiV2CheckedClass\""), "{report}");
        assert!(report.contains("isEqual:"), "{report}");
        assert!(report.contains("expected v@:q"), "{report}");
        assert!(report.contains("actual"), "{report}");
        let missing = class(
            Vec::new(),
            vec![callback(
                Origin::Protocol(c"NSDraggingInfo"),
                c"choscoruiNoSuchSelector",
                method_encoding::<(), ()>,
            )],
        );
        let report = check_callbacks([&missing]).unwrap_err();
        assert!(report.contains("choscoruiNoSuchSelector"), "{report}");
        assert!(
            report.contains("\"NSDraggingInfo\" has no such"),
            "{report}"
        );
    }

    #[test]
    fn a_colliding_own_selector_names_its_origin() {
        let colliding = class(
            vec![c"NSDraggingInfo"],
            vec![
                callback(
                    Origin::Own,
                    c"description",
                    method_encoding::<ReturnedObject<()>, ()>,
                ),
                callback(
                    Origin::Own,
                    c"draggingLocation",
                    method_encoding::<NSPoint, ()>,
                ),
                callback(
                    Origin::Own,
                    c"choscoruiNewSelector",
                    method_encoding::<(), ()>,
                ),
            ],
        );
        let report = check_callbacks([&colliding]).unwrap_err();
        // NSDraggingInfo incorporates the NSObject protocol, which also
        // declares `description`.
        assert!(report.starts_with("3 callbacks"), "{report}");
        assert!(report.contains("\"description\""), "{report}");
        assert!(
            report.contains("superclass \"NSObject\" declares @"),
            "{report}"
        );
        assert!(
            report.contains("protocol \"NSDraggingInfo\" declares @"),
            "{report}"
        );
        assert!(report.contains("\"draggingLocation\""), "{report}");
        assert!(
            report.contains("protocol \"NSDraggingInfo\" declares {CGPoint=dd}"),
            "{report}"
        );
        assert!(!report.contains("choscoruiNewSelector"), "{report}");
    }

    #[test]
    fn every_failure_is_reported() {
        let first = message(c"isKindOfClass:", || "B@:q".to_owned());
        let second = message(c"choscoruiNoSuchSelector", || "v@:".to_owned());
        let report = check_messages([&first, &second]).unwrap_err();
        assert!(report.starts_with("2 message descriptors"), "{report}");
        assert!(report.contains("isKindOfClass:"), "{report}");
        assert!(report.contains("choscoruiNoSuchSelector"), "{report}");
    }
}
