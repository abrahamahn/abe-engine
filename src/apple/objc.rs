//! A minimal binding to the Objective-C runtime, the only door to Metal, QuartzCore, AppKit and
//! UIKit. Everything unsafe about messaging lives here:
//!
//! * [`send!`] calls `objc_msgSend` cast to the exact C signature of the method at each call
//!   site (arm64 has no variadic shortcut), with the selector registered once per call site.
//! * [`Obj`] owns one reference (+1) and releases it on drop. Objects returned by `new…`,
//!   `alloc`/`init…` and `copy…` are owned ([`Obj::owned`]); everything else is autoreleased
//!   and must be retained ([`Obj::retained`]) to outlive the current autorelease pool.
//! * [`autoreleasepool`] drains autoreleased objects; the engine has no run loop to do it, so
//!   every public entry point that messages Objective-C runs inside one.
//! * Classes are created at runtime with [`ClassBuilder`] for delegates and views.
//!
//! Threading: `Obj` is neither `Send` nor `Sync`. UIKit and AppKit objects may only be used on
//! the main thread; Metal objects are only used from the thread that owns the device.
#![allow(dead_code)]

use core::ffi::{CStr, c_char, c_void};
use core::ptr::NonNull;

/// An Objective-C object pointer (`id`). May be null.
pub type Id = *mut c_void;

/// A registered selector (`SEL`).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sel(pub *const c_void);

/// `NSUInteger` / `NSInteger` on 64-bit Apple platforms.
pub type NSUInteger = usize;
pub type NSInteger = isize;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NSRange {
    pub location: NSUInteger,
    pub length: NSUInteger,
}

#[link(name = "objc", kind = "dylib")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> *const c_void;
    /// Never called through this declaration: always cast to the method's signature first.
    fn objc_msgSend();
    #[cfg(target_arch = "x86_64")]
    fn objc_msgSend_stret();
    fn objc_retain(obj: Id) -> Id;
    fn objc_release(obj: Id);
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
    fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra: usize) -> Id;
    fn objc_registerClassPair(class: Id);
    fn class_addMethod(class: Id, sel: Sel, imp: *const c_void, types: *const c_char) -> i8;
    fn class_addIvar(
        class: Id,
        name: *const c_char,
        size: usize,
        log2_alignment: u8,
        types: *const c_char,
    ) -> i8;
    fn class_getInstanceVariable(class: Id, name: *const c_char) -> *const c_void;
    fn ivar_getOffset(ivar: *const c_void) -> isize;
    fn object_getClass(obj: Id) -> Id;
    fn objc_getProtocol(name: *const c_char) -> *mut c_void;
    fn class_addProtocol(class: Id, protocol: *mut c_void) -> i8;
}

#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {}

/// `objc_msgSend` as an untyped function pointer, for [`send!`] to cast.
#[doc(hidden)]
pub fn msg_send_fn() -> unsafe extern "C" fn() {
    objc_msgSend
}

/// `objc_msgSend_stret` on x86_64 (methods returning structs larger than 16 bytes); arm64
/// returns every struct through `objc_msgSend`.
#[doc(hidden)]
pub fn msg_send_stret_fn() -> unsafe extern "C" fn() {
    #[cfg(target_arch = "x86_64")]
    {
        objc_msgSend_stret
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        objc_msgSend
    }
}

#[doc(hidden)]
pub fn register_selector(name: &CStr) -> Sel {
    // SAFETY: the name is a NUL-terminated string; registration is thread-safe and idempotent.
    Sel(unsafe { sel_registerName(name.as_ptr()) })
}

/// A selector, registered once per call site.
macro_rules! sel {
    ($name:literal) => {{
        static CACHE: core::sync::atomic::AtomicPtr<core::ffi::c_void> =
            core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
        let mut p = CACHE.load(core::sync::atomic::Ordering::Relaxed);
        if p.is_null() {
            let s = $crate::apple::objc::register_selector(
                match core::ffi::CStr::from_bytes_with_nul(concat!($name, "\0").as_bytes()) {
                    Ok(name) => name,
                    Err(_) => panic!("selector names contain no NUL"),
                },
            );
            p = s.0 as *mut core::ffi::c_void;
            CACHE.store(p, core::sync::atomic::Ordering::Relaxed);
        }
        $crate::apple::objc::Sel(p as *const core::ffi::c_void)
    }};
}
pub(crate) use sel;

