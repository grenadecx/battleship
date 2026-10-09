//! The audio backend links against `libasound` (ALSA) on Linux. Distros only
//! ship the unversioned `libasound.so` linker symlink in the `-dev` package, so
//! when it is missing we point the linker at the runtime `libasound.so.2`.

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        ensure_asound_linkable();
    }
}

fn ensure_asound_linkable() {
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let dirs = [
        format!("/usr/lib/{arch}-linux-gnu"),
        format!("/lib/{arch}-linux-gnu"),
        "/usr/lib64".to_string(),
        "/usr/lib".to_string(),
        "/lib64".to_string(),
        "/lib".to_string(),
    ];
    let dirs: Vec<&Path> = dirs.iter().map(Path::new).collect();

    if dirs.iter().any(|d| d.join("libasound.so").exists()) {
        return;
    }
    let Some(runtime_lib) = dirs
        .iter()
        .map(|d| d.join("libasound.so.2"))
        .find(|p| p.exists())
    else {
        println!(
            "cargo:warning=libasound not found; install your distro's ALSA package (e.g. libasound2-dev)"
        );
        return;
    };

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is set by cargo"));
    let link = out_dir.join("libasound.so");
    let _ = std::fs::remove_file(&link);
    #[cfg(unix)]
    std::os::unix::fs::symlink(&runtime_lib, &link).expect("create libasound.so symlink");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
}
