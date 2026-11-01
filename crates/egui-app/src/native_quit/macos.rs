//! The macOS side of [`super`]: answer `applicationShouldTerminate:` with "ask me later".
//!
//! Winit installs the application delegate and does not implement `applicationShouldTerminate:`,
//! and a delegate cannot be subclassed or swapped without breaking winit. The method is therefore
//! added to the delegate's class at run time (`class_addMethod`). That is the smallest native
//! surface that makes Command+Q, the Dock's Quit and log-out reach the host's close guard.
//!
//! The method returns `NSTerminateLater`: AppKit then waits for
//! `-[NSApplication replyToApplicationShouldTerminate:]`, which [`reply`] sends when the host has
//! the answer. Nothing is cancelled outright, so a log-out is not refused for a clean notebook, and
//! nothing terminates before the host has had its say.
//!
//! If a later winit implements `applicationShouldTerminate:` itself, [`install`] reports that it
//! could not add its own and Command+Q is unguarded again: the log line and the native test in this
//! file are what to look at when bumping winit.

use super::Inbox;
use eframe::egui;
use objc2::ffi::class_addMethod;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{class, msg_send, sel};
use std::sync::OnceLock;

/// `NSApplicationTerminateReply`.
const NS_TERMINATE_NOW: usize = 1;
const NS_TERMINATE_LATER: usize = 2;

struct Hook {
    ctx: egui::Context,
    inbox: Inbox,
}

/// One application, one hook: the delegate method is a plain function and finds its state here.
static HOOK: OnceLock<Hook> = OnceLock::new();

/// `-[delegate applicationShouldTerminate:]`. Runs on the main thread, inside AppKit's handling of
/// the quit command.
extern "C" fn should_terminate(_this: *mut AnyObject, _cmd: Sel, _sender: *mut AnyObject) -> usize {
    match HOOK.get() {
        Some(hook) => {
            hook.inbox.store(true, std::sync::atomic::Ordering::Release);
            // Wake the event loop: a minimised or idle window runs no frame otherwise.
            hook.ctx.request_repaint();
            NS_TERMINATE_LATER
        }
        // Not installed through `install`: behave as an application without the method would.
        None => NS_TERMINATE_NOW,
    }
}

/// Adds `applicationShouldTerminate:` to the application delegate's class. Returns whether Quit is
/// now guarded. Call once, on the main thread, after winit has created its delegate (the
/// application creator closure of `eframe::run_native` is late enough).
pub fn install(ctx: &egui::Context, inbox: Inbox) -> bool {
    let delegate: *mut AnyObject = unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        msg_send![app, delegate]
    };
    if delegate.is_null() {
        eprintln!("omatree-egui: no application delegate; Command+Q cannot be guarded");
        return false;
    }
    let class: &AnyClass = unsafe { (*delegate).class() };
    install_on(class, ctx, inbox)
}

fn install_on(class: &AnyClass, ctx: &egui::Context, inbox: Inbox) -> bool {
    if HOOK
        .set(Hook {
            ctx: ctx.clone(),
            inbox,
        })
        .is_err()
    {
        return false;
    }
    let imp: Imp = unsafe {
        std::mem::transmute::<extern "C" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize, Imp>(
            should_terminate,
        )
    };
    // "Q@:@": returns NSUInteger, takes self, _cmd and the sender.
    let added = unsafe {
        class_addMethod(
            class as *const AnyClass as *mut AnyClass as *mut _,
            sel!(applicationShouldTerminate:),
            imp,
            c"Q@:@".as_ptr(),
        )
    };
    if !added.as_bool() {
        eprintln!(
            "omatree-egui: the application delegate already answers applicationShouldTerminate:; \
             Command+Q is not guarded"
        );
    }
    added.as_bool()
}

/// Tells AppKit the answer to the request that was left waiting: `true` lets the application
/// terminate, `false` keeps it running. Main thread only.
pub fn reply(allow: bool) {
    unsafe {
        let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        let _: () = msg_send![app, replyToApplicationShouldTerminate: Bool::new(allow)];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::declare::ClassBuilder;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// The native mechanism itself, on a stand-in for winit's delegate: the method is added, an
    /// `applicationShouldTerminate:` message returns "later", and the inbox is raised. (This is the
    /// only test that touches the process-wide hook.)
    #[test]
    fn the_added_method_answers_later_and_raises_the_inbox() {
        let builder = ClassBuilder::new(c"OmaTreeFakeWinitDelegate", class!(NSObject))
            .expect("declare a stand-in delegate class");
        let class = builder.register();
        assert!(
            !class.responds_to(sel!(applicationShouldTerminate:)),
            "the stand-in must start without the method, as winit's delegate does"
        );
        let inbox: Inbox = Arc::new(AtomicBool::new(false));
        let ctx = egui::Context::default();
        assert!(install_on(class, &ctx, inbox.clone()));
        assert!(class.responds_to(sel!(applicationShouldTerminate:)));

        let obj: *mut AnyObject = unsafe { msg_send![class, new] };
        let reply: usize = unsafe {
            msg_send![obj, applicationShouldTerminate: std::ptr::null_mut::<AnyObject>()]
        };
        assert_eq!(reply, NS_TERMINATE_LATER);
        assert!(
            inbox.load(Ordering::Acquire),
            "the request reaches the host"
        );

        // A second class cannot take the hook over: the hook is process-wide and set once.
        let other = ClassBuilder::new(c"OmaTreeFakeWinitDelegateTwo", class!(NSObject)).unwrap();
        let other = other.register();
        assert!(!install_on(other, &ctx, inbox));
    }
}
