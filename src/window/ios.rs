//! UIKit app shell for iOS: `UIApplicationMain` with an app delegate, a window-scene delegate,
//! a view controller and a `UIView` subclass whose `+layerClass` is `CAMetalLayer`, all
//! created at runtime. A `CADisplayLink` drives the frame loop on the main thread.
//!
//! The app hands [`run`] a setup function. When the scene connects, `run` creates the window
//! and calls it with a [`Window`] (alive for the rest of the process); the returned closure
//! runs once per display refresh while the app is in the foreground. Rendering stops in the
//! background, where iOS refuses GPU work.
//!
//! Input: touches arrive as [`Input::Touch`] in pixels from the top-left of the view, taps on
//! buttons made with [`Window::add_button`] as [`Input::Button`], and size and visibility
//! changes as [`Input::Event`]. Sizes are pixels (points times the screen scale).
//!
//! The scene manifest in the app's `Info.plist` must name `GanbateSceneDelegate` (see
//! `scripts/apple/build-ios.sh`); the app delegate also returns that configuration.
use super::Event;
use crate::apple::objc::{
    self, CGPoint, CGRect, CGSize, ClassBuilder, Id, NSInteger, NSUInteger, Obj, Sel,
    autoreleasepool, class, send, send_stret,
};
use core::ffi::{c_char, c_int, c_void};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::PathBuf;

#[link(name = "UIKit", kind = "framework")]
unsafe extern "C" {
    fn UIApplicationMain(argc: c_int, argv: *mut *mut c_char, principal: Id, delegate: Id)
    -> c_int;
}
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
    static NSRunLoopCommonModes: Id;
}
unsafe extern "C" {
    fn _NSGetArgc() -> *mut c_int;
    fn _NSGetArgv() -> *mut *mut *mut c_char;
}

/// One phase of a finger on the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPhase {
    Began,
    Moved,
    /// Lifted or cancelled.
    Ended,
}

/// Input for an iOS app, in arrival order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Input {
    Event(Event),
    /// A finger at `(x, y)` pixels; `id` stays the same while the finger is down.
    Touch {
        id: usize,
        x: f32,
        y: f32,
        phase: TouchPhase,
    },
    /// The button with this index (from [`Window::add_button`]) was tapped.
    Button(usize),
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct UIEdgeInsets {
    top: f64,
    left: f64,
    bottom: f64,
    right: f64,
}

struct State {
    inputs: RefCell<VecDeque<Input>>,
    size: Cell<(u32, u32)>,
    visible: Cell<bool>,
    buttons: RefCell<Vec<Obj>>,
}

/// The app's Metal view. Lives for the rest of the process once the scene connected.
pub struct Window {
    view: Obj,
    layer: Id,
    button_target: Obj,
    state: Box<State>,
}

const STATE_IVAR: &core::ffi::CStr = c"ganbate_state";
const WINDOW_IVAR: &core::ffi::CStr = c"ganbate_window";

