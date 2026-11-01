//! AppKit window for macOS: a programmatically configured `NSApplication`, an `NSWindow` and
//! a layer-backed `NSView` whose backing layer is a `CAMetalLayer`.
//!
//! Sizes are physical pixels everywhere, as on Windows and X11: `new` and `resize` take
//! pixels (the window is `pixels / backingScaleFactor` points), `size` reports the drawable
//! size in pixels and pointer coordinates are pixels from the top-left of the content area.
//! On a Retina display a 1600x900 window is 800x450 points and renders every pixel.
//!
//! Events: `poll` dequeues AppKit events without blocking (`nextEventMatchingMask:`), maps
//! keyboard and mouse events to [`Event`]s, then hands every event to `NSApp` so window
//! dragging, resizing and the traffic-light buttons keep working. Letters and digits are
//! mapped from physical key codes (ANSI positions), so they arrive with any keyboard layout or
//! input method. Window-state changes come from a delegate created at runtime. Cmd+Q and
//! Cmd+W report [`Event::Close`].
//!
//! Threading: AppKit requires the main thread; `new` fails elsewhere. The window and the
//! state its delegate reads live until drop, which detaches the delegate first.
use super::{Event, Key};
use crate::apple::objc::{
    self, CGPoint, CGRect, CGSize, ClassBuilder, Id, NSInteger, NSUInteger, Obj, Sel,
    autoreleasepool, class, send, send_stret,
};
use core::ffi::c_void;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
    static NSDefaultRunLoopMode: Id;
}

/// Shared between the window and its delegate.
struct State {
    events: RefCell<VecDeque<Event>>,
    /// Content size in pixels.
    size: Cell<(u32, u32)>,
    visible: Cell<bool>,
    minimized: Cell<bool>,
}

pub struct Window {
    app: Id,
    window: Obj,
    view: Obj,
    layer: Obj,
    delegate: Obj,
    state: Box<State>,
}

const STATE_IVAR: &core::ffi::CStr = c"abe_state";

/// `NSWindowStyleMaskTitled | Closable | Miniaturizable | Resizable`.
const STYLE: NSUInteger = 1 | 2 | 4 | 8;

fn delegate_state<'a>(this: Id) -> Option<&'a State> {
    // SAFETY: the ivar is null or points at the Box<State> of a live Window (the window
    // detaches its delegate before the box is freed).
    unsafe { (objc::get_ivar(this, STATE_IVAR) as *const State).as_ref() }
}

extern "C" fn should_close(this: Id, _: Sel, _: Id) -> i8 {
    if let Some(s) = delegate_state(this) {
        s.events.borrow_mut().push_back(Event::Close);
    }
    0
}
extern "C" fn did_resize(this: Id, _: Sel, notification: Id) {
    if let Some(s) = delegate_state(this) {
        // SAFETY: the notification's object is the live NSWindow being resized; its content
        // view is the Metal view. Moving to a screen with another scale changes the layer's.
        let window = unsafe {
            let window = send!(notification, "object"; Id);
            let layer = send!(send!(window, "contentView"; Id), "layer"; Id);
            let scale = send!(window, "backingScaleFactor"; f64).max(1.0);
            if !layer.is_null() {
                send!(layer, "setContentsScale:", scale => f64; ());
            }
            window
        };
        let size = content_pixels(window);
        if s.size.replace(size) != size {
            s.events
                .borrow_mut()
                .push_back(Event::Resized(size.0, size.1));
        }
    }
}
extern "C" fn did_miniaturize(this: Id, _: Sel, _: Id) {
    if let Some(s) = delegate_state(this) {
        s.minimized.set(true);
        s.visible.set(false);
        s.events.borrow_mut().push_back(Event::Visibility(false));
    }
}
extern "C" fn did_deminiaturize(this: Id, _: Sel, _: Id) {
    if let Some(s) = delegate_state(this) {
        s.minimized.set(false);
        s.visible.set(true);
        s.events.borrow_mut().push_back(Event::Visibility(true));
    }
}
extern "C" fn did_become_key(this: Id, _: Sel, _: Id) {
    if let Some(s) = delegate_state(this) {
        s.events.borrow_mut().push_back(Event::Focus(true));
    }
}
extern "C" fn did_resign_key(this: Id, _: Sel, _: Id) {
    if let Some(s) = delegate_state(this) {
        s.events.borrow_mut().push_back(Event::Focus(false));
    }
}
extern "C" fn did_change_occlusion(this: Id, _: Sel, notification: Id) {
    if let Some(s) = delegate_state(this) {
        // SAFETY: the notification's object is the live NSWindow; 2 = NSWindowOcclusionStateVisible.
        let shown = unsafe {
            let window = send!(notification, "object"; Id);
            send!(window, "occlusionState"; NSUInteger) & 2 != 0
        } && !s.minimized.get();
        if s.visible.replace(shown) != shown {
            s.events.borrow_mut().push_back(Event::Visibility(shown));
        }
    }
}

