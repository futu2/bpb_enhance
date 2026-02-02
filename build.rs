fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    // Rebuild if the icon changes.
    println!("cargo:rerun-if-changed=icon.ico");

    let mut res = winres::WindowsResource::new();
    res.set_icon("icon.ico");
    res.compile().unwrap();
}