type Setup = fn(&'static Window) -> Box<dyn FnMut()>;

thread_local! {
    static SETUP: Cell<Option<Setup>> = const { Cell::new(None) };
    static FRAME: RefCell<Option<Box<dyn FnMut()>>> = const { RefCell::new(None) };
    static WINDOW: Cell<Option<&'static Window>> = const { Cell::new(None) };
}

fn state_of<'a>(obj: Id) -> Option<&'a State> {
    // SAFETY: the ivar is null or points at the State of the leaked, never-freed Window.
    unsafe { (objc::get_ivar(obj, STATE_IVAR) as *const State).as_ref() }
}

fn push(input: Input) {
    if let Some(w) = WINDOW.with(Cell::get) {
        w.state.inputs.borrow_mut().push_back(input);
    }
}

fn set_visible(on: bool) {
    if let Some(w) = WINDOW.with(Cell::get)
        && w.state.visible.replace(on) != on
    {
        w.state
            .inputs
            .borrow_mut()
            .push_back(Input::Event(Event::Visibility(on)));
    }
}

// ---------------------------------------------------------------- app and scene delegates

extern "C" fn did_finish_launching(_: Id, _: Sel, _: Id, _: Id) -> i8 {
    1
}

extern "C" fn configuration_for_scene(_: Id, _: Sel, _: Id, session: Id, _: Id) -> Id {
    // SAFETY: building an autoreleased UISceneConfiguration for the connecting session.
    unsafe {
        let name = objc::nsstring("Default");
        let role = send!(session, "role"; Id);
        let alloc = send!(class!("UISceneConfiguration"), "alloc"; Id);
        let config = send!(
            alloc,
            "initWithName:sessionRole:",
            name.id() => Id,
            role => Id;
            Id
        );
        send!(config, "setDelegateClass:", class!("GanbateSceneDelegate") => Id; ());
        send!(config, "autorelease"; Id)
    }
}

extern "C" fn scene_will_connect(this: Id, _: Sel, scene: Id, _: Id, _: Id) {
    autoreleasepool(|| {
        if let Err(e) = connect(this, scene) {
            eprintln!("ganbate: cannot start: {e}");
        }
    });
}

fn connect(delegate: Id, scene: Id) -> Result<(), String> {
    let (_, _, controller_class, _, target_class) = classes();
    // SAFETY: (whole function) UIKit calls on the main thread during scene connection; the
    // window, controller and view are retained by UIKit's hierarchy and by the leaked Window.
    unsafe {
        let walloc = send!(class!("UIWindow"), "alloc"; Id);
        let window = send!(walloc, "initWithWindowScene:", scene => Id; Id);
        if window.is_null() {
            return Err("UIWindow initWithWindowScene failed".into());
        }
        objc::set_ivar(delegate, WINDOW_IVAR, window);
        let calloc = send!(controller_class, "alloc"; Id);
        let controller = Obj::owned(send!(calloc, "init"; Id)).ok_or("view controller init")?;
        send!(window, "setRootViewController:", controller.id() => Id; ());
        // Loads the view (loadView below).
        let view = send!(controller.id(), "view"; Id);
        let layer = send!(view, "layer"; Id);
        // Render at most twice the point size: phones with 3x screens stay smooth.
        let native = send!(send!(class!("UIScreen"), "mainScreen"; Id), "nativeScale"; f64);
        let scale = native.clamp(1.0, 2.0);
        send!(view, "setContentScaleFactor:", scale => f64; ());
        send!(layer, "setContentsScale:", scale => f64; ());
        send!(view, "setMultipleTouchEnabled:", true => bool; ());
        let talloc = send!(target_class, "alloc"; Id);
        let button_target = Obj::owned(send!(talloc, "init"; Id)).ok_or("button target init")?;
        let state = Box::new(State {
            inputs: RefCell::new(VecDeque::new()),
            size: Cell::new((0, 0)),
            visible: Cell::new(true),
            buttons: RefCell::new(Vec::new()),
        });
        let state_ptr = (&*state as *const State).cast_mut().cast();
        objc::set_ivar(view, STATE_IVAR, state_ptr);
        objc::set_ivar(button_target.id(), STATE_IVAR, state_ptr);
        let w: &'static Window = Box::leak(Box::new(Window {
            view: Obj::retained(view).ok_or("nil view")?,
            layer,
            button_target,
            state,
        }));
        w.update_size();
        WINDOW.with(|c| c.set(Some(w)));
        send!(window, "makeKeyAndVisible"; ());
        if let Some(setup) = SETUP.with(Cell::get) {
            let frame = setup(w);
            FRAME.with(|f| *f.borrow_mut() = Some(frame));
        }
        let link = send!(
            class!("CADisplayLink"),
            "displayLinkWithTarget:selector:",
            delegate => Id,
            objc::sel!("frame:") => Sel;
            Id
        );
        let main_loop = send!(class!("NSRunLoop"), "mainRunLoop"; Id);
        send!(link, "addToRunLoop:forMode:", main_loop => Id, NSRunLoopCommonModes => Id; ());
        drop(controller);
        Ok(())
    }
}

extern "C" fn delegate_window(this: Id, _: Sel) -> Id {
    // SAFETY: reading this delegate's ivar.
    unsafe { objc::get_ivar(this, WINDOW_IVAR) }
}
extern "C" fn delegate_set_window(this: Id, _: Sel, window: Id) {
    // SAFETY: writing this delegate's ivar (UIKit keeps the window alive through the scene).
    unsafe { objc::set_ivar(this, WINDOW_IVAR, window) }
}
extern "C" fn scene_foreground(_: Id, _: Sel, _: Id) {
    set_visible(true);
}
extern "C" fn scene_background(_: Id, _: Sel, _: Id) {
    set_visible(false);
}

