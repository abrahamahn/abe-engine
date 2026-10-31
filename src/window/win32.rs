use super::{Event, Key};
use core::ffi::c_void;
use core::ptr::null_mut;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
type H = *mut c_void;
#[repr(C)]
struct Class {
    style: u32,
    proc: Option<unsafe extern "system" fn(H, u32, usize, isize) -> isize>,
    class_bytes: i32,
    window_bytes: i32,
    instance: H,
    icon: H,
    cursor: H,
    brush: H,
    menu: *const u16,
    name: *const u16,
}
#[repr(C)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
struct Msg {
    hwnd: H,
    msg: u32,
    w: usize,
    l: isize,
    time: u32,
    pt: [i32; 2],
    private: u32,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> H;
    fn GetLastError() -> u32;
}
#[link(name = "user32")]
unsafe extern "system" {
    fn RegisterClassW(class: *const Class) -> u16;
    fn CreateWindowExW(
        ex: u32,
        class: *const u16,
        title: *const u16,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        parent: H,
        menu: H,
        instance: H,
        param: H,
    ) -> H;
    fn DestroyWindow(w: H) -> i32;
    fn SetWindowTextW(w: H, text: *const u16) -> i32;
    fn ShowWindow(w: H, cmd: i32) -> i32;
    fn SetWindowPos(w: H, after: H, x: i32, y: i32, width: i32, height: i32, flags: u32) -> i32;
    fn DefWindowProcW(w: H, msg: u32, wp: usize, lp: isize) -> isize;
    #[cfg_attr(target_pointer_width = "32", link_name = "GetWindowLongW")]
    fn GetWindowLongPtrW(w: H, index: i32) -> isize;
    #[cfg_attr(target_pointer_width = "32", link_name = "SetWindowLongW")]
    fn SetWindowLongPtrW(w: H, index: i32, value: isize) -> isize;
    fn AdjustWindowRectEx(r: *mut Rect, style: u32, menu: i32, ex: u32) -> i32;
    fn LoadCursorW(instance: H, resource: *const u16) -> H;
    fn PeekMessageW(msg: *mut Msg, w: H, min: u32, max: u32, remove: u32) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
}
const STYLE: u32 = 0x00cf0000;
struct State {
    events: RefCell<VecDeque<Event>>,
    size: Cell<(u32, u32)>,
    visible: Cell<bool>,
}
pub struct Window {
    pub(crate) instance: H,
    pub(crate) id: H,
    state: Box<State>,
}
fn outer_size(w: u32, h: u32) -> Result<(i32, i32), String> {
    if !(1..=32767).contains(&w) || !(1..=32767).contains(&h) {
        return Err("invalid window size".into());
    }
    let mut r = Rect {
        left: 0,
        top: 0,
        right: w as i32,
        bottom: h as i32,
    };
    if unsafe { AdjustWindowRectEx(&mut r, STYLE, 0, 0) } == 0 {
        return Err("AdjustWindowRectEx failed".into());
    }
    Ok((r.right - r.left, r.bottom - r.top))
}
impl Window {
    pub fn new(title: &str, width: u32, height: u32) -> Result<Self, String> {
        let (w, h) = outer_size(width, height)?;
        let class: Vec<_> = "ABE.Player.Window".encode_utf16().chain([0]).collect();
        if title.contains('\0') {
            return Err("window title contains NUL".into());
        }
        let title: Vec<_> = title.encode_utf16().chain([0]).collect();
        unsafe {
            let instance = GetModuleHandleW(core::ptr::null());
            let wc = Class {
                style: 0,
                proc: Some(wndproc),
                class_bytes: 0,
                window_bytes: 0,
                instance,
                icon: null_mut(),
                cursor: LoadCursorW(null_mut(), 32512usize as *const u16),
                brush: null_mut(),
                menu: core::ptr::null(),
                name: class.as_ptr(),
            };
            if RegisterClassW(&wc) == 0 && GetLastError() != 1410 {
                return Err(format!("RegisterClassW: {}", GetLastError()));
            }
            let id = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                STYLE,
                i32::MIN,
                i32::MIN,
                w,
                h,
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            if id.is_null() {
                return Err(format!("CreateWindowExW: {}", GetLastError()));
            }
            let win = Self {
                id,
                instance,
                state: Box::new(State {
                    events: RefCell::new(VecDeque::new()),
                    size: Cell::new((width, height)),
                    visible: Cell::new(false),
                }),
            };
            // The Box has a stable address. It is only accessed by this window thread
            // and detached before DestroyWindow can invoke the window procedure.
            SetWindowLongPtrW(id, -21, (&*win.state as *const State) as isize);
            ShowWindow(id, 5);
            Ok(win)
        }
    }
    pub fn size(&self) -> (u32, u32) {
        self.state.size.get()
    }
    pub fn visible(&self) -> bool {
        self.state.visible.get()
    }
    pub fn set_visible(&self, on: bool) {
        unsafe {
            ShowWindow(self.id, if on { 5 } else { 0 });
        }
    }
    pub fn resize(&self, width: u32, height: u32) {
        if let Ok((w, h)) = outer_size(width, height) {
            unsafe {
                SetWindowPos(self.id, null_mut(), 0, 0, w, h, 2 | 4 | 16);
            }
        }
    }
    pub fn set_title(&self, title: &str) {
        let t: Vec<u16> = title
            .encode_utf16()
            .filter(|c| *c != 0)
            .chain([0])
            .collect();
        // SAFETY: the window is owned by self and the string outlives the call.
        unsafe {
            SetWindowTextW(self.id, t.as_ptr());
        }
    }
    pub fn poll(&self) -> Option<Event> {
        loop {
            if let Some(e) = self.state.events.borrow_mut().pop_front() {
                return Some(e);
            }
            let mut msg: Msg = unsafe { core::mem::zeroed() };
            unsafe {
                if PeekMessageW(&mut msg, self.id, 0, 0, 1) == 0 {
                    return None;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}
unsafe extern "system" fn wndproc(w: H, msg: u32, wp: usize, lp: isize) -> isize {
    let raw = unsafe { GetWindowLongPtrW(w, -21) } as *const State;
    if let Some(s) = unsafe { raw.as_ref() } {
        let event = match msg {
            0x10 => Some(Event::Close),
            5 => {
                let size = ((lp as u32) & 0xffff, ((lp as u32) >> 16) & 0xffff);
                s.size.set(size);
                if wp == 1 {
                    s.visible.set(false);
                    Some(Event::Visibility(false))
                } else {
                    s.visible.set(true);
                    Some(Event::Resized(size.0, size.1))
                }
            }
            0x18 => {
                s.visible.set(wp != 0);
                Some(Event::Visibility(wp != 0))
            }
            0x200..=0x202 => {
                let (x, y) = ((lp as u16 as i16) as i32, ((lp >> 16) as u16 as i16) as i32);
                Some(match msg {
                    0x201 => Event::Clicked(x, y),
                    0x202 => Event::Unclicked(x, y),
                    _ => Event::Pointer(x, y),
                })
            }
            0x100 | 0x101 => {
                let key = match wp {
                    27 => Some(Key::Escape),
                    32 => Some(Key::Space),
                    37 => Some(Key::Left),
                    38 => Some(Key::Up),
                    39 => Some(Key::Right),
                    40 => Some(Key::Down),
                    0x10 => Some(Key::Shift),
                    k @ (0x30..=0x39) => Some(Key::Char(k as u8)),
                    k @ (0x41..=0x5a) => Some(Key::Char(k as u8 + 32)),
                    _ => None,
                };
                key.map(|k| {
                    if msg == 0x100 {
                        Event::Pressed(k)
                    } else {
                        Event::Released(k)
                    }
                })
            }
            _ => None,
        };
        if let Some(e) = event {
            s.events.borrow_mut().push_back(e);
            return 0;
        }
    }
    unsafe { DefWindowProcW(w, msg, wp, lp) }
}
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.id, -21, 0);
            DestroyWindow(self.id);
        }
    }
}
