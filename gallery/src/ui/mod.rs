//! The gallery built with views: the page API, the page views, the window
//! shell, and the application.

#[cfg(target_os = "macos")]
pub mod app;
pub mod page;
pub mod pages;
pub mod shell;