/// One display refresh: run the app's frame while it is visible.
extern "C" fn frame(_: Id, _: Sel, _: Id) {
    let visible = WINDOW
        .with(Cell::get)
        .is_some_and(|w| w.state.visible.get());
    if !visible {
        return;
    }
    autoreleasepool(|| {
        FRAME.with(|f| {
            if let Some(frame) = f.borrow_mut().as_mut() {
                frame();
            }
        })
    });
}

// ---------------------------------------------------------------- view controller and view

extern "C" fn load_view(this: Id, _: Sel) {
    let (_, view_class, _, _, _) = classes();
    // SAFETY: creating the controller's view on the main thread.
    unsafe {
        let screen = send!(class!("UIScreen"), "mainScreen"; Id);
        let bounds: CGRect = send_stret!(screen, "bounds"; CGRect);
        let alloc = send!(view_class, "alloc"; Id);
        let view = send!(alloc, "initWithFrame:", bounds => CGRect; Id);
        send!(this, "setView:", view => Id; ());
        send!(view, "release"; ());
    }
}
extern "C" fn yes(_: Id, _: Sel) -> i8 {
    1
}
extern "C" fn did_layout(this: Id, _: Sel) {
    if let Some(w) = WINDOW.with(Cell::get) {
        // SAFETY: the controller's view is the window's view.
        let _ = this;
        w.update_size();
        w.layout_buttons();
    }
}
extern "C" fn layer_class(_: Id, _: Sel) -> Id {
    class!("CAMetalLayer")
}

fn touches(view: Id, set: Id, phase: TouchPhase) {
    let Some(state) = state_of(view) else {
        return;
    };
    // SAFETY: iterating the NSSet of UITouches UIKit passes to the view.
    unsafe {
        let scale = send!(view, "contentScaleFactor"; f64).max(1.0);
        let all = send!(set, "allObjects"; Id);
        let n = send!(all, "count"; NSUInteger);
        for i in 0..n {
            let t = send!(all, "objectAtIndex:", i => NSUInteger; Id);
            let p: CGPoint = send!(t, "locationInView:", view => Id; CGPoint);
            state.inputs.borrow_mut().push_back(Input::Touch {
                id: t as usize,
                x: (p.x * scale) as f32,
                y: (p.y * scale) as f32,
                phase,
            });
        }
    }
}
extern "C" fn touches_began(this: Id, _: Sel, set: Id, _: Id) {
    touches(this, set, TouchPhase::Began);
}
extern "C" fn touches_moved(this: Id, _: Sel, set: Id, _: Id) {
    touches(this, set, TouchPhase::Moved);
}
extern "C" fn touches_ended(this: Id, _: Sel, set: Id, _: Id) {
    touches(this, set, TouchPhase::Ended);
}
extern "C" fn button_tapped(_: Id, _: Sel, sender: Id) {
    // SAFETY: the sender is the tapped UIButton; its tag is the button index.
    let tag = unsafe { send!(sender, "tag"; NSInteger) };
    push(Input::Button(tag.max(0) as usize));
}

