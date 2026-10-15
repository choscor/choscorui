//! Match rules: which signals a subscription receives.

use super::{Error, Message, MessageKind, names};

/// A rule for the signals that a subscription receives. The bus applies
/// all the fields. The connection then routes each signal to every
/// subscription whose rule matches it. That local match compares the
/// sender only when the rule names a unique name, because a signal
/// carries the unique name of its sender.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MatchRule {
    pub sender: Option<String>,
    pub path: Option<String>,
    /// The path and every path below it.
    pub path_namespace: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    /// The first argument, when it is a string.
    pub arg0: Option<String>,
}

/// `value` quoted for a match rule: an apostrophe becomes `'\''`.
fn quoted(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

impl MatchRule {
    /// A rule for every signal.
    pub fn signal() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn sender(mut self, sender: impl Into<String>) -> Self {
        self.sender = Some(sender.into());
        self
    }

    #[must_use]
    pub fn path(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }

    #[must_use]
    pub fn path_namespace(mut self, path: impl Into<String>) -> Self {
        self.path_namespace = Some(path.into());
        self
    }

    #[must_use]
    pub fn interface(mut self, interface: impl Into<String>) -> Self {
        self.interface = Some(interface.into());
        self
    }

    #[must_use]
    pub fn member(mut self, member: impl Into<String>) -> Self {
        self.member = Some(member.into());
        self
    }

    #[must_use]
    pub fn arg0(mut self, value: impl Into<String>) -> Self {
        self.arg0 = Some(value.into());
        self
    }

    /// The rule as the bus reads it, such as
    /// `type='signal',interface='org.example.Item'`.
    pub fn to_rule(&self) -> Result<String, Error> {
        let invalid = |what: &str, value: &str| {
            Err(Error::InvalidArgument(format!(
                "{value:?} is not a valid {what}"
            )))
        };
        let mut parts = vec!["type='signal'".to_owned()];
        if let Some(sender) = &self.sender {
            if !names::is_bus_name(sender) {
                return invalid("bus name", sender);
            }
            parts.push(format!("sender={}", quoted(sender)));
        }
        for (key, path) in [
            ("path", &self.path),
            ("path_namespace", &self.path_namespace),
        ] {
            if let Some(path) = path {
                if !names::is_object_path(path) {
                    return invalid("object path", path);
                }
                parts.push(format!("{key}={}", quoted(path)));
            }
        }
        if let Some(interface) = &self.interface {
            if !names::is_interface_name(interface) {
                return invalid("interface", interface);
            }
            parts.push(format!("interface={}", quoted(interface)));
        }
        if let Some(member) = &self.member {
            if !names::is_member_name(member) {
                return invalid("member", member);
            }
            parts.push(format!("member={}", quoted(member)));
        }
        if let Some(arg0) = &self.arg0 {
            if arg0.contains('\0') {
                return invalid("argument", arg0);
            }
            parts.push(format!("arg0={}", quoted(arg0)));
        }
        Ok(parts.join(","))
    }

    /// True when `message` is a signal that the rule describes.
    pub fn matches(&self, message: &Message) -> bool {
        if message.kind() != Some(MessageKind::Signal) {
            return false;
        }
        let same = |rule: &Option<String>, actual: Option<String>| {
            rule.as_ref()
                .is_none_or(|rule| actual.as_deref() == Some(rule))
        };
        if let Some(sender) = &self.sender
            && sender.starts_with(':')
            && message.sender().as_deref() != Some(sender)
        {
            return false;
        }
        if let Some(namespace) = &self.path_namespace {
            let path = message.path().unwrap_or_default();
            let inside = namespace == "/"
                || path == *namespace
                || path.starts_with(&format!("{namespace}/"));
            if !inside {
                return false;
            }
        }
        if !same(&self.path, message.path())
            || !same(&self.interface, message.interface())
            || !same(&self.member, message.member())
        {
            return false;
        }
        match &self.arg0 {
            None => true,
            Some(arg0) => message.args().is_ok_and(|args| {
                args.first().and_then(|arg| match arg {
                    super::Value::Str(text) => Some(text.as_str()),
                    _ => None,
                }) == Some(arg0.as_str())
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_quote_their_values() {
        let rule = MatchRule::signal()
            .sender("org.freedesktop.portal.Desktop")
            .path("/org/freedesktop/portal/desktop/request/1_2/t")
            .interface("org.freedesktop.portal.Request")
            .member("Response")
            .arg0("it's");
        assert_eq!(
            rule.to_rule().unwrap(),
            "type='signal',sender='org.freedesktop.portal.Desktop',\
             path='/org/freedesktop/portal/desktop/request/1_2/t',\
             interface='org.freedesktop.portal.Request',member='Response',arg0='it'\\''s'"
        );
        assert!(MatchRule::signal().path("bad").to_rule().is_err());
    }
}
