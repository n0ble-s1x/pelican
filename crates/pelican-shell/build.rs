fn main() {
    // The frontend is checked-in static files, not a build product — but the
    // codegen inlines them into the binary, so a CSS edit has to invalidate
    // the crate or `cargo run` serves a stale window.
    println!("cargo:rerun-if-changed=../../ui");

    // The app manifest is the whole point of this build script. Without it,
    // our own commands are not ACL-gated at all and `capabilities/main.json`
    // governs nothing. With it, that file is the real boundary: a command
    // missing from the capability cannot be invoked from the webview.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "subscribe",
            "connect",
            "disconnect",
            "delete_remote",
            "forget_uploads",
            "pick_folder",
            "scan_folder",
            "start_sync",
            "stop_sync",
            "set_now_playing",
            "cover_art",
        ]),
    ))
    .expect("tauri build failed");
}
