//! Ganbate Studio's host: it serves the editor interface, owns the project folder (the only writer
//! of its files), keeps every open document's undo history and speaks the edit protocol over
//! WebSocket to the interface, running games and phones. Standard library only.
pub mod ai;
pub mod http;
pub mod server;
pub mod session;
pub mod ws;
