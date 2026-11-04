fn main() {
    build_voice_helper();
    tauri_build::build()
}

/// Compiles the native speech helper (Swift, macOS only) into
/// `binaries/forge-voice-<target>`, where Tauri picks it up as a sidecar.
fn build_voice_helper() {
    println!("cargo:rerun-if-changed=voice/main.swift");
    let target = std::env::var("TARGET").unwrap_or_default();
    println!("cargo:rustc-env=FORGE_TARGET={target}");
    if !target.contains("apple-darwin") {
        return;
    }
    let out_dir = std::path::Path::new("binaries");
    std::fs::create_dir_all(out_dir).expect("binaries dir");
    let out = out_dir.join(format!("forge-voice-{target}"));
    let arch = if target.starts_with("aarch64") { "arm64" } else { "x86_64" };
    let status = std::process::Command::new("swiftc")
        .args(["-O", "-swift-version", "5", "-target", &format!("{arch}-apple-macos12.0"), "-o"])
        .arg(&out)
        .arg("voice/main.swift")
        .status();
    match status {
        Ok(s) if s.success() => {}
        other => panic!(
            "não foi possível compilar o helper de voz (voice/main.swift) com swiftc: {other:?}. Instale as Command Line Tools do Xcode (xcode-select --install)."
        ),
    }
}
