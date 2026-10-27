fn main() {
    // Exe icon (Explorer, taskbar, Alt+Tab). On the GNU toolchain this needs windres on PATH (WinLibs).
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/icon.ico");
    embed_resource::compile("assets/app.rc", embed_resource::NONE).manifest_optional().unwrap();
}
