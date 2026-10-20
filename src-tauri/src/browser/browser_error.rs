use std::path::PathBuf;

use lynx_browser_protocol::link::LinkError;

#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error(
        "the browser engine is not installed (expected {0}); install it in Lynx's settings, under Browser"
    )]
    EngineMissing(PathBuf),
    #[error(
        "the browser engine at {0} is out of date for this Lynx; update it in Lynx's settings, under Browser"
    )]
    EngineOutdated(PathBuf),
    #[error("enter an http or https address")]
    InvalidAddress,
    #[error("the browser is not running")]
    NotRunning,
    #[error("the browser engine failed: {0}")]
    EngineFailed(String),
    #[error("could not open the browser window: {0}")]
    Window(String),
    #[error("the page did not answer in time")]
    PageTimeout,
    #[error("the page refused the call: {0}")]
    Page(String),
    #[error("the browser is only available on Linux and Windows for now")]
    Unsupported,
    #[error(transparent)]
    Link(#[from] LinkError),
    #[error(transparent)]
    EngineInstall(#[from] super::engine_install_error::EngineInstallError),
}
