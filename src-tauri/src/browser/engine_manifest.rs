//! What a published browser engine says about itself: which version it is, which protocol it
//! speaks, and where its archive is and how to check it. The script that builds the engine writes
//! this file next to the archive (`bun run browser:engine -- --bundle`), and the app reads it
//! before downloading anything.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;
use url::Url;

use super::engine_install_error::EngineInstallError;

/// Where the manifests of the latest engine are published, one per platform, next to the archives.
/// `LYNX_ENGINE_MANIFEST_URL` replaces the address, for a mirror or a build under test.
const ENGINE_DOWNLOADS: &str = "https://downloads.zdearo.dev/engine";
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

/// The updater's config, which holds the public half of the key that signs the app's updates. The
/// same key signs the engine manifest, so the app has one key to trust and nothing new to ship.
const APP_CONFIG: &str = include_str!("../../tauri.conf.json");

/// The key a published manifest must be signed with.
pub fn release_key() -> Result<PublicKey, EngineInstallError> {
    let config: serde_json::Value = serde_json::from_str(APP_CONFIG)
        .map_err(|error| EngineInstallError::BadSignature(error.to_string()))?;
    let encoded = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or_else(|| EngineInstallError::BadSignature("the app has no public key".to_owned()))?;
    public_key(encoded)
}

/// A public key as `tauri signer generate` writes it: the minisign key file, base64 encoded.
fn public_key(encoded: &str) -> Result<PublicKey, EngineInstallError> {
    let text = decode_text(encoded, "the public key")?;
    PublicKey::decode(&text).map_err(|error| EngineInstallError::BadSignature(error.to_string()))
}

fn decode_text(encoded: &str, what: &str) -> Result<String, EngineInstallError> {
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|_| EngineInstallError::BadSignature(format!("{what} is not base64")))?;
    String::from_utf8(bytes)
        .map_err(|_| EngineInstallError::BadSignature(format!("{what} is not text")))
}

/// Checks that `manifest` is exactly what the holder of `key` signed. `signature` is the `.sig` file
/// `tauri signer sign` writes: the minisign signature, base64 encoded.
pub fn verify_signature(
    key: &PublicKey,
    manifest: &str,
    signature: &str,
) -> Result<(), EngineInstallError> {
    let text = decode_text(signature, "the signature")?;
    let signature = Signature::decode(&text)
        .map_err(|error| EngineInstallError::BadSignature(error.to_string()))?;
    key.verify(manifest.as_bytes(), &signature, false)
        .map_err(|error| EngineInstallError::BadSignature(error.to_string()))
}

pub fn manifest_url() -> String {
    std::env::var(MANIFEST_URL_ENV).unwrap_or_else(|_| default_manifest_url(PLATFORM))
}

fn default_manifest_url(platform: &str) -> String {
    format!("{ENGINE_DOWNLOADS}/browser-engine-manifest-{platform}.json")
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
        // Not the platform this test runs on: `windows-x64` is that on Windows.
        let mut other = manifest();
        other.platform = "darwin-arm64".to_owned();
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
    fn the_default_manifest_is_published_next_to_the_engine_archives() {
        assert_eq!(
            default_manifest_url("linux-x64"),
            "https://downloads.zdearo.dev/engine/browser-engine-manifest-linux-x64.json"
        );
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

    // A throwaway key made for these tests with `tauri signer generate`; its private half was thrown away.
    const TEST_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEFDNTIwOTA3Njk5Q0E5ODMKUldTRHFaeHBCd2xTckVVbkprbHcyRUswcWdNWUp2dlYzNUFzeDZRRytPbmxVTDcxWjc4dGN4YnUK";
    const SIGNED_MANIFEST: &str = "{\"version\":\"154.0.34-p2\",\"protocol\":2}\n";
    const SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTRHFaeHBCd2xTckFuTjNlNXdaRzdEZW9XSGg5TUFwRFZYMFlXUmZ0U0VOVFRCZ1p3U1NyUmZoK2NLeGNseEJZc2pWZndZNWNMa1BjVFNKN1hmcnZzZ1dvaDdvWkdObndBPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzk0NTAyODQ4CWZpbGU6bWFuaWZlc3QuanNvbgppM2JWcVh5bk5RSzV1bXNUS3dsY0VFVDFmR0dlM1BnajdHYkpmVkNQQXpadEdiNmo2dlFjUkU2R1ZOZFcrUzd1RGkyMDh6OHZwRUFOa1FxanhnbDFDUT09Cg==";

    #[test]
    fn the_manifest_its_owner_signed_is_accepted() {
        let key = public_key(TEST_KEY).expect("key");
        verify_signature(&key, SIGNED_MANIFEST, SIGNATURE).expect("verified");
    }

    #[test]
    fn a_manifest_changed_after_signing_is_refused() {
        let key = public_key(TEST_KEY).expect("key");
        let changed = SIGNED_MANIFEST.replace("154.0.34", "999.0.0");
        let error = verify_signature(&key, &changed, SIGNATURE).unwrap_err();
        assert!(matches!(error, EngineInstallError::BadSignature(_)));
    }

    #[test]
    fn a_signature_from_another_key_is_refused() {
        // The app's own key did not make this signature.
        let key = release_key().expect("the app's key");
        let error = verify_signature(&key, SIGNED_MANIFEST, SIGNATURE).unwrap_err();
        assert!(matches!(error, EngineInstallError::BadSignature(_)));
    }

    #[test]
    fn a_signature_that_is_not_a_signature_is_refused() {
        let key = public_key(TEST_KEY).expect("key");
        for junk in ["", "not base64 !", "aGVsbG8="] {
            let error = verify_signature(&key, SIGNED_MANIFEST, junk).unwrap_err();
            assert!(
                matches!(error, EngineInstallError::BadSignature(_)),
                "{junk}"
            );
        }
    }

    #[test]
    fn the_app_carries_a_usable_key() {
        release_key().expect("the updater key in tauri.conf.json parses");
    }
}
