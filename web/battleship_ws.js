// WebSocket bridge for the browser build of Battleship (see
// crates/game/src/web_socket.rs). Sockets connect to /ws on the server that
// served the page. Messages are queued here until the game polls for them,
// so nothing is lost between frames, even after the socket closed.
(function () {
    "use strict";

    const CONNECTING = 0, OPEN = 1, CLOSED = 2;
    const sockets = new Map();
    const encoder = new TextEncoder();
    const decoder = new TextDecoder();
    let nextId = 1;

    function relayUrl() {
        const scheme = window.location.protocol === "https:" ? "wss://" : "ws://";
        return scheme + window.location.host + "/ws";
    }

    function register_plugin(importObject) {
        const env = importObject.env;

        env.battleship_ws_open = function () {
            const id = nextId++;
            const entry = { state: CONNECTING, inbox: [], ws: null };
            sockets.set(id, entry);
            try {
                const ws = new WebSocket(relayUrl());
                ws.onopen = () => { entry.state = OPEN; };
                ws.onmessage = (event) => {
                    if (typeof event.data === "string") {
                        entry.inbox.push(encoder.encode(event.data));
                    }
                };
                ws.onclose = () => { entry.state = CLOSED; };
                ws.onerror = () => { entry.state = CLOSED; };
                entry.ws = ws;
            } catch (error) {
                console.error("Battleship: could not open a WebSocket", error);
                entry.state = CLOSED;
            }
            return id;
        };

        env.battleship_ws_state = function (id) {
            const entry = sockets.get(id);
            return entry ? entry.state : CLOSED;
        };

        env.battleship_ws_send = function (id, ptr, len) {
            const entry = sockets.get(id);
            if (entry && entry.state === OPEN) {
                entry.ws.send(decoder.decode(new Uint8Array(wasm_memory.buffer, ptr, len)));
            }
        };

        env.battleship_ws_next_len = function (id) {
            const entry = sockets.get(id);
            return entry && entry.inbox.length > 0 ? entry.inbox[0].length : -1;
        };

        env.battleship_ws_take = function (id, ptr) {
            const bytes = sockets.get(id).inbox.shift();
            new Uint8Array(wasm_memory.buffer, ptr, bytes.length).set(bytes);
        };

        env.battleship_ws_close = function (id) {
            const entry = sockets.get(id);
            if (entry) {
                if (entry.ws) {
                    entry.ws.close();
                }
                sockets.delete(id);
            }
        };
    }

    miniquad_add_plugin({ register_plugin, name: "battleship_ws", version: 1 });
})();
