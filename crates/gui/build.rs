//! Embed the icon and version information into the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("ProductName", "MouseForge")
            .set(
                "FileDescription",
                "MouseForge - onboard mouse profile editor",
            )
            .set("InternalName", "mouseforge")
            .set("OriginalFilename", "mouseforge.exe")
            .set("LegalCopyright", "MIT License");
        res.compile()
            .expect("failed to compile Windows resources (is the Windows SDK installed?)");
    }
}
