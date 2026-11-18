// Embeds the Poro icon (resource ID 1, which GPUI uses for the window and
// taskbar) and the app's name in the exe.
fn main() {
    println!("cargo:rerun-if-changed=assets/poro.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon_with_id("assets/poro.ico", "1");
        res.set("FileDescription", "Poro");
        res.set("ProductName", "Poro");
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed the icon: {e}");
        }
    }
}
