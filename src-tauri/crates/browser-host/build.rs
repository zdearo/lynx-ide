fn main() {
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    match os.as_str() {
        // libcef.so sits next to the host in the engine folder.
        "linux" => println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN"),
        // Chromium wants a manifest: the Windows versions it supports, per-monitor DPI, UTF-8.
        "windows" => {
            use embed_manifest::manifest::{ActiveCodePage, DpiAwareness};
            embed_manifest::embed_manifest(
                embed_manifest::new_manifest("Lynx.BrowserHost")
                    .active_code_page(ActiveCodePage::Utf8)
                    .dpi_awareness(DpiAwareness::PerMonitorV2),
            )
            .expect("could not embed the application manifest");
        }
        _ => {}
    }
    println!("cargo:rerun-if-changed=build.rs");
}
