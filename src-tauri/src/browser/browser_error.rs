use std::path::PathBuf;

use lynx_browser_protocol::link::LinkError;

#[derive(Debug, thiserror::Error)]
pub enum BrowserError {
    #[error("the browser engine is not installed (expected {0}); run `bun run browser:engine`")]
    EngineMissing(PathBuf),
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
    #[error("the browser is only available on Linux for now")]
    Unsupported,
    #[error(transparent)]
    Link(#[from] LinkError),
}
