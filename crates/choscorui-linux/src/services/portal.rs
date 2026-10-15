//! The xdg-desktop-portal requests: a call that returns a `Request`
//! object, and the `Response` signal of that object.

use crate::dbus::{BUS_NAME, Connection, Error, MatchRule, Message, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// The bus name of the portal.
pub const DESKTOP: &str = "org.freedesktop.portal.Desktop";
/// The object of every portal interface.
pub const PATH: &str = "/org/freedesktop/portal/desktop";
const REQUEST: &str = "org.freedesktop.portal.Request";

/// A new `handle_token`: unique in the process.
pub fn token() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let number = NEXT.fetch_add(1, Ordering::Relaxed);
    format!("choscorui{}_{number}", std::process::id())
}

/// The path of the request object that the portal makes for `token`.
fn request_path(connection: &Connection, token: &str) -> Option<String> {
    let sender = connection.unique_name()?;
    let sender = sender.trim_start_matches(':').replace('.', "_");
    Some(format!("{PATH}/request/{sender}/{token}"))
}

/// The answer of a request: the response code and the results.
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    /// 0 for success, 1 when the user cancelled, 2 for another end.
    pub code: u32,
    pub results: Value,
}

/// Why a request had no response.
#[derive(Clone, Debug, PartialEq)]
pub enum RequestError {
    /// No portal, or the portal has no backend for the interface.
    NoPortal,
    Failed(String),
}

impl From<Error> for RequestError {
    fn from(error: Error) -> Self {
        // These errors mean that nothing implements the call.
        let missing = [
            "org.freedesktop.DBus.Error.ServiceUnknown",
            "org.freedesktop.DBus.Error.UnknownMethod",
            "org.freedesktop.DBus.Error.UnknownInterface",
            "org.freedesktop.DBus.Error.UnknownObject",
            "org.freedesktop.DBus.Error.NameHasNoOwner",
        ];
        match error.name() {
            Some(name) if missing.contains(&name) => Self::NoPortal,
            _ => Self::Failed(error.to_string()),
        }
    }
}

fn response_rule(path: &str) -> MatchRule {
    MatchRule::signal()
        .sender(DESKTOP)
        .path(path)
        .interface(REQUEST)
        .member("Response")
}

/// The `Response` of `signal`.
fn parse_response(signal: &Message) -> Result<Response, RequestError> {
    let args = signal.args()?;
    let code = args.first().and_then(Value::as_u32);
    match (code, args.get(1)) {
        (Some(code), Some(results)) => Ok(Response {
            code,
            results: results.clone(),
        }),
        _ => Err(RequestError::Failed("the response has no code".to_owned())),
    }
}

/// Calls `method` of the portal `interface` with `args`, then waits in
/// a nested loop for the `Response` of the request. `args` ends with the
/// options, to which this adds the `handle_token`. The wait ends early
/// when the portal leaves the bus.
pub fn request(
    connection: &Connection,
    interface: &str,
    method: &str,
    mut args: Vec<Value>,
) -> Result<Response, RequestError> {
    let token = token();
    if let Some(Value::Dict { entries, .. }) = args.last_mut() {
        entries.push((Value::from("handle_token"), Value::variant(token.clone())));
    }
    let expected = request_path(connection, &token).ok_or(RequestError::NoPortal)?;
    // Subscribe first, so a fast response is not lost.
    let mut response = connection.subscribe(response_rule(&expected))?;
    let owner = connection.subscribe(
        MatchRule::signal()
            .sender(BUS_NAME)
            .interface(BUS_NAME)
            .member("NameOwnerChanged")
            .arg0(DESKTOP),
    )?;
    let message =
        Message::method_call(Some(DESKTOP), PATH, Some(interface), method)?.with(&args)?;
    let reply = connection.call(message, Duration::from_secs(30))?.args()?;
    let handle = reply
        .first()
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if handle != expected && !handle.is_empty() {
        // An old portal ignores the token; follow the path it returned.
        response = connection.subscribe(response_rule(&handle))?;
    }
    let signal = connection.run_until(None, |_| {
        if let Some(signal) = response.next() {
            return Some(Ok(signal));
        }
        let gone = owner.take().iter().any(|changed| {
            changed
                .args()
                .ok()
                .and_then(|args| args.get(2).and_then(Value::as_str).map(str::is_empty))
                .unwrap_or(false)
        });
        gone.then(|| Err(RequestError::Failed("the portal left the bus".to_owned())))
    })??;
    parse_response(&signal)
}