extern "C" fn yes(_: Id, _: Sel) -> i8 {
    1
}
/// The view's backing layer: AppKit sizes it with the view.
extern "C" fn make_backing_layer(_: Id, _: Sel) -> Id {
    // SAFETY: +[CAMetalLayer layer] returns an autoreleased layer, as this method must.
    unsafe { send!(class!("CAMetalLayer"), "layer"; Id) }
}
extern "C" fn update_layer(_: Id, _: Sel) {}
extern "C" fn yes_with(_: Id, _: Sel, _: Id) -> i8 {
    1
}
/// Key events reach the view after `poll` has read them; swallowing them here avoids the
/// system beep for unhandled keys.
extern "C" fn ignore_event(_: Id, _: Sel, _: Id) {}

/// The delegate and view classes, registered once per process.
fn classes() -> (Id, Id) {
    static CLASSES: std::sync::OnceLock<(usize, usize)> = std::sync::OnceLock::new();
    let (d, v) = *CLASSES.get_or_init(|| {
        let delegate = match ClassBuilder::new(c"ABEWindowDelegate", class!("NSObject")) {
            Some(mut b) => {
                b.pointer_ivar(STATE_IVAR);
                // SAFETY: each implementation matches its type encoding (v = void, c = BOOL,
                // @ = id, : = SEL).
                unsafe {
                    b.method(
                        objc::sel!("windowShouldClose:"),
                        should_close as *const c_void,
                        c"c@:@",
                    );
                    b.method(
                        objc::sel!("windowDidResize:"),
                        did_resize as *const c_void,
                        c"v@:@",
                    );
                    b.method(
                        objc::sel!("windowDidChangeBackingProperties:"),
                        did_resize as *const c_void,
                        c"v@:@",
                    );
                    b.method(
                        objc::sel!("windowDidMiniaturize:"),
                        did_miniaturize as *const c_void,
                        c"v@:@",
                    );
                    b.method(
                        objc::sel!("windowDidDeminiaturize:"),
                        did_deminiaturize as *const c_void,
                        c"v@:@",
                    );
                    b.method(
                        objc::sel!("windowDidBecomeKey:"),
                        did_become_key as *const c_void,
                        c"v@:@",
                    );
                    b.method(
                        objc::sel!("windowDidResignKey:"),
                        did_resign_key as *const c_void,
                        c"v@:@",
                    );
                    b.method(
                        objc::sel!("windowDidChangeOcclusionState:"),
                        did_change_occlusion as *const c_void,
                        c"v@:@",
                    );
                }
                b.register()
            }
            None => objc::class_named(c"ABEWindowDelegate"),
        };
        let view = match ClassBuilder::new(c"ABEMetalView", class!("NSView")) {
            Some(mut b) => {
                // SAFETY: as above.
                unsafe {
                    b.method(
                        objc::sel!("makeBackingLayer"),
                        make_backing_layer as *const c_void,
                        c"@@:",
                    );
                    b.method(objc::sel!("wantsUpdateLayer"), yes as *const c_void, c"c@:");
                    b.method(
                        objc::sel!("updateLayer"),
                        update_layer as *const c_void,
                        c"v@:",
                    );
                    b.method(
                        objc::sel!("acceptsFirstResponder"),
                        yes as *const c_void,
                        c"c@:",
                    );
                    b.method(
                        objc::sel!("acceptsFirstMouse:"),
                        yes_with as *const c_void,
                        c"c@:@",
                    );
                    b.method(
                        objc::sel!("keyDown:"),
                        ignore_event as *const c_void,
                        c"v@:@",
                    );
                    b.method(objc::sel!("keyUp:"), ignore_event as *const c_void, c"v@:@");
                    b.method(
                        objc::sel!("flagsChanged:"),
                        ignore_event as *const c_void,
                        c"v@:@",
                    );
                }
                b.register()
            }
            None => objc::class_named(c"ABEMetalView"),
        };
        (delegate as usize, view as usize)
    });
    (d as Id, v as Id)
}

