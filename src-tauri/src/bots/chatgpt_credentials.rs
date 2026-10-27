//! The ChatGPT sign-in kept between runs. The tokens are secrets, so the file is private to its
//! owner and stays out of `config.json`.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::private_file;

/// The client OpenAI issued, the host this install reports as, and the session if one is open.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    pub client_id: String,
    pub host_id: String,
    pub session: Option<Session>,
}

/// One signed-in account and its tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub subject: String,
    pub email: Option<String>,
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds when the access token stops working.
    pub expires_at: i64,
    pub scope: String,
}

/// None when the file is missing or broken, so a broken file starts the sign-in over.
pub fn read(file: &Path) -> Option<Credentials> {
    serde_json::from_str(&fs::read_to_string(file).ok()?).ok()
}

pub fn write(file: &Path, credentials: &Credentials) -> io::Result<()> {
    private_file::write(file, serde_json::to_string(credentials)?.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> Credentials {
        Credentials {
            client_id: "oaiapp_test".to_owned(),
            host_id: "urn:uuid:test".to_owned(),
            session: Some(Session {
                subject: "user-1".to_owned(),
                email: Some("dev@example.com".to_owned()),
                id_token: "id".to_owned(),
                access_token: "access".to_owned(),
                refresh_token: "refresh".to_owned(),
                expires_at: 1_800_003_600,
                scope: "chatgpt.tokens.use.direct".to_owned(),
            }),
        }
    }

    #[test]
    fn saved_credentials_read_back() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("bots").join("chatgpt.json");

        write(&file, &credentials()).expect("write");

        assert_eq!(read(&file), Some(credentials()));
    }

    #[test]
    fn a_missing_or_broken_file_reads_as_nothing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("chatgpt.json");
        assert_eq!(read(&file), None);

        fs::write(&file, "not json").expect("write");
        assert_eq!(read(&file), None);
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_can_read_the_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temp dir");
        let file = dir.path().join("chatgpt.json");

        write(&file, &credentials()).expect("write");

        let mode = fs::metadata(&file).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
