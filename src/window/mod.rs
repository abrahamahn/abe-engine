//! A single-threaded native window and a small, game-independent input vocabulary.
#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
pub use x11::Window;
#[cfg(target_os = "windows")]
mod win32;
#[cfg(target_os = "windows")]
pub use win32::Window;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
    Space,
    Left,
    Right,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Close,
    Resized(u32, u32),
    Visibility(bool),
    Pressed(Key),
    Clicked(i32, i32),
}
