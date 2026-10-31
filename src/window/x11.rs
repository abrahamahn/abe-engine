use super::{Event, Key};
use core::ffi::{c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::{cell::Cell, ffi::CString, ptr::null, sync::OnceLock};

#[repr(C)]
#[derive(Clone, Copy)]
struct KeyEvent {
    kind: c_int,
    serial: c_ulong,
    send: c_int,
    display: *mut c_void,
    window: c_ulong,
    root: c_ulong,
    sub: c_ulong,
    time: c_ulong,
    x: c_int,
    y: c_int,
    rx: c_int,
    ry: c_int,
    state: c_uint,
    code: c_uint,
    same: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Configure {
    kind: c_int,
    serial: c_ulong,
    send: c_int,
    display: *mut c_void,
    event: c_ulong,
    window: c_ulong,
    x: c_int,
    y: c_int,
    width: c_int,
    height: c_int,
    border: c_int,
    above: c_ulong,
    override_redirect: c_int,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct Client {
    kind: c_int,
    serial: c_ulong,
    send: c_int,
    display: *mut c_void,
    window: c_ulong,
    message: c_ulong,
    format: c_int,
    data: [c_long; 5],
}
#[repr(C)]
union XEvent {
    kind: c_int,
    key: KeyEvent,
    config: Configure,
    client: Client,
    pad: [c_long; 24],
}
#[link(name = "libX11.so.6", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn XInitThreads() -> c_int;
    fn XOpenDisplay(name: *const c_char) -> *mut c_void;
    fn XCloseDisplay(d: *mut c_void) -> c_int;
    fn XDefaultRootWindow(d: *mut c_void) -> c_ulong;
    fn XCreateSimpleWindow(
        d: *mut c_void,
        parent: c_ulong,
        x: c_int,
        y: c_int,
        w: c_uint,
        h: c_uint,
        border: c_uint,
        line: c_ulong,
        bg: c_ulong,
    ) -> c_ulong;
    fn XDestroyWindow(d: *mut c_void, w: c_ulong) -> c_int;
    fn XStoreName(d: *mut c_void, w: c_ulong, name: *const c_char) -> c_int;
    fn XSelectInput(d: *mut c_void, w: c_ulong, mask: c_long) -> c_int;
    fn XInternAtom(d: *mut c_void, name: *const c_char, only_if_exists: c_int) -> c_ulong;
    fn XSetWMProtocols(d: *mut c_void, w: c_ulong, atoms: *mut c_ulong, count: c_int) -> c_int;
    fn XMapWindow(d: *mut c_void, w: c_ulong) -> c_int;
    fn XUnmapWindow(d: *mut c_void, w: c_ulong) -> c_int;
    fn XResizeWindow(d: *mut c_void, w: c_ulong, width: c_uint, height: c_uint) -> c_int;
    fn XFlush(d: *mut c_void) -> c_int;
    fn XPending(d: *mut c_void) -> c_int;
    fn XNextEvent(d: *mut c_void, e: *mut XEvent) -> c_int;
    fn XLookupKeysym(e: *mut KeyEvent, index: c_int) -> c_ulong;
}
pub struct Window {
    pub(crate) display: *mut c_void,
    pub(crate) id: c_ulong,
    extent: Cell<(u32, u32)>,
    mapped: Cell<bool>,
    protocols: c_ulong,
    delete: c_ulong,
}
impl Window {
    pub fn new(title: &str, width: u32, height: u32) -> Result<Self, String> {
        if !(1..=32767).contains(&width) || !(1..=32767).contains(&height) {
            return Err("invalid window size".into());
        }
        let title = CString::new(title).map_err(|_| "window title contains NUL")?;
        static THREAD_SUPPORT: OnceLock<bool> = OnceLock::new();
        if !*THREAD_SUPPORT.get_or_init(|| unsafe { XInitThreads() != 0 }) {
            return Err("Xlib thread support unavailable".into());
        }
        // SAFETY: this thread owns the display and the XID until drop; strings outlive the calls.
        unsafe {
            let d = XOpenDisplay(null());
            if d.is_null() {
                return Err(
                    "cannot open X11 display; set DISPLAY or use the offscreen renderer".into(),
                );
            }
            let id = XCreateSimpleWindow(d, XDefaultRootWindow(d), 0, 0, width, height, 0, 0, 0);
            if id == 0 {
                XCloseDisplay(d);
                return Err("XCreateSimpleWindow failed".into());
            }
            // Key press/release, button press/release, pointer motion, structure changes.
            XSelectInput(d, id, 1 | 2 | 4 | 8 | (1 << 6) | (1 << 17));
            XStoreName(d, id, title.as_ptr());
            let protocols = XInternAtom(d, c"WM_PROTOCOLS".as_ptr(), 0);
            let mut delete = XInternAtom(d, c"WM_DELETE_WINDOW".as_ptr(), 0);
            XSetWMProtocols(d, id, &mut delete, 1);
            XMapWindow(d, id);
            XFlush(d);
            Ok(Self {
                display: d,
                id,
                extent: Cell::new((width, height)),
                mapped: Cell::new(false),
                protocols,
                delete,
            })
        }
    }
    pub fn size(&self) -> (u32, u32) {
        self.extent.get()
    }
    pub fn visible(&self) -> bool {
        self.mapped.get()
    }
    pub fn resize(&self, w: u32, h: u32) {
        if (1..=32767).contains(&w) && (1..=32767).contains(&h) {
            unsafe {
                XResizeWindow(self.display, self.id, w, h);
                XFlush(self.display);
            }
        }
    }
    pub fn set_visible(&self, on: bool) {
        unsafe {
            if on {
                XMapWindow(self.display, self.id);
            } else {
                XUnmapWindow(self.display, self.id);
            }
            XFlush(self.display);
        }
    }
    pub fn set_title(&self, title: &str) {
        if let Ok(t) = CString::new(title) {
            // SAFETY: the display and window are owned by self; the string outlives the call.
            unsafe {
                XStoreName(self.display, self.id, t.as_ptr());
                XFlush(self.display);
            }
        }
    }
    pub fn poll(&self) -> Option<Event> {
        unsafe {
            while XPending(self.display) > 0 {
                let mut e = XEvent { pad: [0; 24] };
                XNextEvent(self.display, &mut e);
                match e.kind {
                    2 | 3 => {
                        let key = match XLookupKeysym(&mut e.key, 0) {
                            0xff1b => Key::Escape,
                            32 => Key::Space,
                            0xff51 => Key::Left,
                            0xff52 => Key::Up,
                            0xff53 => Key::Right,
                            0xff54 => Key::Down,
                            0xffe1 | 0xffe2 => Key::Shift,
                            k @ (0x30..=0x39 | 0x61..=0x7a) => Key::Char(k as u8),
                            k @ 0x41..=0x5a => Key::Char(k as u8 + 32),
                            _ => continue,
                        };
                        return Some(if e.kind == 2 {
                            Event::Pressed(key)
                        } else {
                            Event::Released(key)
                        });
                    }
                    4 if e.key.code == 1 => return Some(Event::Clicked(e.key.x, e.key.y)),
                    5 if e.key.code == 1 => return Some(Event::Unclicked(e.key.x, e.key.y)),
                    6 => return Some(Event::Pointer(e.key.x, e.key.y)),
                    18 | 19 => {
                        let shown = e.kind == 19;
                        self.mapped.set(shown);
                        return Some(Event::Visibility(shown));
                    }
                    22 => {
                        let c = e.config;
                        let size = (c.width.max(0) as u32, c.height.max(0) as u32);
                        if self.extent.replace(size) != size {
                            return Some(Event::Resized(size.0, size.1));
                        }
                    }
                    33 => {
                        let m = e.client;
                        if m.message == self.protocols
                            && m.format == 32
                            && m.data[0] as c_ulong == self.delete
                        {
                            return Some(Event::Close);
                        }
                    }
                    _ => {}
                }
            }
        }
        None
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            XDestroyWindow(self.display, self.id);
            XCloseDisplay(self.display);
        }
    }
}
