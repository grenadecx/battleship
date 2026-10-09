// End-to-end test of the web build: two headless Chromes, one per player, play
// the opening of an online game through a running server. The host opens a
// room, the guest joins with the code, both deploy and the host fires a shot.
// Everything is checked on the WebSocket frames each browser sends and receives,
// read over the Chrome DevTools protocol; screenshots are saved along the way.
//
// Usage: node scripts/web-e2e.mjs HOST_DEVTOOLS_PORT GUEST_DEVTOOLS_PORT URL OUT_DIR
// (scripts/web-e2e.sh starts the browsers.)
import { writeFileSync } from "node:fs";

const [hostPort, guestPort, url, outDir] = process.argv.slice(2);
const sleep = ms => new Promise(r => setTimeout(r, ms));

// Screen positions on the game's 1200x760 virtual canvas, shown at 1:1.
const HOST_BUTTON = [600, 461];
const JOIN_BUTTON = [600, 533];
const ENEMY_A1 = [729, 184];

async function player(name, port) {
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const ws = new WebSocket(targets.find(t => t.type === "page").webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
  let nextId = 0;
  const pending = new Map();
  const p = { name, sent: [], received: [], console: [] };
  ws.onmessage = event => {
    const m = JSON.parse(event.data);
    if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
    if (m.method === "Network.webSocketFrameSent") p.sent.push(m.params.response.payloadData);
    if (m.method === "Network.webSocketFrameReceived") p.received.push(m.params.response.payloadData);
    if (m.method === "Runtime.consoleAPICalled") p.console.push(`${m.params.type}: ${m.params.args.map(a => a.value ?? a.description).join(" ")}`);
    if (m.method === "Runtime.exceptionThrown") p.console.push(`exception: ${m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text}`);
  };
  const call = (method, params = {}) => new Promise(resolve => {
    const id = ++nextId;
    pending.set(id, resolve);
    ws.send(JSON.stringify({ id, method, params }));
  });
  p.waitFor = async (what, check, ms = 10000) => {
    for (const end = Date.now() + ms; Date.now() < end; await sleep(50)) {
      const value = check();
      if (value) return value;
    }
    throw new Error(`${name}: timed out waiting for ${what}\n  sent: ${p.sent.join(" | ")}\n  received: ${p.received.join(" | ")}\n  console: ${p.console.join(" | ")}`);
  };
  p.click = async ([x, y]) => {
    // macroquad tracks the pointer from mouse moves, as a real mouse makes them.
    await call("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
    await sleep(100);
    for (const type of ["mousePressed", "mouseReleased"]) {
      await call("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
    }
    await sleep(150);
  };
  p.key = async (key, code, text) => {
    await call("Input.dispatchKeyEvent", { type: "keyDown", key, code, text });
    await call("Input.dispatchKeyEvent", { type: "keyUp", key, code });
  };
  p.type = async text => {
    for (const ch of text) {
      await p.key(ch, /\d/.test(ch) ? `Digit${ch}` : `Key${ch.toUpperCase()}`, ch);
    }
    await sleep(150);
  };
  p.enter = async () => { await p.key("Enter", "Enter"); await sleep(150); };
  p.screenshot = async file => {
    const shot = await call("Page.captureScreenshot", { format: "png" });
    writeFileSync(`${outDir}/${file}`, Buffer.from(shot.result.data, "base64"));
  };
  /// Clicks until the game reacts with a frame, in case it was still loading.
  p.clickUntilSent = async (target, what, sentFrame) => {
    for (let attempt = 0; attempt < 10; attempt++) {
      await p.click(target);
      try { return await p.waitFor(what, () => p.sent.find(sentFrame), 1500); } catch { /* retry */ }
    }
    return p.waitFor(what, () => p.sent.find(sentFrame), 1);
  };

  for (const domain of ["Runtime", "Network", "Page"]) await call(`${domain}.enable`);
  await call("Emulation.setDeviceMetricsOverride", { width: 1200, height: 760, deviceScaleFactor: 1, mobile: false });
  await call("Page.navigate", { url });
  return p;
}

const host = await player("host", hostPort);
const guest = await player("guest", guestPort);
try {
  await host.clickUntilSent(HOST_BUTTON, "HOST", f => f === "HOST");
  const code = (await host.waitFor("a room code", () => host.received.find(f => f.startsWith("ROOM ")))).slice(5);
  await sleep(300);
  await host.screenshot("1-host-room.png");

  // The guest types the code in lower case; the game upper-cases it.
  await guest.waitFor("the page to load", () => true);
  for (let attempt = 0; !guest.sent.some(f => f.startsWith("JOIN")); attempt++) {
    if (attempt === 10) throw new Error("guest: never managed to join");
    await guest.click(JOIN_BUTTON);
    await guest.type(code.toLowerCase());
    await guest.enter();
    await sleep(1000);
  }
  await guest.waitFor("pairing", () => guest.received.includes("PAIRED"));
  await host.waitFor("the guest's handshake", () => host.received.some(f => f.startsWith("HELLO BATTLESHIP")));
  await guest.waitFor("the host's handshake", () => guest.received.some(f => f.startsWith("HELLO BATTLESHIP")));
  await sleep(500);
  await guest.screenshot("2-guest-deploy.png");

  // Fleets start out randomly placed, so Enter deploys them straight away.
  await host.enter();
  await guest.enter();
  await host.waitFor("the guest's READY", () => host.received.includes("READY"));
  await guest.waitFor("the host's READY", () => guest.received.includes("READY"));

  // The host opens the battle.
  await host.clickUntilSent(ENEMY_A1, "a shot at A1", f => f === "FIRE 0 0");
  await guest.waitFor("the shot", () => guest.received.includes("FIRE 0 0"));
  const result = await host.waitFor("the result", () => host.received.find(f => f.startsWith("RESULT")));
  await host.waitFor("a latency measurement", () => host.received.some(f => f.startsWith("PONG")));
  await sleep(1500);
  await host.screenshot("3-host-battle.png");
  await guest.screenshot("4-guest-battle.png");

  const exceptions = [host, guest].flatMap(p => p.console.filter(l => l.startsWith("exception")));
  if (exceptions.length) throw new Error(`JavaScript exceptions:\n  ${exceptions.join("\n  ")}`);
  const game = f => !/^(PING|PONG)/.test(f);
  console.log(`Room ${code}: the host fired at A1, ${result}.`);
  console.log(`  host sent:  ${host.sent.filter(game).join(" | ")}`);
  console.log(`  guest sent: ${guest.sent.filter(game).join(" | ")}`);
  process.exit(0);
} catch (error) {
  console.error(error.message);
  await host.screenshot("failure-host.png").catch(() => {});
  await guest.screenshot("failure-guest.png").catch(() => {});
  process.exit(1);
}
