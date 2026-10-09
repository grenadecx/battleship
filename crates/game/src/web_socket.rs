//! The browser's WebSocket, reached through the JavaScript plugin in
//! `web/battleship_ws.js`. Each [`JsSocket`] is one socket to the relay server
//! at `/ws` on the server that served the page.

use battleship_core::relay_link::{SocketState, Transport};

// Provided by the plugin through the import object's `env`, like macroquad's own
// imports. Newer Rust no longer links undefined wasm symbols without this.
#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn battleship_ws_open() -> i32;
    fn battleship_ws_state(id: i32) -> i32;
    fn battleship_ws_send(id: i32, text: *const u8, len: usize);
    fn battleship_ws_next_len(id: i32) -> i32;
    fn battleship_ws_take(id: i32, buffer: *mut u8);
    fn battleship_ws_close(id: i32);
}

/// Matched against the plugin's own version when the page loads, so a stale
/// `battleship_ws.js` shows up in the browser console.
#[unsafe(no_mangle)]
pub extern "C" fn battleship_ws_crate_version() -> u32 {
    1
}

pub struct JsSocket {
    id: i32,
}

impl JsSocket {
    /// Starts connecting; [`Transport::state`] tells when it is open.
    pub fn open() -> Self {
        JsSocket {
            id: unsafe { battleship_ws_open() },
        }
    }
}

impl Transport for JsSocket {
    fn state(&self) -> SocketState {
        match unsafe { battleship_ws_state(self.id) } {
            0 => SocketState::Connecting,
            1 => SocketState::Open,
            _ => SocketState::Closed,
        }
    }

    fn send(&mut self, text: &str) {
        unsafe { battleship_ws_send(self.id, text.as_ptr(), text.len()) }
    }

    fn recv(&mut self) -> Option<String> {
        let len = usize::try_from(unsafe { battleship_ws_next_len(self.id) }).ok()?;
        let mut buffer = vec![0u8; len];
        unsafe { battleship_ws_take(self.id, buffer.as_mut_ptr()) }
        Some(String::from_utf8_lossy(&buffer).into_owned())
    }

    fn close(&mut self) {
        unsafe { battleship_ws_close(self.id) }
    }

    fn now(&self) -> f64 {
        macroquad::miniquad::date::now()
    }
}

impl Drop for JsSocket {
    fn drop(&mut self) {
        self.close();
    }
}