/// (app delegate, view, view controller, scene delegate, button target) classes.
fn classes() -> (Id, Id, Id, Id, Id) {
    static CLASSES: std::sync::OnceLock<[usize; 5]> = std::sync::OnceLock::new();
    let c = CLASSES.get_or_init(|| {
        let build = |name: &core::ffi::CStr, superclass: Id, f: &dyn Fn(&mut ClassBuilder)| {
            match ClassBuilder::new(name, superclass) {
                Some(mut b) => {
                    f(&mut b);
                    b.register() as usize
                }
                None => objc::class_named(name) as usize,
            }
        };
        // SAFETY (all method registrations): each implementation matches its encoding.
        let app = build(c"GanbateAppDelegate", class!("UIResponder"), &|b| unsafe {
            b.protocol(c"UIApplicationDelegate");
            b.method(
                objc::sel!("application:didFinishLaunchingWithOptions:"),
                did_finish_launching as *const c_void,
                c"c@:@@",
            );
            b.method(
                objc::sel!("application:configurationForConnectingSceneSession:options:"),
                configuration_for_scene as *const c_void,
                c"@@:@@@",
            );
        });
        let view = build(c"GanbateMetalView", class!("UIView"), &|b| unsafe {
            b.pointer_ivar(STATE_IVAR);
            b.class_method(
                objc::sel!("layerClass"),
                layer_class as *const c_void,
                c"#@:",
            );
            b.method(
                objc::sel!("touchesBegan:withEvent:"),
                touches_began as *const c_void,
                c"v@:@@",
            );
            b.method(
                objc::sel!("touchesMoved:withEvent:"),
                touches_moved as *const c_void,
                c"v@:@@",
            );
            b.method(
                objc::sel!("touchesEnded:withEvent:"),
                touches_ended as *const c_void,
                c"v@:@@",
            );
            b.method(
                objc::sel!("touchesCancelled:withEvent:"),
                touches_ended as *const c_void,
                c"v@:@@",
            );
        });
        let controller = build(
            c"GanbateViewController",
            class!("UIViewController"),
            &|b| unsafe {
                b.method(objc::sel!("loadView"), load_view as *const c_void, c"v@:");
                b.method(
                    objc::sel!("prefersStatusBarHidden"),
                    yes as *const c_void,
                    c"c@:",
                );
                b.method(
                    objc::sel!("prefersHomeIndicatorAutoHidden"),
                    yes as *const c_void,
                    c"c@:",
                );
                b.method(
                    objc::sel!("viewDidLayoutSubviews"),
                    did_layout as *const c_void,
                    c"v@:",
                );
            },
        );
        let scene = build(
            c"GanbateSceneDelegate",
            class!("UIResponder"),
            &|b| unsafe {
                b.protocol(c"UIWindowSceneDelegate");
                b.pointer_ivar(WINDOW_IVAR);
                b.method(
                    objc::sel!("scene:willConnectToSession:options:"),
                    scene_will_connect as *const c_void,
                    c"v@:@@@",
                );
                b.method(
                    objc::sel!("window"),
                    delegate_window as *const c_void,
                    c"@@:",
                );
                b.method(
                    objc::sel!("setWindow:"),
                    delegate_set_window as *const c_void,
                    c"v@:@",
                );
                b.method(
                    objc::sel!("sceneWillEnterForeground:"),
                    scene_foreground as *const c_void,
                    c"v@:@",
                );
                b.method(
                    objc::sel!("sceneDidBecomeActive:"),
                    scene_foreground as *const c_void,
                    c"v@:@",
                );
                b.method(
                    objc::sel!("sceneDidEnterBackground:"),
                    scene_background as *const c_void,
                    c"v@:@",
                );
                b.method(objc::sel!("frame:"), frame as *const c_void, c"v@:@");
            },
        );
        let target = build(c"GanbateButtonTarget", class!("NSObject"), &|b| unsafe {
            b.pointer_ivar(STATE_IVAR);
            b.method(objc::sel!("tap:"), button_tapped as *const c_void, c"v@:@");
        });
        [app, view, controller, scene, target]
    });
    (c[0] as Id, c[1] as Id, c[2] as Id, c[3] as Id, c[4] as Id)
}

