//! Runs the web service: `PORT` (default 8080) and `BATTLESHIP_WEB_DIR`, the
//! output of `scripts/build-web.sh` (default `target/web`).
//! `battleship-server --health-check` asks a running server on `PORT` whether it
//! is up, for Docker's health check in an image without curl.

use std::env;
use std::io::{Read, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let port: u16 = match env::var("PORT") {
        Ok(port) => match port.parse() {
            Ok(port) => port,
            Err(_) => {
                eprintln!("error: PORT must be a port number, not {port:?}");
                return ExitCode::FAILURE;
            }
        },
        Err(_) => 8080,
    };
    if env::args().nth(1).as_deref() == Some("--health-check") {
        return health_check(port);
    }
    let runtime = tokio::runtime::Runtime::new().expect("start the async runtime");
    match runtime.block_on(serve(port)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// Exits successfully when the server on `port` answers its health check.
fn health_check(port: u16) -> ExitCode {
    let healthy = (|| {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).ok()?;
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .ok()?;
        stream
            .write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\n\r\n")
            .ok()?;
        let mut response = String::new();
        stream.read_to_string(&mut response).ok()?;
        // The status line answers in the request's HTTP version: "HTTP/1.0 200 OK".
        (response.split_whitespace().nth(1) == Some("200")).then_some(())
    })();
    if healthy.is_some() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

async fn serve(port: u16) -> std::io::Result<()> {
    let web_dir = env::var("BATTLESHIP_WEB_DIR").unwrap_or_else(|_| "target/web".into());
    if !std::path::Path::new(&web_dir).join("index.html").exists() {
        eprintln!("warning: no index.html in {web_dir}; run scripts/build-web.sh first");
    }

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    println!("Battleship on http://localhost:{port}/ serving {web_dir}");
    let app = battleship_server::app(&web_dir);
    tokio::select! {
        served = axum::serve(listener, app) => served,
        () = shutdown_requested() => Ok(()),
    }
}

/// Ctrl+C, or SIGTERM from `docker stop`. Open games end with the process.
async fn shutdown_requested() {
    let interrupt = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(_) => std::future::pending().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        () = interrupt => {}
        () = terminate => {}
    }
}
