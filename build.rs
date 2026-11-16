// Embeds the icon, manifest and version into Retypr.exe (only when building on Windows).
// Needs the Windows SDK (rc.exe), installed with the Visual Studio Build Tools. If it is missing,
// the build does not fail: the exe just has no icon (cargo prints a warning).

#[cfg(windows)]
fn main() {
    println!("cargo:rerun-if-changed=assets/retypr.ico");
    println!("cargo:rerun-if-changed=assets/retypr.manifest");
    println!("cargo:rerun-if-changed=build.rs");

    let mut res = winresource::WindowsResource::new();
    res.set_icon("assets/retypr.ico");
    res.set_manifest_file("assets/retypr.manifest");
    res.set("ProductName", "Retypr");
    res.set("FileDescription", "Retypr — keyboard layout switcher");
    res.set("OriginalFilename", "Retypr.exe");
    res.set("LegalCopyright", "Retypr");
    if let Err(e) = res.compile() {
        println!("cargo:warning=could not embed resources (icon/manifest): {e}");
    }
}

#[cfg(not(windows))]
fn main() {}
