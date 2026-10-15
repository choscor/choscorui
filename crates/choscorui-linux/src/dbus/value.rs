//! The values of D-Bus messages as owned Rust data.

use super::names;

/// One value of a message body. A container names the signature of its
/// items, so an empty array keeps its type.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Byte(u8),
    Bool(bool),
    Int16(i16),
    Uint16(u16),
    Int32(i32),
    Uint32(u32),
    Int64(i64),
    Uint64(u64),
    Double(f64),
    Str(String),
    ObjectPath(String),
    Signature(String),
    /// An array of `items`, each of the single complete type `element`.
    Array {
        element: String,
        items: Vec<Value>,
    },
    /// An array of dictionary entries: `a{<key><value>}`.
    Dict {
        key: String,
        value: String,
        entries: Vec<(Value, Value)>,
    },
    Struct(Vec<Value>),
    Variant(Box<Value>),
}

/// Why a value cannot go into a message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvalidValue(pub String);

impl std::fmt::Display for InvalidValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid D-Bus value: {}", self.0)
    }
}

impl std::error::Error for InvalidValue {}

impl Value {
    /// A string.
    pub fn str(text: impl Into<String>) -> Self {
        Self::Str(text.into())
    }

    /// An object path.
    pub fn object_path(path: impl Into<String>) -> Self {
        Self::ObjectPath(path.into())
    }

    /// A variant that holds `value`.
    pub fn variant(value: impl Into<Value>) -> Self {
        Self::Variant(Box::new(value.into()))
    }

    /// An array of `items` of the type `element`.
    pub fn array(element: impl Into<String>, items: Vec<Value>) -> Self {
        Self::Array {
            element: element.into(),
            items,
        }
    }

    /// An array of strings (`as`).
    pub fn strings<S: Into<String>>(items: impl IntoIterator<Item = S>) -> Self {
        Self::array(
            "s",
            items
                .into_iter()
                .map(|item| Self::Str(item.into()))
                .collect(),
        )
    }

    /// A dictionary of strings to variants (`a{sv}`). Each value goes in
    /// a variant.
    pub fn vardict<S: Into<String>>(entries: impl IntoIterator<Item = (S, Value)>) -> Self {
        Self::Dict {
            key: "s".to_owned(),
            value: "v".to_owned(),
            entries: entries
                .into_iter()
                .map(|(key, value)| (Self::Str(key.into()), Self::variant(value)))
                .collect(),
        }
    }

    /// The signature of the value: one complete type.
    pub fn signature(&self) -> String {
        match self {
            Self::Byte(_) => "y".to_owned(),
            Self::Bool(_) => "b".to_owned(),
            Self::Int16(_) => "n".to_owned(),
            Self::Uint16(_) => "q".to_owned(),
            Self::Int32(_) => "i".to_owned(),
            Self::Uint32(_) => "u".to_owned(),
            Self::Int64(_) => "x".to_owned(),
            Self::Uint64(_) => "t".to_owned(),
            Self::Double(_) => "d".to_owned(),
            Self::Str(_) => "s".to_owned(),
            Self::ObjectPath(_) => "o".to_owned(),
            Self::Signature(_) => "g".to_owned(),
            Self::Array { element, .. } => format!("a{element}"),
            Self::Dict { key, value, .. } => format!("a{{{key}{value}}}"),
            Self::Struct(fields) => {
                let inner: String = fields.iter().map(Self::signature).collect();
                format!("({inner})")
            }
            Self::Variant(_) => "v".to_owned(),
        }
    }

    /// Checks that libdbus can marshal the value: valid text, paths, and
    /// signatures, and container items of the declared type.
    pub fn check(&self) -> Result<(), InvalidValue> {
        let fail = |reason: String| Err(InvalidValue(reason));
        match self {
            Self::Str(text) if text.contains('\0') => fail("a string contains NUL".to_owned()),
            Self::ObjectPath(path) if !names::is_object_path(path) => {
                fail(format!("{path:?} is not an object path"))
            }
            Self::Signature(signature) if !names::is_signature(signature) => {
                fail(format!("{signature:?} is not a signature"))
            }
            Self::Array { element, items } => {
                if !names::is_single_type(element) {
                    return fail(format!("{element:?} is not one complete type"));
                }
                for item in items {
                    if &item.signature() != element {
                        return fail(format!(
                            "an item of type {} is in an array of {element}",
                            item.signature()
                        ));
                    }
                    item.check()?;
                }
                self.check_depth()
            }
            Self::Dict {
                key,
                value,
                entries,
            } => {
                let key_basic = key.len() == 1 && names::is_basic_type(key.as_bytes()[0]);
                if !key_basic || !names::is_single_type(value) {
                    return fail(format!("a{{{key}{value}}} is not a dictionary type"));
                }
                for (entry_key, entry_value) in entries {
                    if &entry_key.signature() != key || &entry_value.signature() != value {
                        return fail(format!(
                            "an entry of type {{{}{}}} is in a dictionary of {{{key}{value}}}",
                            entry_key.signature(),
                            entry_value.signature()
                        ));
                    }
                    entry_key.check()?;
                    entry_value.check()?;
                }
                self.check_depth()
            }
            Self::Struct(fields) => {
                if fields.is_empty() {
                    return fail("a struct has no fields".to_owned());
                }
                fields.iter().try_for_each(Self::check)?;
                self.check_depth()
            }
            Self::Variant(inner) => inner.check(),
            _ => Ok(()),
        }
    }

