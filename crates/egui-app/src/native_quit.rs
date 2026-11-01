//! Quit requests that come from the operating system rather than from the window (M1).
//!
//! On macOS, Command+Q, the Dock's "Quit" and a log-out or restart do not close the window. They
//! call `NSApplication terminate:`, and winit (which owns the application delegate and its menu)
//! implements only `applicationWillTerminate:`: the process simply ends, the window never gets a
//! close request, and so `Host::request_close`, the one guard that asks Save / Discard / Cancel,
//! is never consulted. (Winit 0.30 source: `platform_impl/macos/menu.rs` wires Quit to
//! `terminate:`, `app_state.rs` has no `applicationShouldTerminate:`.)
//!
//! The fix keeps one guard. A platform hook ([`macos`]) answers AppKit's "should I terminate?"
//! with "ask me later", and raises a flag in the [`Inbox`]. [`Host::logic`] takes the flag and runs
//! exactly the close path a click on the window's red button takes (`request_close`), then tells
//! AppKit the final answer through the [`Answer`] callback: yes once the close was allowed (a clean
//! notebook, a successful Save, a confirmed Discard), no when the question was cancelled or could
//! not be asked. Nothing here knows what "dirty" means, and nothing decides to quit on its own.
//!
//! The types are platform-independent so the whole decision is tested on any system; only the
//! hook that raises the flag exists on macOS alone.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// Raised by the platform hook, taken by the host once per frame.
pub type Inbox = Arc<AtomicBool>;

/// Tells the operating system whether the application may now terminate (`true`) or must go on
/// (`false`). Called at most once for each request that was waiting.
pub type Answer = Box<dyn Fn(bool)>;

#[cfg(target_os = "macos")]
pub mod macos;
