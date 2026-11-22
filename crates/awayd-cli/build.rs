//! Build script — embeds the Windows icon and version information into `awayctl.exe`.

fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let icon = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/icons/awayd.ico");
    println!("cargo:rerun-if-changed={icon}");
    let mut res = winresource::WindowsResource::new();
    res.set_icon(icon)
        .set("ProductName", "awayd")
        .set("FileDescription", "awayd command-line client")
        .set("OriginalFilename", "awayctl.exe")
        .set(
            "LegalCopyright",
            "Copyright (C) 2026 awayd contributors. GPL-3.0-or-later.",
        );
    res.compile()
        .expect("embed Windows resources (icon, version info)");
}
