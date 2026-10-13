//! The SDK header check: one Objective-C++ file, generated from every
//! descriptor, compiled against the macOS SDK.
//!
//! The file holds a `static_assert` for each C function argument and
//! result, each SDK constant, and each struct layout, and one function for
//! each message. The compiler reports every mismatch, deprecation, and
//! availability error in one run.

use super::MsgInfo;
use crate::ffi::callback::{ClassDef, Origin};
use crate::ffi::class::ReceiverKind;
use crate::ffi::header::{Check, HeaderType, Signature, receiver_header};
use crate::ffi::msg::Kind;
use crate::ffi::sdk::{CItem, CStruct, SdkConst, Shape};
use std::ffi::CStr;
use std::fmt::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The compiler flags after `xcrun clang++`. `-ferror-limit=0` makes the
/// compiler report every mismatch, not the first 20.
const FLAGS: &[&str] = &[
    "-x",
    "objective-c++",
    "-std=c++17",
    "-fsyntax-only",
    "-mmacosx-version-min=15.0",
    "-Werror",
    "-Wall",
    "-Wconversion",
    "-Wobjc-method-access",
    "-Wdeprecated-declarations",
    "-Wunguarded-availability-new",
    "-ferror-limit=0",
];

/// The headers and the shape helpers of the generated file.
const PRELUDE: &str = r#"#import <AppKit/AppKit.h>
#import <Carbon/Carbon.h>
#import <CoreText/CoreText.h>
#import <ImageIO/ImageIO.h>
#import <Metal/Metal.h>
#import <QuartzCore/QuartzCore.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>
#import <UserNotifications/UserNotifications.h>
#include <Block.h>
#include <objc/message.h>
#include <objc/runtime.h>
#include <pthread.h>
#include <cstddef>
#include <cstdint>
#include <tuple>
#include <type_traits>

namespace cc {
enum class Kind { Void, Pointer, Signed, Unsigned, Float, Struct, Array };

template <class T> constexpr Kind kind_of() {
  using U = std::remove_cv_t<T>;
  if constexpr (std::is_void_v<U>) return Kind::Void;
  else if constexpr (std::is_array_v<U>) return Kind::Array;
  else if constexpr (std::is_enum_v<U>) return kind_of<std::underlying_type_t<U>>();
  else if constexpr (std::is_floating_point_v<U>) return Kind::Float;
  else if constexpr (std::is_integral_v<U>) return std::is_signed_v<U> ? Kind::Signed : Kind::Unsigned;
  else if constexpr (std::is_class_v<U>) return Kind::Struct;
  else return Kind::Pointer;
}

template <class T> constexpr std::size_t size_of() {
  if constexpr (std::is_void_v<T>) return 0;
  else return sizeof(T);
}

// Class templates, so a failed assertion prints the header type.
template <class T, Kind kind, std::size_t size> struct is {
  static constexpr bool value = kind_of<T>() == kind && size_of<T>() == size;
};

template <class T, class S, std::size_t size> struct is_struct {
  static constexpr bool value = std::is_same_v<std::remove_cv_t<T>, S> && sizeof(T) == size;
};

template <class A, class B> constexpr bool same_shape() {
  if constexpr (std::is_class_v<A> || std::is_class_v<B>)
    return std::is_same_v<std::remove_cv_t<A>, std::remove_cv_t<B>>;
  else return kind_of<A>() == kind_of<B>() && size_of<A>() == size_of<B>();
}

template <class F> struct function;
template <class R, class... A> struct function<R (*)(A...)> {
  static constexpr std::size_t arity = sizeof...(A);
  using ret = R;
  template <std::size_t I> using arg = std::tuple_element_t<I, std::tuple<A...>>;
};
// libdispatch declares its functions `noexcept` in C++.
template <class R, class... A> struct function<R (*)(A...) noexcept> : function<R (*)(A...)> {};

template <class T> using type = T;

// A number argument: it converts to a parameter of the same ABI shape,
// enums included, and to nothing else.
template <class T> struct scalar {
  T value;
  template <class P> operator P() const {
    static_assert(same_shape<T, P>(), "a number argument has another ABI shape than the parameter");
    return static_cast<P>(value);
  }
};

// A pointer argument whose target type the descriptor does not name.
struct pointer {
  const void *value;
  template <class P> operator P *() const { return (P *)value; }
};
}
"#;

/// Every descriptor that the SDK header check compiles.
pub(crate) struct Descriptors<'a> {
    pub(crate) items: &'a [CItem],
    pub(crate) consts: &'a [SdkConst],
    pub(crate) structs: &'a [CStruct],
    pub(crate) messages: &'a [&'a MsgInfo],
    pub(crate) classes: &'a [ClassDef],
}

