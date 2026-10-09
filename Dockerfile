# The web version of Battleship: the browser build of the game and the relay
# server, both compiled from the same workspace as the desktop app.
#
#   docker build -t battleship .
#   docker run --rm -p 8080:8080 battleship     # then open http://localhost:8080

# Same toolchain as CI.
FROM rust:1.99-slim-trixie AS build
RUN rustup target add wasm32-unknown-unknown
WORKDIR /src
COPY . .
# The cache mounts keep downloaded crates and compiled dependencies between builds;
# the results are copied out of them into /out.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    scripts/build-web.sh /out/web \
    && cargo build --release --locked -p battleship-server \
    && cp target/release/battleship-server /out/

FROM debian:trixie-slim
COPY --from=build /out/battleship-server /usr/local/bin/battleship-server
COPY --from=build /out/web /srv/battleship
ENV PORT=8080 \
    BATTLESHIP_WEB_DIR=/srv/battleship
EXPOSE 8080
USER nobody
HEALTHCHECK --interval=30s --timeout=3s \
    CMD ["battleship-server", "--health-check"]
CMD ["battleship-server"]