/// Runs the app: registers the UIKit classes, calls `UIApplicationMain` (which never returns)
/// and, once the scene connects, `setup` with the window. Call it from `main`.
pub fn run(setup: fn(&'static Window) -> Box<dyn FnMut()>) -> ! {
    SETUP.with(|s| s.set(Some(setup)));
    let (app, ..) = classes();
    // SAFETY: the main thread's argc/argv; the delegate class name is an NSString that UIKit
    // copies. UIApplicationMain runs the main run loop forever.
    unsafe {
        let _ = app;
        let delegate = objc::nsstring("GanbateAppDelegate");
        UIApplicationMain(
            *_NSGetArgc(),
            *_NSGetArgv(),
            core::ptr::null_mut(),
            delegate.id(),
        );
    }
    std::process::exit(0)
}

/// Height and width of on-screen buttons, in points.
const BUTTON: (f64, f64) = (44.0, 104.0);

impl Window {
    /// The view size in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.state.size.get()
    }
    /// False while the app is in the background.
    pub fn visible(&self) -> bool {
        self.state.visible.get()
    }
    /// Size and visibility changes (touches and buttons are dropped; use [`Window::poll_input`]).
    pub fn poll(&self) -> Option<Event> {
        loop {
            match self.poll_input()? {
                Input::Event(e) => return Some(e),
                _ => continue,
            }
        }
    }
    /// The next input in arrival order.
    pub fn poll_input(&self) -> Option<Input> {
        self.state.inputs.borrow_mut().pop_front()
    }
    /// Kept for API parity with desktop windows; iOS apps have no title or size to set.
    pub fn set_title(&self, _: &str) {}
    pub fn resize(&self, _: u32, _: u32) {}
    pub fn set_visible(&self, _: bool) {}

    /// Adds a button to the column at the bottom right and returns its index.
    pub fn add_button(&self, title: &str) -> usize {
        let index = self.state.buttons.borrow().len();
        autoreleasepool(|| {
            // SAFETY: UIKit calls on the main thread; the button is retained in `buttons`.
            unsafe {
                let b =
                    send!(class!("UIButton"), "buttonWithType:", 1 as NSInteger => NSInteger; Id);
                let t = objc::nsstring(title);
                send!(b, "setTitle:forState:", t.id() => Id, 0 as NSUInteger => NSUInteger; ());
                let white = send!(class!("UIColor"), "whiteColor"; Id);
                send!(b, "setTitleColor:forState:", white => Id, 0 as NSUInteger => NSUInteger; ());
                let shade = send!(
                    class!("UIColor"),
                    "colorWithWhite:alpha:",
                    0.0f64 => f64,
                    0.45f64 => f64;
                    Id
                );
                send!(b, "setBackgroundColor:", shade => Id; ());
                send!(send!(b, "layer"; Id), "setCornerRadius:", 10.0f64 => f64; ());
                send!(b, "setTag:", index as NSInteger => NSInteger; ());
                send!(
                    b,
                    "addTarget:action:forControlEvents:",
                    self.button_target.id() => Id,
                    objc::sel!("tap:") => Sel,
                    (1usize << 6) as NSUInteger => NSUInteger;
                    ()
                );
                send!(self.view.id(), "addSubview:", b => Id; ());
                if let Some(b) = Obj::retained(b) {
                    self.state.buttons.borrow_mut().push(b);
                }
            }
        });
        self.layout_buttons();
        index
    }

    /// Changes the title of button `index`.
    pub fn set_button_title(&self, index: usize, title: &str) {
        if let Some(b) = self.state.buttons.borrow().get(index) {
            autoreleasepool(|| {
                let t = objc::nsstring(title);
                // SAFETY: a live UIButton on the main thread.
                unsafe {
                    send!(b.id(), "setTitle:forState:", t.id() => Id, 0 as NSUInteger => NSUInteger; ())
                };
            });
        }
    }

    /// The path of a file copied into the app bundle's resources.
    pub fn resource(name: &str) -> Option<PathBuf> {
        autoreleasepool(|| {
            // SAFETY: reading the main bundle's resource path.
            let dir = unsafe {
                let bundle = send!(class!("NSBundle"), "mainBundle"; Id);
                objc::string(send!(bundle, "resourcePath"; Id))
            };
            let path = PathBuf::from(dir).join(name);
            path.exists().then_some(path)
        })
    }

    pub(crate) fn metal_layer(&self) -> Id {
        self.layer
    }

    fn update_size(&self) {
        // SAFETY: queries on the live view.
        let size = unsafe {
            let bounds: CGRect = send_stret!(self.view.id(), "bounds"; CGRect);
            let scale = send!(self.view.id(), "contentScaleFactor"; f64).max(1.0);
            (
                (bounds.size.width * scale).round().max(0.0) as u32,
                (bounds.size.height * scale).round().max(0.0) as u32,
            )
        };
        if self.state.size.replace(size) != size {
            self.state
                .inputs
                .borrow_mut()
                .push_back(Input::Event(Event::Resized(size.0, size.1)));
        }
    }

    /// Stacks the buttons upwards from the bottom-right corner of the safe area.
    fn layout_buttons(&self) {
        let buttons = self.state.buttons.borrow();
        // SAFETY: layout of live subviews on the main thread.
        unsafe {
            let bounds: CGRect = send_stret!(self.view.id(), "bounds"; CGRect);
            let safe: UIEdgeInsets = send_stret!(self.view.id(), "safeAreaInsets"; UIEdgeInsets);
            let (h, w) = BUTTON;
            let gap = 10.0;
            for (i, b) in buttons.iter().enumerate() {
                let frame = CGRect {
                    origin: CGPoint {
                        x: bounds.size.width - safe.right - gap - w,
                        y: bounds.size.height - safe.bottom - gap - (h + gap) * (i as f64 + 1.0)
                            + gap,
                    },
                    size: CGSize {
                        width: w,
                        height: h,
                    },
                };
                send!(b.id(), "setFrame:", frame => CGRect; ());
            }
        }
    }
}
