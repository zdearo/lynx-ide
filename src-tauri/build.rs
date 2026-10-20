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
    tauri_build::build();
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_test_manifest();
    }
}

/// Tauri embeds the manifest only in the app binary. The unit-test harness is another executable
/// without it, so it loads Common Controls v5 and exits with STATUS_ENTRYPOINT_NOT_FOUND.
fn embed_test_manifest() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR"));
    let manifest = out_dir.join("common-controls.manifest");
    let resource = out_dir.join("common-controls.rc");
    fs::write(&manifest, COMMON_CONTROLS_V6).expect("write the test manifest");
    // Resource 1 of type RT_MANIFEST (24). Forward slashes keep the quoted path valid in an .rc file.
    let manifest_path = manifest.display().to_string().replace('\\', "/");
    fs::write(&resource, format!("1 24 \"{manifest_path}\"\n")).expect("write the test resource");
    embed_resource::compile_for_tests(&resource, embed_resource::NONE)
        .manifest_required()
        .expect("embed the test manifest");
}
