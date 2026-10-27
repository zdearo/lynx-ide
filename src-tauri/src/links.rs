//! Opens a link in the user's browser. Used by the pull request views and by the bots' replies, so
//! the link rules live in one place.

use crate::error::{AppError, IpcError};
use crate::process::blocking_task::run_blocking;

/// Opens a link in the browser. Only `https` links are opened.
#[tauri::command]
#[specta::specta]
pub async fn open_link(url: String) -> Result<(), IpcError> {
    run_blocking(move || {
        if !is_https_link(&url) {
            return Err(AppError::InvalidLink);
        }
        open::that(&url).map_err(AppError::OpenLink)
    })
    .await
}

/// A link without spaces or control characters, starting with `https://`.
pub fn is_https_link(url: &str) -> bool {
    url.starts_with("https://")
        && url.len() > "https://".len()
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_links_without_spaces_are_opened() {
        assert!(is_https_link("https://github.com/acme/app/pull/42"));
        assert!(!is_https_link("http://github.com/acme/app/pull/42"));
        assert!(!is_https_link("file:///etc/passwd"));
        assert!(!is_https_link("https://"));
        assert!(!is_https_link("https://github.com/a b"));
    }
}