    fn check_depth(&self) -> Result<(), InvalidValue> {
        let signature = self.signature();
        if names::is_single_type(&signature) {
            Ok(())
        } else {
            Err(InvalidValue(format!("{signature} nests too deep")))
        }
    }

    /// The value inside any variants.
    pub fn unvariant(&self) -> &Value {
        match self {
            Self::Variant(inner) => inner.unvariant(),
            other => other,
        }
    }

    /// The text of a string, an object path, or a signature.
    pub fn as_str(&self) -> Option<&str> {
        match self.unvariant() {
            Self::Str(text) | Self::ObjectPath(text) | Self::Signature(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self.unvariant() {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// An unsigned integer value of any width.
    pub fn as_u64(&self) -> Option<u64> {
        match *self.unvariant() {
            Self::Byte(value) => Some(u64::from(value)),
            Self::Uint16(value) => Some(u64::from(value)),
            Self::Uint32(value) => Some(u64::from(value)),
            Self::Uint64(value) => Some(value),
            Self::Int16(value) => u64::try_from(value).ok(),
            Self::Int32(value) => u64::try_from(value).ok(),
            Self::Int64(value) => u64::try_from(value).ok(),
            _ => None,
        }
    }

    /// An unsigned integer value that fits in 32 bits.
    pub fn as_u32(&self) -> Option<u32> {
        self.as_u64().and_then(|value| u32::try_from(value).ok())
    }

    /// A signed integer value of any width.
    pub fn as_i64(&self) -> Option<i64> {
        match *self.unvariant() {
            Self::Int16(value) => Some(i64::from(value)),
            Self::Int32(value) => Some(i64::from(value)),
            Self::Int64(value) => Some(value),
            _ => self.as_u64().and_then(|value| i64::try_from(value).ok()),
        }
    }

    /// A number: a double, or an integer as a double.
    pub fn as_f64(&self) -> Option<f64> {
        match *self.unvariant() {
            Self::Double(value) => Some(value),
            _ => self.as_i64().map(|value| value as f64),
        }
    }

    /// The items of an array.
    pub fn as_array(&self) -> Option<&[Value]> {
        match self.unvariant() {
            Self::Array { items, .. } => Some(items),
            _ => None,
        }
    }

    /// The fields of a struct.
    pub fn as_struct(&self) -> Option<&[Value]> {
        match self.unvariant() {
            Self::Struct(fields) => Some(fields),
            _ => None,
        }
    }

    /// The entries of a dictionary.
    pub fn as_dict(&self) -> Option<&[(Value, Value)]> {
        match self.unvariant() {
            Self::Dict { entries, .. } => Some(entries),
            _ => None,
        }
    }

    /// The value of the string key `key` in a dictionary, inside any
    /// variant.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_dict()?
            .iter()
            .find(|(entry, _)| entry.as_str() == Some(key))
            .map(|(_, value)| value.unvariant())
    }

    /// The items of an array of strings.
    pub fn as_strings(&self) -> Option<Vec<String>> {
        self.as_array()?
            .iter()
            .map(|item| item.as_str().map(str::to_owned))
            .collect()
    }
}

macro_rules! value_from {
    ($($ty:ty => $variant:ident),* $(,)?) => {$(
        impl From<$ty> for Value {
            fn from(value: $ty) -> Self {
                Self::$variant(value)
            }
        }
    )*};
}

value_from!(
    u8 => Byte,
    bool => Bool,
    i16 => Int16,
    u16 => Uint16,
    i32 => Int32,
    u32 => Uint32,
    i64 => Int64,
    u64 => Uint64,
    f64 => Double,
    String => Str,
);

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Str(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_describe_nested_values() {
        let value = Value::Struct(vec![
            Value::from(1i32),
            Value::vardict([("a", Value::from(true))]),
            Value::strings(["x"]),
            Value::array("(iiay)", Vec::new()),
        ]);
        assert_eq!(value.signature(), "(ia{sv}asa(iiay))");
        assert!(value.check().is_ok());
    }

    #[test]
    fn a_mistyped_item_fails_the_check() {
        let value = Value::array("s", vec![Value::from(1u32)]);
        assert!(value.check().is_err());
        assert!(Value::object_path("not/a/path").check().is_err());
        assert!(Value::Struct(Vec::new()).check().is_err());
        assert!(Value::str("a\0b").check().is_err());
        let dict = Value::Dict {
            key: "v".to_owned(),
            value: "s".to_owned(),
            entries: Vec::new(),
        };
        assert!(dict.check().is_err());
    }

    #[test]
    fn accessors_look_through_variants() {
        let dict = Value::vardict([
            ("color-scheme", Value::variant(1u32)),
            ("name", Value::from("Cantarell 11")),
        ]);
        assert_eq!(dict.get("color-scheme").and_then(Value::as_u32), Some(1));
        assert_eq!(
            dict.get("name").and_then(Value::as_str),
            Some("Cantarell 11")
        );
        assert_eq!(Value::from(2i32).as_f64(), Some(2.0));
        assert_eq!(Value::from(-1i32).as_u32(), None);
    }
}