/// A class by name, looked up once per call site (null if the class does not exist).
macro_rules! class {
    ($name:literal) => {{
        static CACHE: core::sync::atomic::AtomicPtr<core::ffi::c_void> =
            core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
        let mut p = CACHE.load(core::sync::atomic::Ordering::Relaxed);
        if p.is_null() {
            p = $crate::apple::objc::class_named(
                match core::ffi::CStr::from_bytes_with_nul(concat!($name, "\0").as_bytes()) {
                    Ok(name) => name,
                    Err(_) => panic!("class names contain no NUL"),
                },
            );
            CACHE.store(p, core::sync::atomic::Ordering::Relaxed);
        }
        p
    }};
}
pub(crate) use class;

/// Sends `selector` to `receiver` with arguments of the given C types and returns `ret`:
/// `send!(obj, "setWidth:", w => usize; ())`. The caller asserts that the receiver responds
/// to the selector with exactly this signature (an unsafe operation).
macro_rules! send {
    ($obj:expr, $sel:literal $(, $arg:expr => $ty:ty)* ; $ret:ty) => {{
        let f: unsafe extern "C" fn(
            $crate::apple::objc::Id,
            $crate::apple::objc::Sel
            $(, $ty)*
        ) -> $ret = core::mem::transmute::<unsafe extern "C" fn(), _>(
            $crate::apple::objc::msg_send_fn(),
        );
        f($obj, $crate::apple::objc::sel!($sel) $(, $arg)*)
    }};
}
pub(crate) use send;

/// [`send!`] for methods that return structs larger than 16 bytes (`NSRect`, `CGRect`).
macro_rules! send_stret {
    ($obj:expr, $sel:literal $(, $arg:expr => $ty:ty)* ; $ret:ty) => {{
        let f: unsafe extern "C" fn(
            $crate::apple::objc::Id,
            $crate::apple::objc::Sel
            $(, $ty)*
        ) -> $ret = core::mem::transmute::<unsafe extern "C" fn(), _>(
            $crate::apple::objc::msg_send_stret_fn(),
        );
        f($obj, $crate::apple::objc::sel!($sel) $(, $arg)*)
    }};
}
pub(crate) use send_stret;

#[doc(hidden)]
pub fn class_named(name: &CStr) -> Id {
    // SAFETY: a lookup by NUL-terminated name.
    unsafe { objc_getClass(name.as_ptr()) }
}

/// One owned (+1) reference to an Objective-C object, released on drop.
#[derive(Debug)]
pub struct Obj(NonNull<c_void>);

impl Obj {
    /// Takes ownership of a +1 reference (`new…`, `alloc`/`init…`, `copy…`). `None` for nil.
    ///
    /// # Safety
    /// `id` must be nil or an object the caller owns one reference to.
    pub unsafe fn owned(id: Id) -> Option<Obj> {
        NonNull::new(id).map(Obj)
    }
    /// Retains an autoreleased (+0) object. `None` for nil.
    ///
    /// # Safety
    /// `id` must be nil or a live object.
    pub unsafe fn retained(id: Id) -> Option<Obj> {
        // SAFETY: the caller guarantees a live object; retaining keeps it alive past the pool.
        NonNull::new(unsafe { objc_retain(id) }).map(Obj)
    }
    pub fn id(&self) -> Id {
        self.0.as_ptr()
    }
}

impl Clone for Obj {
    fn clone(&self) -> Self {
        // SAFETY: self holds a reference, so the object is alive.
        unsafe { objc_retain(self.id()) };
        Obj(self.0)
    }
}

impl Drop for Obj {
    fn drop(&mut self) {
        // SAFETY: self owns exactly one reference.
        unsafe { objc_release(self.id()) }
    }
}

/// Runs `f` inside an autorelease pool, draining it afterwards (also on panic).
pub fn autoreleasepool<R>(f: impl FnOnce() -> R) -> R {
    struct Pool(*mut c_void);
    impl Drop for Pool {
        fn drop(&mut self) {
            // SAFETY: pops the pool pushed below, on the same thread, in LIFO order.
            unsafe { objc_autoreleasePoolPop(self.0) }
        }
    }
    // SAFETY: pushing a pool has no preconditions.
    let _pool = Pool(unsafe { objc_autoreleasePoolPush() });
    f()
}

/// A new `NSString` (+1) with the UTF-8 contents of `s`.
pub fn nsstring(s: &str) -> Obj {
    // SAFETY: `initWithBytes:length:encoding:` copies the bytes; 4 = NSUTF8StringEncoding.
    unsafe {
        let alloc = send!(class!("NSString"), "alloc"; Id);
        let id = send!(
            alloc,
            "initWithBytes:length:encoding:",
            s.as_ptr() as *const c_void => *const c_void,
            s.len() => NSUInteger,
            4usize => NSUInteger;
            Id
        );
        Obj::owned(id).unwrap_or_else(|| nsstring("?"))
    }
}

