//! What a published browser engine says about itself: which version it is, which protocol it
//! speaks, and where its archive is and how to check it. The script that builds the engine writes
//! this file next to the archive (`bun run browser:engine -- --bundle`), and the app reads it
//! before downloading anything.

use serde::Deserialize;
use url::Url;

use super::engine_install_error::EngineInstallError;

/// Where the manifests of the latest engine are published, one per platform.
/// `LYNX_ENGINE_MANIFEST_URL` replaces the address, for a mirror or a build under test.
const RELEASE_DOWNLOADS: &str = "https://github.com/zdearo/lynx-ide/releases/latest/download";
pub const MANIFEST_URL_ENV: &str = "LYNX_ENGINE_MANIFEST_URL";

/// The platform this build can run an engine on, as the manifest names it.
pub const PLATFORM: &str = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
    "linux-x64"
} else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
    "windows-x64"
} else {
    "unsupported"
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineManifest {
    /// What identifies the build: the CEF version and the protocol, such as `154.0.34-p2`.
    pub version: String,
    pub protocol: u32,
    pub platform: String,
    pub cef: String,
    /// The archive's address, absolute or relative to the manifest's own.
    pub archive: String,
    pub sha256: String,
    /// The archive's size in bytes.
    pub size: u64,
}

pub fn manifest_url() -> String {
    std::env::var(MANIFEST_URL_ENV)
        .unwrap_or_else(|_| format!("{RELEASE_DOWNLOADS}/browser-engine-manifest-{PLATFORM}.json"))
}

impl EngineManifest {
    /// Refuses a manifest this app cannot use or should not trust the shape of: another platform,
    /// another protocol, or values that would be unsafe to turn into file names.
    pub fn check(&self, protocol: u32) -> Result<(), EngineInstallError> {
        if self.platform != PLATFORM {
            return Err(EngineInstallError::WrongPlatform {
                engine: self.platform.clone(),
                here: PLATFORM.to_owned(),
            });
        }
        if self.protocol != protocol {
            return Err(EngineInstallError::ProtocolMismatch {
                engine: self.protocol,
                app: protocol,
            });
        }
        let name_ok = !self.version.is_empty()
            && self
                .version
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
        let sha_ok = self.sha256.len() == 64 && self.sha256.chars().all(|c| c.is_ascii_hexdigit());
        if !name_ok || !sha_ok || self.size == 0 {
            return Err(EngineInstallError::BadManifest(
                "the version, checksum or size is malformed".to_owned(),
            ));
        }
        Ok(())
    }

    /// The archive's absolute address.
    pub fn archive_url(&self, manifest_url: &str) -> Result<String, EngineInstallError> {
        let base = Url::parse(manifest_url)
            .map_err(|error| EngineInstallError::BadManifest(error.to_string()))?;
        let url = base
            .join(&self.archive)
            .map_err(|error| EngineInstallError::BadManifest(error.to_string()))?;
        Ok(url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> EngineManifest {
        EngineManifest {
            version: "154.0.34-p2".to_owned(),
            protocol: 2,
            platform: PLATFORM.to_owned(),
            cef: "154.0.34".to_owned(),
            archive: "engine.tar.gz".to_owned(),
            sha256: "a".repeat(64),
            size: 10,
        }
    }

    #[test]
    fn a_manifest_is_read_from_the_json_the_script_writes() {
        let json = r#"{"version":"154.0.34-p2","protocol":2,"platform":"linux-x64","cef":"154.0.34","archive":"a.tar.gz","sha256":"00","size":5}"#;
        let parsed: EngineManifest = serde_json::from_str(json).expect("parses");
        assert_eq!(parsed.version, "154.0.34-p2");
        assert_eq!(parsed.archive, "a.tar.gz");
    }

    #[test]
    fn a_good_manifest_passes() {
        manifest().check(2).expect("accepted");
    }

    #[test]
    fn an_engine_that_speaks_another_protocol_is_refused() {
        let error = manifest().check(3).unwrap_err();
        assert!(matches!(
            error,
            EngineInstallError::ProtocolMismatch { engine: 2, app: 3 }
        ));
    }

    #[test]
    fn another_platform_is_refused() {
        let mut other = manifest();
        other.platform = "windows-x64".to_owned();
        assert!(matches!(
            other.check(2),
            Err(EngineInstallError::WrongPlatform { .. })
        ));
    }

    #[test]
    fn values_that_would_make_unsafe_file_names_are_refused() {
        for version in ["../evil", "a/b", ""] {
            let mut bad = manifest();
            bad.version = version.to_owned();
            assert!(
                matches!(bad.check(2), Err(EngineInstallError::BadManifest(_))),
                "{version}"
            );
        }
        let mut short = manifest();
        short.sha256 = "abc".to_owned();
        assert!(short.check(2).is_err());
    }

    #[test]
    fn the_archive_address_is_relative_to_the_manifests() {
        let url = manifest()
            .archive_url("https://example.test/releases/m.json")
            .expect("joins");
        assert_eq!(url, "https://example.test/releases/engine.tar.gz");

        let mut absolute = manifest();
        absolute.archive = "https://cdn.test/e.tar.gz".to_owned();
        assert_eq!(
            absolute.archive_url("https://example.test/m.json").unwrap(),
            "https://cdn.test/e.tar.gz"
        );
    }
}
