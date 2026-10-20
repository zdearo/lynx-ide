use std::io;

#[derive(Debug, thiserror::Error)]
pub enum EngineInstallError {
    #[error("could not reach {url}: {reason}")]
    Network { url: String, reason: String },
    #[error("the engine manifest is not usable: {0}")]
    BadManifest(String),
    #[error("this engine is built for {engine}, and this computer is {here}")]
    WrongPlatform { engine: String, here: String },
    #[error(
        "the published engine speaks protocol {engine} and this Lynx speaks {app}; update Lynx to get an engine it can use"
    )]
    ProtocolMismatch { engine: u32, app: u32 },
    #[error("the download does not match its published checksum, so it was thrown away")]
    ChecksumMismatch,
    #[error("the engine archive holds something unsafe ({0}), so nothing was installed")]
    UnsafeArchive(String),
    #[error("the engine archive is not complete: {0}")]
    IncompleteArchive(String),
    #[error("an engine is being installed already")]
    Busy,
    #[error("the browser engine is only available on Linux x64 for now")]
    Unsupported,
    #[error("could not write {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: io::Error,
    },
}

impl EngineInstallError {
    pub fn io(path: &std::path::Path, source: io::Error) -> Self {
        Self::Io {
            path: path.to_string_lossy().into_owned(),
            source,
        }
    }
}