/// The content size of `window` in pixels.
fn content_pixels(window: Id) -> (u32, u32) {
    // SAFETY: plain queries on a live NSWindow and its content view.
    unsafe {
        let view = send!(window, "contentView"; Id);
        let bounds: CGRect = send_stret!(view, "bounds"; CGRect);
        let scale = send!(window, "backingScaleFactor"; f64).max(1.0);
        (
            (bounds.size.width * scale).round().max(0.0) as u32,
            (bounds.size.height * scale).round().max(0.0) as u32,
        )
    }
}

fn key_for(code: u16) -> Option<Key> {
    // Physical key codes (kVK_ANSI_*) of US-layout positions.
    const LETTERS: [(u16, u8); 36] = [
        (0x00, b'a'),
        (0x0B, b'b'),
        (0x08, b'c'),
        (0x02, b'd'),
        (0x0E, b'e'),
        (0x03, b'f'),
        (0x05, b'g'),
        (0x04, b'h'),
        (0x22, b'i'),
        (0x26, b'j'),
        (0x28, b'k'),
        (0x25, b'l'),
        (0x2E, b'm'),
        (0x2D, b'n'),
        (0x1F, b'o'),
        (0x23, b'p'),
        (0x0C, b'q'),
        (0x0F, b'r'),
        (0x01, b's'),
        (0x11, b't'),
        (0x20, b'u'),
        (0x09, b'v'),
        (0x0D, b'w'),
        (0x07, b'x'),
        (0x10, b'y'),
        (0x06, b'z'),
        (0x1D, b'0'),
        (0x12, b'1'),
        (0x13, b'2'),
        (0x14, b'3'),
        (0x15, b'4'),
        (0x17, b'5'),
        (0x16, b'6'),
        (0x1A, b'7'),
        (0x1C, b'8'),
        (0x19, b'9'),
    ];
    Some(match code {
        0x35 => Key::Escape,
        0x31 => Key::Space,
        0x7B => Key::Left,
        0x7C => Key::Right,
        0x7D => Key::Down,
        0x7E => Key::Up,
        0x38 | 0x3C => Key::Shift,
        c => {
            return LETTERS
                .iter()
                .find(|(k, _)| *k == c)
                .map(|(_, ch)| Key::Char(*ch));
        }
    })
}

/// `NSEventModifierFlagShift` and `NSEventModifierFlagCommand`.
const SHIFT: NSUInteger = 1 << 17;
const COMMAND: NSUInteger = 1 << 20;