/// The C++ expression that is true when type `ty` has `shape`.
fn shape_test(ty: &str, shape: Shape) -> String {
    match shape {
        Shape::Void => format!("std::is_void_v<{ty}>"),
        Shape::Pointer => format!("cc::is<{ty}, cc::Kind::Pointer, sizeof(void *)>::value"),
        Shape::Signed(size) => format!("cc::is<{ty}, cc::Kind::Signed, {size}>::value"),
        Shape::Unsigned(size) => format!("cc::is<{ty}, cc::Kind::Unsigned, {size}>::value"),
        Shape::Float(size) => format!("cc::is<{ty}, cc::Kind::Float, {size}>::value"),
        Shape::Struct(name, size) => format!("cc::is_struct<{ty}, {name}, {size}>::value"),
        Shape::Opaque(name) => format!("std::is_same_v<std::remove_cv_t<{ty}>, {name}>"),
        Shape::Array(size) => format!("cc::is<{ty}, cc::Kind::Array, {size}>::value"),
    }
}

/// A description of `shape` for a failure message.
fn shape_name(shape: Shape) -> String {
    match shape {
        Shape::Void => "void".to_owned(),
        Shape::Pointer => "a pointer".to_owned(),
        Shape::Signed(size) => format!("a signed integer of {size} bytes"),
        Shape::Unsigned(size) => format!("an unsigned integer of {size} bytes"),
        Shape::Float(size) => format!("a floating-point number of {size} bytes"),
        Shape::Struct(name, size) => format!("struct {name} of {size} bytes"),
        Shape::Opaque(name) => name.to_owned(),
        Shape::Array(size) => format!("an array of {size} bytes"),
    }
}

/// Appends the assertions of one C function or static.
fn write_item(out: &mut String, item: &CItem) {
    match *item {
        CItem::Static { name, shape } => {
            let test = shape_test(&format!("decltype({name})"), shape);
            let expected = shape_name(shape);
            let _ = writeln!(
                out,
                "static_assert({test}, \"{name}: expected {expected}\");"
            );
        }
        CItem::Function {
            name, args, ret, ..
        } => {
            let function = format!("cc::function<decltype(&{name})>");
            let count = args.len();
            let _ = writeln!(
                out,
                "static_assert({function}::arity == {count}, \"{name}: expected {count} arguments\");"
            );
            let test = shape_test(&format!("{function}::ret"), ret);
            let expected = shape_name(ret);
            let _ = writeln!(
                out,
                "static_assert({test}, \"{name}: expected the result to be {expected}\");"
            );
            for (index, &arg) in args.iter().enumerate() {
                let test = shape_test(&format!("{function}::arg<{index}>"), arg);
                let expected = shape_name(arg);
                let _ = writeln!(
                    out,
                    "static_assert({test}, \"{name}: expected argument {index} to be {expected}\");"
                );
            }
        }
    }
}

/// Appends the assertion of one SDK constant. Each operand is widened
/// first, so constants of different enums combine without a warning.
fn write_const(out: &mut String, constant: &SdkConst) {
    let sdk = constant
        .sdk
        .split('|')
        .map(|symbol| format!("static_cast<__int128>({})", symbol.trim()))
        .collect::<Vec<_>>()
        .join(" | ");
    let value = constant.value;
    let literal = if value < 0 {
        format!("{value}LL")
    } else {
        format!("{value}ULL")
    };
    let _ = writeln!(
        out,
        "static_assert(({sdk}) == static_cast<__int128>({literal}), \"{}: expected {} to equal {value}\");",
        constant.name, constant.sdk
    );
}

/// Appends the assertions of one struct layout.
fn write_struct(out: &mut String, layout: &CStruct) {
    let name = layout.name;
    let _ = writeln!(
        out,
        "static_assert(sizeof({name}) == {}, \"{name}: size\");",
        layout.size
    );
    let _ = writeln!(
        out,
        "static_assert(alignof({name}) == {}, \"{name}: alignment\");",
        layout.align
    );
    for (field, offset) in layout.fields {
        let _ = writeln!(
            out,
            "static_assert(offsetof({name}, {field}) == {offset}, \"{name}: offset of {field}\");"
        );
    }
}

/// The parameter type that passes an argument of header type `arg`.
fn parameter(arg: &HeaderType) -> String {
    match arg.check {
        Check::Scalar => format!("cc::scalar<{}>", arg.name),
        Check::AnyPointer => "cc::pointer".to_owned(),
        Check::Typed => format!("cc::type<{}>", arg.name),
    }
}