/// The UTF-8 contents of an `NSString` (empty for nil).
///
/// # Safety
/// `s` must be nil or a live `NSString`.
pub unsafe fn string(s: Id) -> String {
    if s.is_null() {
        return String::new();
    }
    // SAFETY: UTF8String returns a pointer valid while `s` and the pool live; copied at once.
    unsafe {
        let p = send!(s, "UTF8String"; *const c_char);
        if p.is_null() {
            String::new()
        } else {
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
    }
}

/// `localizedDescription` of an `NSError` (or a generic message for nil).
///
/// # Safety
/// `error` must be nil or a live `NSError`.
pub unsafe fn error_text(error: Id) -> String {
    if error.is_null() {
        return "unknown error".into();
    }
    // SAFETY: the caller guarantees a live NSError.
    unsafe { string(send!(error, "localizedDescription"; Id)) }
}

/// Builds an Objective-C class at runtime (delegates, views).
pub struct ClassBuilder(Id);

impl ClassBuilder {
    /// A new subclass of `superclass`, or `None` if a class with this name already exists
    /// (look it up with [`class_named`] then).
    pub fn new(name: &CStr, superclass: Id) -> Option<ClassBuilder> {
        // SAFETY: allocating a class pair only reads the names; it fails for duplicates.
        let c = unsafe { objc_allocateClassPair(superclass, name.as_ptr(), 0) };
        (!c.is_null()).then_some(ClassBuilder(c))
    }
    /// Adds an instance method. `types` is its Objective-C type encoding (e.g. `c"v@:@"`).
    ///
    /// # Safety
    /// `imp` must be an `extern "C"` function whose signature matches `types`, taking the
    /// receiver and the selector first.
    pub unsafe fn method(&mut self, sel: Sel, imp: *const c_void, types: &CStr) {
        // SAFETY: the caller guarantees the implementation matches the encoding.
        unsafe { class_addMethod(self.0, sel, imp, types.as_ptr()) };
    }
    /// Adds a class method (for `+layerClass`).
    ///
    /// # Safety
    /// As for [`ClassBuilder::method`].
    pub unsafe fn class_method(&mut self, sel: Sel, imp: *const c_void, types: &CStr) {
        // SAFETY: the metaclass of the class being built receives class methods.
        unsafe {
            let meta = object_getClass(self.0);
            class_addMethod(meta, sel, imp, types.as_ptr());
        }
    }
    /// Declares conformance to a protocol the frameworks have registered (UIKit checks it for
    /// some delegates). Unknown protocols are ignored.
    pub fn protocol(&mut self, name: &CStr) {
        // SAFETY: looking up a protocol by name and adding it before registration.
        unsafe {
            let p = objc_getProtocol(name.as_ptr());
            if !p.is_null() {
                class_addProtocol(self.0, p);
            }
        }
    }
    /// Adds a pointer-sized instance variable.
    pub fn pointer_ivar(&mut self, name: &CStr) {
        // SAFETY: adding an ivar before registration; 3 = log2(8) alignment, "^v" = void*.
        unsafe {
            class_addIvar(
                self.0,
                name.as_ptr(),
                size_of::<*mut c_void>(),
                3,
                c"^v".as_ptr(),
            )
        };
    }
    pub fn register(self) -> Id {
        // SAFETY: the class pair was allocated by `new` and is registered once.
        unsafe { objc_registerClassPair(self.0) };
        self.0
    }
}

/// Reads a pointer instance variable added with [`ClassBuilder::pointer_ivar`].
///
/// # Safety
/// `obj` must be a live instance of a class that has this ivar.
pub unsafe fn get_ivar(obj: Id, name: &CStr) -> *mut c_void {
    // SAFETY: the ivar exists and holds a pointer at the runtime-reported offset.
    unsafe {
        let ivar = class_getInstanceVariable(object_getClass(obj), name.as_ptr());
        if ivar.is_null() {
            return core::ptr::null_mut();
        }
        *(obj as *mut u8)
            .offset(ivar_getOffset(ivar))
            .cast::<*mut c_void>()
    }
}

/// Writes a pointer instance variable added with [`ClassBuilder::pointer_ivar`].
///
/// # Safety
/// As for [`get_ivar`].
pub unsafe fn set_ivar(obj: Id, name: &CStr, value: *mut c_void) {
    // SAFETY: as in get_ivar.
    unsafe {
        let ivar = class_getInstanceVariable(object_getClass(obj), name.as_ptr());
        if !ivar.is_null() {
            *(obj as *mut u8)
                .offset(ivar_getOffset(ivar))
                .cast::<*mut c_void>() = value;
        }
    }
}

/// Is the current thread the main thread?
pub fn is_main_thread() -> bool {
    // SAFETY: a class method with no arguments returning BOOL.
    unsafe { send!(class!("NSThread"), "isMainThread"; i8) != 0 }
}
