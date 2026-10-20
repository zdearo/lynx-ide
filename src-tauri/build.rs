use std::{env, fs, path::PathBuf};

/// Common Controls v6, which the dialog and window code on Windows needs.
const COMMON_CONTROLS_V6: &str = r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"#;

fn main() {
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        embed_common_controls();
        // Tauri's own copy of this manifest is a resource, which only the app binary gets. The link
        // argument reaches every target, so keeping the resource too would embed the manifest twice.
        let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
        tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
            .expect("tauri-build failed");
    } else {
        tauri_build::build();
    }
}

/// Links the manifest into every target of the package. The unit-test harness is a separate
/// executable, and without the manifest it loads Common Controls v5 and exits with
/// STATUS_ENTRYPOINT_NOT_FOUND before any test runs. Tauri does the same for its own tests.
fn embed_common_controls() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let manifest = out_dir.join("common-controls.manifest");
    fs::write(&manifest, COMMON_CONTROLS_V6).expect("write the manifest");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
