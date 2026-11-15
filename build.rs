// Встраивает иконку, манифест и версию в retypr.exe (только при сборке на Windows).
// Нужен Windows SDK (rc.exe) — ставится вместе с Visual Studio Build Tools. Если его нет,
// сборка не падает: exe просто будет без иконки (предупреждение появится в выводе cargo).

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
    res.set("LegalCopyright", "Retypr");
    if let Err(e) = res.compile() {
        println!("cargo:warning=не удалось встроить ресурсы (иконка/манифест): {e}");
    }
}

#[cfg(not(windows))]
fn main() {}
