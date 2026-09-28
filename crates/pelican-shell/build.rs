fn main() {
    // The frontend is checked-in static files, not a build product, but the
    // codegen inlines them into the binary, so a CSS edit has to invalidate
    // the crate or `cargo run` serves a stale window.
    println!("cargo:rerun-if-changed=../../ui");

    // The app manifest is the point of this build script. Without it our own
    // commands are not ACL-gated at all and `capabilities/main.json` governs
    // nothing. With it, that file is the real boundary: a command missing
    // from the capability cannot be invoked from the webview.
    //
    // Keep this list, `generate_handler!` in `main.rs` and the capability in
    // step; `scripts/check.sh` fails when they drift.
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(
        tauri_build::AppManifest::new().commands(&[
            "status",
            "name_watch",
            "library_list",
            "places",
            "preview",
            "push",
            "stop",
            "watch_list",
            "ledger",
            "default_backup_dir",
            "backup_watch",
            "reset_check",
            "reset_ledger",
            "udev_rule_status",
            "install_udev_rule",
        ]),
    ))
    .expect("tauri build failed");
}
