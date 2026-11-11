//! Builds the Swift backend (`backend/*.swift`) into a static library and links it, with the
//! Swift runtime macOS ships in /usr/lib/swift.

use std::path::PathBuf;
use std::process::Command;

fn out(cmd: &mut Command) -> String {
    let o = cmd.output().expect("run xcrun");
    assert!(
        o.status.success(),
        "{:?} failed: {}",
        cmd,
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).unwrap().trim().to_string()
}

fn main() {
    println!("cargo:rerun-if-changed=backend");
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let mut sources: Vec<PathBuf> = std::fs::read_dir("backend")
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "swift"))
        .collect();
    sources.sort();
    let lib = out_dir.join("libNightjarBackend.a");
    let status = Command::new("xcrun")
        .args([
            "swiftc",
            "-parse-as-library",
            "-emit-library",
            "-static",
            "-O",
            "-swift-version",
            "5",
        ])
        .args([
            "-module-name",
            "NightjarBackend",
            "-target",
            "arm64-apple-macosx14.2",
        ])
        .args(&sources)
        .arg("-o")
        .arg(&lib)
        .status()
        .expect("run swiftc");
    assert!(status.success(), "swiftc failed");

    let sdk = out(Command::new("xcrun").args(["--show-sdk-path"]));
    let swiftc = PathBuf::from(out(Command::new("xcrun").args(["-f", "swiftc"])));
    let toolchain = swiftc.parent().and_then(|p| p.parent()).unwrap();
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-lib=static=NightjarBackend");
    println!("cargo:rustc-link-search=native={}/usr/lib/swift", sdk);
    println!(
        "cargo:rustc-link-search=native={}/lib/swift/macosx",
        toolchain.display()
    );
    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    for fw in [
        "AppKit",
        "WebKit",
        "CoreAudio",
        "AudioToolbox",
        "Accelerate",
        "MediaPlayer",
    ] {
        println!("cargo:rustc-link-lib=framework={fw}");
    }
}