/// The message send expression: `[receiver part:a0 part:a1]`.
fn message_expression(receiver: &str, selector: &str, count: usize) -> String {
    if count == 0 {
        return format!("[{receiver} {selector}]");
    }
    let parts = selector
        .split_terminator(':')
        .enumerate()
        .map(|(index, part)| format!("{part}:a{index}"))
        .collect::<Vec<_>>();
    format!("[{receiver} {}]", parts.join(" "))
}

/// A message that the check sends: a message descriptor, or a callback
/// that AppKit sends to a crate class.
struct Send<'a> {
    /// The name of the generated function.
    function: String,
    receiver: &'a CStr,
    receiver_kind: ReceiverKind,
    kind: Kind,
    selector: &'a CStr,
    signature: Signature,
}

impl Send<'_> {
    fn message(index: usize, message: &MsgInfo) -> Send<'_> {
        Send {
            function: format!("check_message_{index}"),
            receiver: message.receiver,
            receiver_kind: message.receiver_kind,
            kind: message.kind,
            selector: message.selector,
            signature: (message.signature)(),
        }
    }
}

/// Appends a function that sends `send` with the header types of its
/// descriptor. Compiling it checks the selector, the argument and result
/// types, the deprecation, and the availability.
fn write_send(out: &mut String, send: Send) {
    let signature = send.signature;
    let receiver_name = send.receiver.to_string_lossy();
    let selector = send.selector.to_string_lossy();
    let mut parameters = Vec::new();
    let receiver = match send.kind {
        Kind::Instance => {
            let ty = receiver_header(send.receiver, send.receiver_kind);
            parameters.push(format!("cc::type<{ty}> r"));
            "r".to_owned()
        }
        Kind::Class => receiver_name.clone().into_owned(),
    };
    for (position, arg) in signature.args.iter().enumerate() {
        parameters.push(format!("{} a{position}", parameter(arg)));
    }
    let expression = message_expression(&receiver, &selector, signature.args.len());
    let ret = &signature.ret;
    let _ = writeln!(out, "void {}({}) {{", send.function, parameters.join(", "));
    match ret.check {
        Check::Typed => {
            let _ = writeln!(out, "  cc::type<{}> result = {expression};", ret.name);
            let _ = writeln!(out, "  (void)result;");
        }
        Check::Scalar | Check::AnyPointer => {
            // The lambda body is a definition, so the compiler checks the
            // argument conversions; `decltype` alone would not.
            let _ = writeln!(out, "  auto send = [&] {{ return {expression}; }};");
            let _ = writeln!(
                out,
                "  static_assert(cc::same_shape<{}, decltype(send())>(), \"{receiver_name} {selector}: expected the result to be {}\");",
                ret.name, ret.name
            );
            let _ = writeln!(out, "  (void)send;");
        }
    }
    let _ = writeln!(out, "}}");
}

/// The header type of each type in a method type encoding, or `None` for
/// an encoding that the declarations of crate methods do not use.
fn encoded_types(types: &str) -> Option<Vec<&'static str>> {
    let mut out = Vec::new();
    let mut rest = types;
    while !rest.is_empty() {
        let (ty, tail) = match rest.as_bytes()[0] {
            b'v' => ("void", &rest[1..]),
            b'@' => ("id", &rest[1..]),
            b':' => ("SEL", &rest[1..]),
            b'B' | b'c' => ("BOOL", &rest[1..]),
            b'q' => ("NSInteger", &rest[1..]),
            b'Q' => ("NSUInteger", &rest[1..]),
            b'd' => ("double", &rest[1..]),
            _ => return None,
        };
        out.push(ty);
        rest = tail;
    }
    Some(out)
}

