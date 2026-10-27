//! Reads an image of a worktree for the viewer. Only formats a webview draws by itself; SVG stays
//! text, so it is edited like any other source file.

use std::fs;
use std::path::Path;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;
use specta::Type;

use super::files_error::FilesError;
use super::path_inside_worktree::resolve_inside;

/// The largest image opened; it crosses the IPC as base64, about a third larger.
pub const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FileImage {
    pub mime: String,
    /// The file's bytes, base64.
    pub data: String,
}

fn mime_of(relative: &str) -> Option<&'static str> {
    let extension = Path::new(relative)
        .extension()?
        .to_str()?
        .to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        _ => return None,
    })
}

pub fn read_image(root: &Path, relative: &str) -> Result<FileImage, FilesError> {
    let mime = mime_of(relative).ok_or_else(|| FilesError::Binary(relative.to_owned()))?;
    let target = resolve_inside(root, relative)?;
    let io_error = |source| FilesError::Io {
        path: relative.to_owned(),
        source,
    };
    let metadata = fs::metadata(&target).map_err(io_error)?;
    if !metadata.is_file() {
        return Err(FilesError::NotAFile(relative.to_owned()));
    }
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err(FilesError::TooLarge {
            path: relative.to_owned(),
            limit: MAX_IMAGE_BYTES,
        });
    }
    let bytes = fs::read(&target).map_err(io_error)?;
    Ok(FileImage {
        mime: mime.to_owned(),
        data: STANDARD.encode(bytes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_image_as_base64_with_its_type() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("A.PNG"), [1u8, 2, 3]).expect("write");
        let image = read_image(dir.path(), "A.PNG").expect("read");
        assert_eq!(image.mime, "image/png");
        assert_eq!(image.data, "AQID");
    }

    #[test]
    fn refuses_other_extensions_folders_and_escapes() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(dir.path().join("logo.svg"), "<svg/>").expect("write");
        fs::create_dir(dir.path().join("dir.png")).expect("mkdir");
        assert!(matches!(
            read_image(dir.path(), "logo.svg"),
            Err(FilesError::Binary(_))
        ));
        assert!(matches!(
            read_image(dir.path(), "dir.png"),
            Err(FilesError::NotAFile(_))
        ));
        assert!(matches!(
            read_image(dir.path(), "../x.png"),
            Err(FilesError::InvalidPath(_))
        ));
    }

    #[test]
    fn refuses_an_image_over_the_limit() {
        let dir = tempfile::tempdir().expect("temp dir");
        let big = vec![0u8; usize::try_from(MAX_IMAGE_BYTES).expect("fits") + 1];
        fs::write(dir.path().join("big.png"), big).expect("write");
        assert!(matches!(
            read_image(dir.path(), "big.png"),
            Err(FilesError::TooLarge { .. })
        ));
    }
}