impl Window {
    pub fn new(title: &str, width: u32, height: u32) -> Result<Self, String> {
        if !(1..=32767).contains(&width) || !(1..=32767).contains(&height) {
            return Err("invalid window size".into());
        }
        if !objc::is_main_thread() {
            return Err("AppKit windows must be created on the main thread".into());
        }
        autoreleasepool(|| {
            let (delegate_class, view_class) = classes();
            // SAFETY: (whole block) AppKit calls on the main thread with live objects; owned
            // objects come from alloc/init or are retained.
            unsafe {
                let app = send!(class!("NSApplication"), "sharedApplication"; Id);
                // NSApplicationActivationPolicyRegular: a Dock icon and keyboard focus.
                send!(app, "setActivationPolicy:", 0 as NSInteger => NSInteger; i8);
                static LAUNCHED: std::sync::Once = std::sync::Once::new();
                LAUNCHED.call_once(|| send!(app, "finishLaunching"; ()));
                let screen = send!(class!("NSScreen"), "mainScreen"; Id);
                let scale = if screen.is_null() {
                    1.0
                } else {
                    send!(screen, "backingScaleFactor"; f64).max(1.0)
                };
                let rect = CGRect {
                    origin: CGPoint { x: 0.0, y: 0.0 },
                    size: CGSize {
                        width: f64::from(width) / scale,
                        height: f64::from(height) / scale,
                    },
                };
                let alloc = send!(class!("NSWindow"), "alloc"; Id);
                let window = Obj::owned(send!(
                    alloc,
                    "initWithContentRect:styleMask:backing:defer:",
                    rect => CGRect,
                    STYLE => NSUInteger,
                    2 as NSUInteger => NSUInteger,
                    false => bool;
                    Id
                ))
                .ok_or("NSWindow initWithContentRect failed")?;
                let w = window.id();
                send!(w, "setReleasedWhenClosed:", false => bool; ());
                let t = objc::nsstring(title);
                send!(w, "setTitle:", t.id() => Id; ());
                send!(w, "setAcceptsMouseMovedEvents:", true => bool; ());

                let valloc = send!(view_class, "alloc"; Id);
                let view = Obj::owned(send!(valloc, "initWithFrame:", rect => CGRect; Id))
                    .ok_or("NSView initWithFrame failed")?;
                // A layer-backed view whose backing layer (makeBackingLayer) is a
                // CAMetalLayer; AppKit keeps its geometry in step with the view.
                send!(view.id(), "setWantsLayer:", true => bool; ());
                // NSViewLayerContentsRedrawDuringViewResize
                send!(view.id(), "setLayerContentsRedrawPolicy:", 2 as NSInteger => NSInteger; ());
                send!(w, "setContentView:", view.id() => Id; ());
                let layer = Obj::retained(send!(view.id(), "layer"; Id))
                    .ok_or("the view has no CAMetalLayer")?;
                let real_scale = send!(w, "backingScaleFactor"; f64).max(1.0);
                send!(layer.id(), "setContentsScale:", real_scale => f64; ());

                let state = Box::new(State {
                    events: RefCell::new(VecDeque::new()),
                    size: Cell::new((0, 0)),
                    visible: Cell::new(true),
                    minimized: Cell::new(false),
                });
                let dalloc = send!(delegate_class, "alloc"; Id);
                let delegate =
                    Obj::owned(send!(dalloc, "init"; Id)).ok_or("window delegate init failed")?;
                objc::set_ivar(
                    delegate.id(),
                    STATE_IVAR,
                    (&*state as *const State).cast_mut().cast(),
                );
                send!(w, "setDelegate:", delegate.id() => Id; ());
                send!(w, "center"; ());
                send!(w, "makeKeyAndOrderFront:", core::ptr::null_mut::<c_void>() => Id; ());
                send!(w, "makeFirstResponder:", view.id() => Id; i8);
                // A binary started from a terminal must take focus from it. macOS 14 added
                // cooperative `activate` and deprecated (but kept) the forceful call, which
                // winit, GLFW and SDL still use; send both where both exist.
                send!(app, "activateIgnoringOtherApps:", true => bool; ());
                if send!(app, "respondsToSelector:", objc::sel!("activate") => Sel; i8) != 0 {
                    send!(app, "activate"; ());
                }
                state.size.set(content_pixels(w));
                Ok(Self {
                    app,
                    window,
                    view,
                    layer,
                    delegate,
                    state,
                })
            }
        })
    }

