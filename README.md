# Battleship

The *Tab 6 - Immersion* lab, built in Rust with test-driven development. It has a
graphical interface, animations and sound, and you can play against the computer or
against a friend. It comes in two forms, built from the same source:

* **Desktop**: a single executable. Friends play each other directly over TCP.
* **Browser**: the same game compiled to WebAssembly, served by a small web service in
  a Docker container. Friends meet in a room on that service with a short code.

## Download

Ready-made builds for Linux x86_64, Windows x86_64 and macOS on Apple silicon are on
the [releases page](https://github.com/grenadecx/battleship/releases/latest). Unpack the
archive and run `battleship` from the unpacked folder.

### macOS: "Apple could not verify 'battleship'"

The macOS build is not signed or notarized by Apple, so Gatekeeper blocks it when it was
downloaded with a browser. Use one of these:

* **Terminal**: in the unpacked folder, clear the quarantine flag and start the game:

  ```sh
  xattr -d com.apple.quarantine battleship
  ./battleship
  ```

* **Finder**: open `battleship` once and dismiss the warning, then go to
  **System Settings → Privacy & Security** and click **Open Anyway**. On macOS 15
  (Sequoia) and later, right-click → Open no longer skips the warning.

* **Download with `curl`**: files downloaded on the command line never get the
  quarantine flag. Set `v` to the version you want:

  ```sh
  v=0.2.0
  curl -LO https://github.com/grenadecx/battleship/releases/download/v$v/battleship-v$v-macos-arm64.tar.gz
  tar xzf battleship-v$v-macos-arm64.tar.gz
  ./battleship-v$v-macos-arm64/battleship
  ```

### Build it yourself

Anything you compile yourself runs without Gatekeeper warnings, on every platform. With
[Rust](https://rustup.rs) installed:

```sh
cargo install --git https://github.com/grenadecx/battleship --tag v0.2.0 battleship
battleship
```

This puts `battleship` in `~/.cargo/bin`. Leave out `--tag` to build the latest code on
`main` instead of a release.

## Build and run

```sh
cargo run --release -p battleship    # play
cargo test --workspace               # run the test suite
```

You get one self-contained binary, `target/release/battleship`, with the font and
sounds built in. Copy it anywhere and run it.

Platform notes:

* **Linux**: you need an X11 or Wayland (XWayland) desktop with OpenGL, plus
  `libasound.so.2` (ALSA), which every desktop distro installs. You don't need the ALSA
  `-dev` package: `build.rs` links against the runtime library when the dev symlink is
  missing.
* **Windows / macOS**: `cargo build --release` works with no extra dependencies.
  Build on the target OS, or cross-compile (for example
  `rustup target add x86_64-pc-windows-gnu` plus the mingw-w64 toolchain).

## Web version

The browser version needs the web service, which serves the game and relays online
games between browsers (they can't connect to each other directly). Run it with Docker:

```sh
docker build -t battleship .
docker run --rm -p 8080:8080 battleship
```

Then open <http://localhost:8080>. Each release also publishes the image as
`ghcr.io/grenadecx/battleship:<version>`.

Without Docker, build the web files and start the service yourself:

```sh
rustup target add wasm32-unknown-unknown
scripts/build-web.sh                  # writes target/web/
cargo run --release -p battleship-server
```

The service listens on `PORT` (default 8080) and serves the files in
`BATTLESHIP_WEB_DIR` (default `target/web`). `/healthz` answers `ok`, which the Docker
image uses for its health check. Put it behind a reverse proxy with TLS when it faces
the internet: pages served over HTTPS can only open secure WebSockets (`wss://`).
`deploy/` has a ready setup for a server of its own: the published image behind
[Caddy](https://caddyserver.com), which gets the certificate itself. Point a domain at
the server, open ports 80 and 443, copy `deploy/` there, then:

```sh
echo BATTLESHIP_DOMAIN=battleship.example.com > .env
docker compose up -d
```

`scripts/web-e2e.sh` plays the opening of an online game in two headless Chromes
against a running service, as CI does against the Docker image.

## How to play

1. **Deploy your fleet.** Your ships start at random. Press **Random** (or Space) to
   shuffle them, or arrange them by hand:
   * pick a ship in the list, or click a placed ship to pick it up,
   * press **R** or right-click to rotate,
   * click the board to drop the ship. The preview is green when the spot is legal.

   Ships may not overlap or touch, not even diagonally.
2. Press **Ready for battle!** (or Enter).
3. When the enemy board pulses it is your turn. Click a square in *Enemy waters* to
   fire. Each shot gets one of three answers: miss (splash), hit (fire), or sunk. When
   a ship sinks, the squares around it are marked as water, because ships never
   touch.
4. Sink all five ships (Carrier 5, Battleship 4, Cruiser 3, Submarine 3, Destroyer 2)
   to win. At the end both fleets are revealed. Choose **Play again** for another
   round. The other player opens the next one.

Press **M** at any time to toggle sound.

### Playing online

* **Host**: choose *Host Online Game*, keep port 7777 or pick another, and press
  *Start hosting*. The screen shows your local address(es) with a *Copy* button.
* **Join**: choose *Join Online Game* and enter the host's address, either `IP` or
  `IP:port` (the default port is 7777). Ctrl+V pastes.

On the same network (LAN or VPN), share the address shown on screen. Over the
internet, the host must forward the TCP port (7777 by default) on their router to
their computer and share their public IP. Allow the game through the firewall when
the OS asks. The host fires first in round one.

In the browser, *Host Online Game* opens a room on the web service and shows its
four-character code. Your opponent opens the same page, chooses *Join Online Game* and
enters the code. Desktop and browser players can't play each other.

## Design

The code is a Cargo workspace of three crates:

| Crate                       | Contents |
|-----------------------------|----------|
| `crates/core` (`battleship-core`)     | All game rules and both wire protocols. No UI, no I/O and no platform code, so every other crate shares it. Fully unit tested. |
| `crates/game` (`battleship`)          | The macroquad game, for the desktop and the browser. `main.rs` is a thin shell that draws state and forwards clicks. Online play is `online_lan.rs` (TCP) on the desktop and `online_web.rs` (relay rooms) in the browser, chosen at compile time. |
| `crates/server` (`battleship-server`) | The web service: serves the browser build and relays games between browsers. |

`web/` holds the page and the JavaScript side of the browser's WebSocket, and the
`Dockerfile` builds the browser game and the service into one image, which `deploy/`
runs behind Caddy for HTTPS.

| Module        | Responsibility |
|---------------|----------------|
| `domain`      | `Coord`, `ShipKind`, `Placement`, `Board`: placement rules (in bounds, no overlap, no contact), firing, sinking, random fleet. |
| `grid`        | `TargetGrid`: the shooter's knowledge of enemy waters (miss, hit, sunk, deduced water). |
| `ai`          | The computer's targeting. *Target mode* finishes a damaged ship along its line. *Hunt mode* fires where the most legal positions of the remaining ships overlap. It averages under 60 shots, versus about 95 for random fire. |
| `session`     | One player's state machine: placement, waiting, my turn, awaiting result, their turn, won/lost. It also handles rematches and READY messages that arrive early. |
| `protocol`    | The line-based wire format (`FIRE 3 7`, `RESULT SUNK Cruiser 3,7 4,7 5,7`, ...). |
| `opponent`    | The `Opponent` trait. `ComputerOpponent` speaks the same protocol as a network peer, so the UI has one code path for every kind of opponent. |
| `game`        | One match as the UI plays it: relays moves, paces shots for the animations and keeps score. |
| `relay`       | The relay server's lobby protocol (`HOST`, `JOIN K7QD`, `ROOM K7QD`, `PAIRED`) and room codes. |
| `relay_link`  | The browser's `Opponent`: runs the lobby and the handshake over any message socket, then relays the game protocol. |
| `net`         | (game crate, desktop only) TCP host and join, with a version handshake (`HELLO BATTLESHIP 2`), background reader threads, disconnect detection and address parsing. |
| `setup`       | `FleetEditor`: interactive fleet placement (select, rotate, drop, pick up). |
| `sound`       | Sound effects synthesised into in-memory WAV files at start-up, so the game needs no asset files. |
| `rng`         | A seedable SplitMix64 generator, so tests are deterministic. |

Neither side ever learns the other's layout until the game ends. Each player only
reports the result of the shots fired at them.

### Test-driven development

Each module was written test first. The specification (tests plus `todo!()` stubs)
was written and run red, then the implementation turned it green. The tests caught
real bugs along the way. One example: a READY from the previous round carried over
into the next, which the rematch tests exposed. The suite covers:

* placement rules, including diagonal contact, and random fleets checked over 300 seeds,
* firing, sinking and game end on both sides,
* the turn state machine, including out-of-turn and duplicate shots, inconsistent reports
  and rematch timing,
* AI behaviour (follows lines, never repeats a shot, always wins within 100 shots, and
  beats random fire by a wide margin),
* protocol round trips and rejection of malformed input,
* real TCP sockets on localhost: handshake, message exchange, disconnects, version
  mismatch, strangers on the port, and freeing the port,
* the relay server over real WebSockets: rooms, pairing, relaying, players leaving,
  and the browser's `RelayLink` playing through it,
* WAV encoding, checked with the same decoder the audio backend uses.

### Mutation testing

[cargo-mutants](https://mutants.rs/) checks that the tests actually catch bugs. It
makes small changes to the library, such as turning `<` into `<=` or `+` into `-`,
and runs the tests against each one. A mutant the tests still pass on points to
behaviour nothing checks.

```sh
cargo install --locked cargo-mutants
cargo mutants                              # every mutant (several minutes)
cargo mutants -f crates/core/src/ai.rs     # one file
git diff origin/main... > pr.diff && cargo mutants --in-diff pr.diff   # only your changes
```

The results land in `mutants.out/`: `missed.txt` lists the mutants no test caught.
Every mutant that compiles should be caught, or make the tests hang until the timeout,
which also counts as caught.

`.cargo/mutants.toml` skips the rendering shell, the browser-only socket and the
server's `main.rs`, and lists the few mutants no test can catch,
each with its reason. Some are equivalent (they behave exactly like the original),
some depend on the machine (network routes, the clock), and some only change how a
sound effect sounds.

## Credits

The font is DejaVu Sans Bold (see `crates/game/assets/DejaVu-LICENSE.txt`). The lab is from
*Essential Test-Driven Development* by Rob Myers.

## Releases

CI (`.github/workflows/ci.yml`) checks formatting, runs clippy and the tests on every
push and pull request, builds the browser version, and builds the Docker image and
plays an online game against it in two headless browsers.
`.github/workflows/mutants.yml` runs mutation testing on the lines each pull request
changes and fails if a mutant survives. It also runs against the whole workspace every
Monday and on demand.

Releases follow [semantic versioning](https://semver.org). To publish one, run the
release script from an up-to-date `main`:

```sh
scripts/release.sh minor              # or patch, major, or a version such as 1.0.0-rc.1
scripts/release.sh minor --dry-run    # only run the checks
```

It runs the same checks as CI, shows the changes since the last release and asks for
confirmation. It then bumps `version` in `Cargo.toml`, commits, and pushes the commit
together with a matching `v` tag. To release by hand instead, do the same: bump the
version, commit, and push a tag such as `v0.2.0`.

`.github/workflows/release.yml` checks that the tag matches `Cargo.toml` and runs the
tests. It then builds the binary for Linux x86_64, Windows x86_64 and macOS arm64 and
attaches them to a new GitHub release, and publishes the web version's Docker image to
`ghcr.io/grenadecx/battleship`. Tags with a suffix such as `v1.0.0-rc.1` are
published as pre-releases.
