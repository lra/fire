// minifb has no fullscreen API, and AppKit's native `toggleFullScreen:`
// can't exit cleanly through minifb's NSWindow geometry overrides — so
// classic borderless fullscreen instead: swap the styleMask and cover the
// screen, restore the saved frame on the way back. minifb's NSWindow
// overrides canBecomeKeyWindow to YES, so a borderless window keeps
// keyboard input.
// ponytail: macOS + main display only; other OSes need their own backend FFI.

#![allow(clashing_extern_declarations)] // objc_msgSend is declared once per call signature

use minifb::Window;
use std::ffi::{CStr, c_char, c_void};

#[repr(C)]
#[derive(Clone, Copy)]
struct CGRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}
#[repr(C)]
struct CGSize {
    w: f64,
    h: f64,
}

unsafe extern "C" {
    fn sel_registerName(name: *const c_char) -> *const c_void;
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn CGMainDisplayID() -> u32;
    fn CGDisplayBounds(display: u32) -> CGRect;
    // objc_msgSend is one trampoline; declare it per call signature.
    // Struct *returns* would need objc_msgSend_stret on x86_64 — none used.
    #[link_name = "objc_msgSend"]
    fn msg_send_usize(obj: *mut c_void, sel: *const c_void, v: usize);
    #[link_name = "objc_msgSend"]
    fn msg_send_rect_bool(obj: *mut c_void, sel: *const c_void, r: CGRect, display: bool);
    #[link_name = "objc_msgSend"]
    fn msg_send_cgsize(obj: *mut c_void, sel: *const c_void, s: CGSize);
    #[link_name = "objc_msgSend"]
    fn msg_send_ret_id(obj: *mut c_void, sel: *const c_void) -> *mut c_void;
    #[link_name = "objc_msgSend"]
    fn msg_send_id(obj: *mut c_void, sel: *const c_void, arg: *mut c_void);
    #[link_name = "objc_msgSend"]
    fn msg_send_rect(obj: *mut c_void, sel: *const c_void, r: CGRect);
}

unsafe fn sel(name: &CStr) -> *const c_void {
    unsafe { sel_registerName(name.as_ptr()) }
}

unsafe fn nsapp() -> *mut c_void {
    unsafe {
        msg_send_ret_id(
            objc_getClass(c"NSApplication".as_ptr()),
            sel(c"sharedApplication"),
        )
    }
}

// NSWindowStyleMask: Titled | Closable | Resizable — what minifb
// creates the window with (resize: true).
const WINDOWED_MASK: usize = 1 | 2 | 8;
// NSApplicationPresentationOptions: HideDock | HideMenuBar.
const HIDE_DOCK_AND_MENU: usize = 2 | 8;

/// minifb lays out its content view and Metal view with a baked-in
/// title-bar offset and only autoresizes them, so the offset survives
/// styleMask swaps as a growing hole — pin both views explicitly.
unsafe fn pin_views(win: *mut c_void, w: f64, h: f64) {
    unsafe {
        let content = msg_send_ret_id(win, sel(c"contentView"));
        let mtk = msg_send_ret_id(
            msg_send_ret_id(content, sel(c"subviews")),
            sel(c"firstObject"),
        );
        let r = CGRect { x: 0.0, y: 0.0, w, h };
        msg_send_rect(content, sel(c"setFrame:"), r);
        msg_send_rect(mtk, sel(c"setFrame:"), r);
    }
}

/// Windowed position/size to restore on exit.
pub struct Saved {
    pos: (isize, isize),
    size: (usize, usize),
}

pub fn enter(w: &Window) -> Saved {
    let saved = Saved {
        pos: w.get_position(),
        size: w.get_size(),
    };
    unsafe {
        let win = w.get_window_handle();
        msg_send_usize(nsapp(), sel(c"setPresentationOptions:"), HIDE_DOCK_AND_MENU);
        msg_send_usize(win, sel(c"setStyleMask:"), 0); // borderless
        // CG is top-left-origin, AppKit bottom-left; identical for the
        // full main-display rect.
        let bounds = CGDisplayBounds(CGMainDisplayID());
        msg_send_rect_bool(win, sel(c"setFrame:display:"), bounds, true);
        msg_send_id(win, sel(c"makeKeyAndOrderFront:"), std::ptr::null_mut());
        pin_views(win, bounds.w, bounds.h);
    }
    saved
}

pub fn exit(w: &mut Window, saved: &Saved) {
    unsafe {
        let win = w.get_window_handle();
        msg_send_usize(nsapp(), sel(c"setPresentationOptions:"), 0);
        msg_send_usize(win, sel(c"setStyleMask:"), WINDOWED_MASK);
        msg_send_cgsize(
            win,
            sel(c"setContentSize:"),
            CGSize {
                w: saved.size.0 as f64,
                h: saved.size.1 as f64,
            },
        );
        msg_send_id(win, sel(c"makeKeyAndOrderFront:"), std::ptr::null_mut());
        pin_views(win, saved.size.0 as f64, saved.size.1 as f64);
    }
    // minifb's set_position/get_position disagree by the title-bar
    // height — set once, measure the error, compensate.
    w.set_position(saved.pos.0, saved.pos.1);
    let (_, got_y) = w.get_position();
    w.set_position(saved.pos.0, saved.pos.1 + (saved.pos.1 - got_y));
}
