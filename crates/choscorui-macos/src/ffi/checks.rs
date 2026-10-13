//! Checks of every FFI descriptor against the Objective-C runtime.
//!
//! Each check collects all mismatches and then fails one time, so one run
//! shows the whole scope of an SDK change. A missing class, protocol, or
//! selector is a failure.

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
    [super::foundation::MESSAGES, super::appkit::MESSAGES]
        .into_iter()
        .flatten()
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

#[test]
fn ownership_follows_naming() {
    if let Err(report) = check_message_ownership(all_messages()) {
        panic!("{report}");
    }
}

#[test]
fn messages_match_runtime() {
    if let Err(report) = check_messages(all_messages()) {
        panic!("{report}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn every_failure_is_reported() {
        let first = message(c"isKindOfClass:", || "B@:q".to_owned());
        let second = message(c"choscoruiNoSuchSelector", || "v@:".to_owned());
        let report = check_messages([&first, &second]).unwrap_err();
        assert!(report.starts_with("2 message descriptors"), "{report}");
        assert!(report.contains("isKindOfClass:"), "{report}");
        assert!(report.contains("choscoruiNoSuchSelector"), "{report}");
    }
}