    /// Content size in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.state.size.get()
    }

    pub fn visible(&self) -> bool {
        self.state.visible.get()
    }

    /// Resizes the content area to `w`x`h` pixels.
    pub fn resize(&self, w: u32, h: u32) {
        if !(1..=32767).contains(&w) || !(1..=32767).contains(&h) {
            return;
        }
        autoreleasepool(|| {
            // SAFETY: AppKit calls on the main thread with a live window.
            unsafe {
                let scale = send!(self.window.id(), "backingScaleFactor"; f64).max(1.0);
                let size = CGSize {
                    width: f64::from(w) / scale,
                    height: f64::from(h) / scale,
                };
                send!(self.window.id(), "setContentSize:", size => CGSize; ());
            }
        });
    }

    pub fn set_visible(&self, on: bool) {
        autoreleasepool(|| {
            // SAFETY: as above.
            unsafe {
                let none = core::ptr::null_mut::<c_void>();
                if on {
                    send!(self.window.id(), "makeKeyAndOrderFront:", none => Id; ());
                } else {
                    send!(self.window.id(), "orderOut:", none => Id; ());
                }
            }
        });
        if self.state.visible.replace(on) != on {
            self.state
                .events
                .borrow_mut()
                .push_back(Event::Visibility(on));
        }
    }

    pub fn set_title(&self, title: &str) {
        autoreleasepool(|| {
            let t = objc::nsstring(title);
            // SAFETY: as above; the window copies the string.
            unsafe { send!(self.window.id(), "setTitle:", t.id() => Id; ()) };
        });
    }

    /// The `CAMetalLayer` presented in the window's content view.
    pub(crate) fn metal_layer(&self) -> Id {
        self.layer.id()
    }

    pub fn poll(&self) -> Option<Event> {
        autoreleasepool(|| {
            loop {
                if let Some(e) = self.state.events.borrow_mut().pop_front() {
                    return Some(e);
                }
                // SAFETY: AppKit event handling on the main thread.
                unsafe {
                    let until = send!(class!("NSDate"), "distantPast"; Id);
                    let event = send!(
                        self.app,
                        "nextEventMatchingMask:untilDate:inMode:dequeue:",
                        NSUInteger::MAX => NSUInteger,
                        until => Id,
                        NSDefaultRunLoopMode => Id,
                        true => bool;
                        Id
                    );
                    if event.is_null() {
                        return None;
                    }
                    if let Some(e) = self.translate(event) {
                        self.state.events.borrow_mut().push_back(e);
                    }
                    send!(self.app, "sendEvent:", event => Id; ());
                }
            }
        })
    }

    /// The engine event for an AppKit event of this window, if any.
    unsafe fn translate(&self, event: Id) -> Option<Event> {
        // SAFETY: (whole function) queries on a live NSEvent and this window's view.
        unsafe {
            let ty = send!(event, "type"; NSUInteger);
            let window = send!(event, "window"; Id);
            if !window.is_null() && window != self.window.id() {
                return None;
            }
            match ty {
                // Mouse: left down/up, moved, left dragged.
                1 | 2 | 5 | 6 => {
                    let p: CGPoint = send!(event, "locationInWindow"; CGPoint);
                    let v = send!(
                        self.view.id(),
                        "convertPoint:fromView:",
                        p => CGPoint,
                        core::ptr::null_mut::<c_void>() => Id;
                        CGPoint
                    );
                    let bounds: CGRect = send_stret!(self.view.id(), "bounds"; CGRect);
                    let scale = send!(self.window.id(), "backingScaleFactor"; f64).max(1.0);
                    let x = (v.x * scale).round() as i32;
                    let y = ((bounds.size.height - v.y) * scale).round() as i32;
                    Some(match ty {
                        1 => Event::Clicked(x, y),
                        2 => Event::Unclicked(x, y),
                        _ => Event::Pointer(x, y),
                    })
                }
                // Key down / up.
                10 | 11 => {
                    let flags = send!(event, "modifierFlags"; NSUInteger);
                    let code = send!(event, "keyCode"; u16);
                    if ty == 10 && flags & COMMAND != 0 && matches!(code, 0x0C | 0x0D) {
                        return Some(Event::Close);
                    }
                    let key = key_for(code)?;
                    Some(if ty == 10 {
                        Event::Pressed(key)
                    } else {
                        Event::Released(key)
                    })
                }
                // Modifier changes: Shift.
                12 => {
                    let code = send!(event, "keyCode"; u16);
                    if !matches!(code, 0x38 | 0x3C) {
                        return None;
                    }
                    let flags = send!(event, "modifierFlags"; NSUInteger);
                    Some(if flags & SHIFT != 0 {
                        Event::Pressed(Key::Shift)
                    } else {
                        Event::Released(Key::Shift)
                    })
                }
                _ => None,
            }
        }
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        autoreleasepool(|| {
            // SAFETY: detach the delegate before its state is freed, then close the window.
            unsafe {
                send!(self.window.id(), "setDelegate:", core::ptr::null_mut::<c_void>() => Id; ());
                objc::set_ivar(self.delegate.id(), STATE_IVAR, core::ptr::null_mut());
                send!(self.window.id(), "close"; ());
            }
        });
    }
}
