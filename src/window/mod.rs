//! A single-threaded native window and a small, game-independent input vocabulary.
#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
pub use x11::Window;
#[cfg(target_os = "windows")]
mod win32;
#[cfg(target_os = "windows")]
pub use win32::Window;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::Window;
#[cfg(target_os = "ios")]
pub mod ios;
#[cfg(target_os = "ios")]
pub use ios::Window;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Escape,
    Space,
    Left,
    Right,
    Up,
    Down,
    Shift,
    /// A letter (lower-case ASCII) or digit key.
    Char(u8),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Close,
    Resized(u32, u32),
    Visibility(bool),
    Pressed(Key),
    Released(Key),
    /// Left button pressed at window coordinates.
    Clicked(i32, i32),
    /// Left button released.
    Unclicked(i32, i32),
    /// Pointer moved to window coordinates.
    Pointer(i32, i32),
    /// Keyboard focus gained or lost. Keys held when focus is lost send no release.
    Focus(bool),
}