/// Appends an `@interface` for each crate class with the methods that the
/// crate invents, so the messages to those methods compile.
fn write_class(out: &mut String, class: &ClassDef) -> Result<(), String> {
    let name = class.name.to_string_lossy();
    let protocols = class
        .protocols
        .iter()
        .map(|protocol| protocol.to_string_lossy())
        .collect::<Vec<_>>();
    let adopts = if protocols.is_empty() {
        String::new()
    } else {
        format!(" <{}>", protocols.join(", "))
    };
    let _ = writeln!(
        out,
        "@interface {name} : {}{adopts}",
        class.superclass.to_string_lossy()
    );
    for method in class.methods.iter().filter(|method| method.is_own()) {
        let encoding = method.encoding();
        let selector = method.selector.to_string_lossy();
        let types = encoded_types(&encoding)
            .filter(|types| types.get(1..3) == Some(&["id", "SEL"]))
            .ok_or_else(|| format!("-[{name} {selector}]: unsupported encoding {encoding}"))?;
        let args = &types[3..];
        let declaration = if args.is_empty() {
            selector.clone().into_owned()
        } else {
            selector
                .split_terminator(':')
                .zip(args)
                .enumerate()
                .map(|(index, (part, ty))| format!("{part}:({ty})a{index}"))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let _ = writeln!(out, "- ({}){declaration};", types[0]);
    }
    let _ = writeln!(out, "@end");
    Ok(())
}

/// Appends a send of each protocol method that `class` implements, to a
/// receiver of the protocol. AppKit calls these methods, so the check
/// compiles the call as AppKit makes it. This check covers a protocol
/// that the runtime does not load, which the runtime check cannot see.
fn write_callbacks(out: &mut String, class: &ClassDef) {
    for (index, method) in class.methods.iter().enumerate() {
        if let Origin::Protocol(protocol) = method.origin {
            write_send(
                out,
                Send {
                    function: format!("check_callback_{}_{index}", class.name.to_string_lossy()),
                    receiver: protocol,
                    receiver_kind: ReceiverKind::Protocol,
                    kind: Kind::Instance,
                    selector: method.selector,
                    signature: (method.signature)(),
                },
            );
        }
    }
}

/// The Objective-C++ source of the check.
pub(crate) fn source(descriptors: &Descriptors) -> Result<String, String> {
    let mut out = PRELUDE.to_owned();
    for class in descriptors.classes {
        write_class(&mut out, class)?;
    }
    for item in descriptors.items {
        write_item(&mut out, item);
    }
    for constant in descriptors.consts {
        write_const(&mut out, constant);
    }
    for layout in descriptors.structs {
        write_struct(&mut out, layout);
    }
    for (index, message) in descriptors.messages.iter().enumerate() {
        write_send(&mut out, Send::message(index, message));
    }
    for class in descriptors.classes {
        write_callbacks(&mut out, class);
    }
    Ok(out)
}

/// The path of the macOS SDK.
fn sdk_path() -> Result<String, String> {
    let output = Command::new("xcrun")
        .args(["--show-sdk-path"])
        .output()
        .map_err(|error| {
            format!(
                "xcrun did not start ({error}). The SDK header check needs the Xcode \
                 Command Line Tools; see the prerequisites in CONTRIBUTING.md."
            )
        })?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || path.is_empty() {
        return Err(format!(
            "xcrun found no macOS SDK ({}). The SDK header check needs the Xcode \
             Command Line Tools; see the prerequisites in CONTRIBUTING.md.",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(path)
}

/// A new, empty directory under the temporary directory.
fn fresh_directory() -> Result<PathBuf, String> {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let directory = std::env::temp_dir().join(format!(
        "choscorui-sdk-check-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    if directory.exists() {
        std::fs::remove_dir_all(&directory).map_err(|error| error.to_string())?;
    }
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

/// Compiles `source` against the SDK headers. The error holds the whole
/// compiler output.
pub(crate) fn compile(source: &str) -> Result<(), String> {
    let sdk = sdk_path()?;
    let directory = fresh_directory()?;
    let file = directory.join("sdk_check.mm");
    std::fs::write(&file, source).map_err(|error| error.to_string())?;
    let output = Command::new("xcrun")
        .arg("clang++")
        .args(FLAGS)
        .arg("-isysroot")
        .arg(&sdk)
        .arg(&file)
        .output();
    let _ = std::fs::remove_dir_all(&directory);
    let output = output.map_err(|error| {
        format!(
            "xcrun clang++ did not start ({error}). The SDK header check needs the Xcode \
             Command Line Tools; see the prerequisites in CONTRIBUTING.md."
        )
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "the SDK headers reject the FFI declarations:\n{}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Generates the check from `descriptors` and compiles it.
pub(crate) fn check_sdk(descriptors: &Descriptors) -> Result<(), String> {
    compile(&source(descriptors)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::geometry::NSRange;
    use crate::ffi::header::signature;
    use crate::ffi::msg::Ownership;

    fn check(descriptors: Descriptors) -> String {
        check_sdk(&descriptors).unwrap_err()
    }

    fn only_consts(consts: &[SdkConst]) -> Descriptors<'_> {
        Descriptors {
            items: &[],
            consts,
            structs: &[],
            messages: &[],
            classes: &[],
        }
    }

    fn message(
        receiver: &'static CStr,
        selector: &'static CStr,
        signature: fn() -> crate::ffi::header::Signature,
    ) -> MsgInfo {
        MsgInfo {
            receiver,
            receiver_kind: ReceiverKind::Class,
            implementation: None,
            kind: Kind::Instance,
            selector,
            ownership: Ownership::Value,
            encoding: String::new,
            signature,
        }
    }

    const WRONG_CONST: SdkConst = SdkConst {
        name: "FULL_SCREEN",
        sdk: "NSWindowStyleMaskFullScreen",
        value: 1 << 13,
    };

    const WRONG_STRUCT: CStruct = CStruct {
        name: "CGPoint",
        size: 24,
        align: 8,
        fields: &[("x", 0), ("y", 8)],
    };

    #[test]
    fn a_wrong_constant_names_the_symbol_and_both_values() {
        let report = check(only_consts(&[WRONG_CONST]));
        assert!(
            report.contains("FULL_SCREEN: expected NSWindowStyleMaskFullScreen to equal 8192"),
            "{report}"
        );
        assert!(report.contains("'16384 == 8192'"), "{report}");
    }

    #[test]
    fn a_wrong_struct_size_names_the_struct_and_both_sizes() {
        let report = check(Descriptors {
            structs: &[WRONG_STRUCT],
            ..only_consts(&[])
        });
        assert!(report.contains("CGPoint: size"), "{report}");
        assert!(report.contains("'16 == 24'"), "{report}");
    }

    #[test]
    fn a_wrong_function_shape_names_the_function_and_the_header_type() {
        let report = check(Descriptors {
            items: &[CItem::Function {
                name: "CFGetTypeID",
                args: &[Shape::Pointer],
                ret: Shape::Signed(8),
                owned: false,
            }],
            ..only_consts(&[])
        });
        assert!(
            report.contains("CFGetTypeID: expected the result to be a signed integer of 8 bytes"),
            "{report}"
        );
        assert!(report.contains("unsigned long"), "{report}");
    }

    #[test]
    fn a_wrong_message_argument_names_the_selector_and_both_types() {
        let wrong = message(
            c"NSApplication",
            c"setActivationPolicy:",
            signature::<(usize,), bool>,
        );
        let report = check(Descriptors {
            messages: &[&wrong],
            ..only_consts(&[])
        });
        assert!(report.contains("setActivationPolicy:"), "{report}");
        assert!(
            report.contains("unsigned long, NSApplicationActivationPolicy"),
            "{report}"
        );
    }

    #[test]
    fn a_deprecated_message_fails() {
        let deprecated = message(
            c"NSWindow",
            c"disableScreenUpdatesUntilFlush",
            signature::<(), ()>,
        );
        let report = check(Descriptors {
            messages: &[&deprecated],
            ..only_consts(&[])
        });
        assert!(
            report.contains("'disableScreenUpdatesUntilFlush' is deprecated"),
            "{report}"
        );
    }

    /// A class with one protocol callback.
    fn callback_class(
        protocol: &'static CStr,
        selector: &'static CStr,
        signature: fn() -> crate::ffi::header::Signature,
    ) -> ClassDef {
        let method = crate::ffi::callback::Method {
            selector,
            imp: std::ptr::null(),
            encode: |_| {},
            origin: Origin::Protocol(protocol),
            signature,
        };
        ClassDef {
            name: c"ChoscoruiCheckDelegate",
            superclass: c"NSObject",
            protocols: &[],
            methods: Vec::leak(vec![method]),
        }
    }

    #[test]
    fn a_wrong_callback_argument_names_the_selector_and_both_types() {
        let wrong = callback_class(
            c"NSApplicationDelegate",
            c"applicationDidBecomeActive:",
            signature::<(NSRange,), ()>,
        );
        let report = check(Descriptors {
            classes: &[wrong],
            ..only_consts(&[])
        });
        assert!(report.contains("applicationDidBecomeActive:"), "{report}");
        assert!(report.contains("NSRange"), "{report}");
        assert!(report.contains("NSNotification"), "{report}");
    }

    #[test]
    fn a_callback_of_an_unknown_protocol_fails() {
        let unknown = callback_class(
            c"ChoscoruiNoSuchProtocol",
            c"applicationDidBecomeActive:",
            signature::<(), ()>,
        );
        let report = check(Descriptors {
            classes: &[unknown],
            ..only_consts(&[])
        });
        assert!(report.contains("ChoscoruiNoSuchProtocol"), "{report}");
    }

    #[test]
    fn every_failure_is_reported() {
        let report = check(Descriptors {
            structs: &[WRONG_STRUCT],
            ..only_consts(&[WRONG_CONST])
        });
        assert!(report.contains("FULL_SCREEN"), "{report}");
        assert!(report.contains("CGPoint: size"), "{report}");
    }
}
