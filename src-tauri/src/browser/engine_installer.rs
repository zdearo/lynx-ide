//! Downloads the browser engine and puts it in place. The manifest is read first, and refused if it
//! is for another platform or protocol; the archive is checked against the manifest's SHA-256 before
//! a single file is unpacked; it is unpacked beside the old engine and only then does `current`
//! switch over, so a failed install never leaves a half engine behind.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use flate2::read::GzDecoder;
use lynx_browser_protocol::message::PROTOCOL_VERSION;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use specta::Type;
use tar::EntryType;

use super::engine_files::{
    EngineState, HOST_NAME, INFO_FILE, RELEASE_PREFIX, current_dir, engine_root, read_status,
};
use super::engine_install_error::EngineInstallError;
use super::engine_manifest::{EngineManifest, PLATFORM};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// How often progress is reported while downloading.
const REPORT_EVERY: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum EnginePhase {
    Downloading,
    Verifying,
    Unpacking,
    Done,
}

/// How far an install has got, in bytes. `total` is zero while it is not known. (Sizes are `u32`
/// because the interface contract cannot carry 64-bit integers; an engine is far below 4 GB.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EngineProgress {
    pub phase: EnginePhase,
    pub received: u32,
    pub total: u32,
}

/// One install at a time.
#[derive(Default)]
pub struct EngineInstallLock(AtomicBool);

impl EngineInstallLock {
    /// Takes the lock for the guard's life, or None when an install is under way.
    pub fn acquire(&self) -> Option<EngineInstallGuard<'_>> {
        (!self.0.swap(true, Ordering::AcqRel)).then_some(EngineInstallGuard(&self.0))
    }
}

pub struct EngineInstallGuard<'a>(&'a AtomicBool);

impl Drop for EngineInstallGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Installs the engine the manifest at `manifest_url` describes. With `force` false, an engine of
/// the same version that is already there is left alone.
pub fn install(
    lynx_home: &Path,
    manifest_url: &str,
    force: bool,
    report: &mut dyn FnMut(EngineProgress),
) -> Result<(), EngineInstallError> {
    if PLATFORM == "unsupported" {
        return Err(EngineInstallError::Unsupported);
    }
    let manifest = fetch_manifest(manifest_url)?;
    manifest.check(PROTOCOL_VERSION)?;

    let installed = read_status(lynx_home);
    if !force
        && installed.state == EngineState::Ready
        && installed.version.as_deref() == Some(manifest.version.as_str())
    {
        report(done());
        return Ok(());
    }

    let root = engine_root(lynx_home);
    let downloads = root.join("downloads");
    fs::create_dir_all(&downloads).map_err(|error| EngineInstallError::io(&downloads, error))?;
    let archive = downloads.join("engine.tar.gz.part");
    let result = download_and_place(
        lynx_home,
        &manifest,
        &manifest.archive_url(manifest_url)?,
        &archive,
        report,
    );
    // Whatever happened, the archive is not kept: it is as big as the engine itself.
    let _ = fs::remove_file(&archive);
    result?;
    report(done());
    Ok(())
}

fn done() -> EngineProgress {
    EngineProgress {
        phase: EnginePhase::Done,
        received: 0,
        total: 0,
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        // A big download takes as long as it takes; stalls are caught by the read timeout.
        .timeout_global(None)
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .build()
        .new_agent()
}

fn network_error(url: &str, error: &ureq::Error) -> EngineInstallError {
    EngineInstallError::Network {
        url: url.to_owned(),
        reason: error.to_string(),
    }
}

fn fetch_manifest(url: &str) -> Result<EngineManifest, EngineInstallError> {
    let mut response = agent()
        .get(url)
        .call()
        .map_err(|error| network_error(url, &error))?;
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|error| network_error(url, &error))?;
    serde_json::from_str(&text).map_err(|error| EngineInstallError::BadManifest(error.to_string()))
}

fn download_and_place(
    lynx_home: &Path,
    manifest: &EngineManifest,
    archive_url: &str,
    archive: &Path,
    report: &mut dyn FnMut(EngineProgress),
) -> Result<(), EngineInstallError> {
    download(archive_url, archive, manifest, report)?;

    report(EngineProgress {
        phase: EnginePhase::Unpacking,
        received: 0,
        total: 0,
    });
    let root = engine_root(lynx_home);
    let release = root.join(format!("{RELEASE_PREFIX}{}", manifest.version));
    let partial = root.join(format!("{RELEASE_PREFIX}{}.partial", manifest.version));
    let _ = fs::remove_dir_all(&partial);
    fs::create_dir_all(&partial).map_err(|error| EngineInstallError::io(&partial, error))?;
    let unpacked = unpack(archive, &partial, manifest);
    if let Err(error) = unpacked {
        let _ = fs::remove_dir_all(&partial);
        return Err(error);
    }
    let _ = fs::remove_dir_all(&release);
    fs::rename(&partial, &release).map_err(|error| EngineInstallError::io(&release, error))?;
    switch_current(lynx_home, &release)?;
    remove_other_releases(&root, &release);
    Ok(())
}

/// Streams the archive to disk, hashing as it goes, and checks the hash at the end.
fn download(
    url: &str,
    archive: &Path,
    manifest: &EngineManifest,
    report: &mut dyn FnMut(EngineProgress),
) -> Result<(), EngineInstallError> {
    let mut response = agent()
        .get(url)
        .call()
        .map_err(|error| network_error(url, &error))?;
    let total = response
        .body()
        .content_length()
        .filter(|length| *length > 0)
        .unwrap_or(manifest.size);
    let mut reader = response.body_mut().as_reader();
    let mut file = File::create(archive).map_err(|error| EngineInstallError::io(archive, error))?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut received = 0u64;
    let mut last_report = Instant::now() - REPORT_EVERY;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| network_error(url, &ureq::Error::Io(error)))?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
        file.write_all(&buffer[..read])
            .map_err(|error| EngineInstallError::io(archive, error))?;
        received += read as u64;
        if last_report.elapsed() >= REPORT_EVERY {
            last_report = Instant::now();
            report(EngineProgress {
                phase: EnginePhase::Downloading,
                received: clamp(received),
                total: clamp(total),
            });
        }
    }
    file.flush()
        .map_err(|error| EngineInstallError::io(archive, error))?;
    report(EngineProgress {
        phase: EnginePhase::Downloading,
        received: clamp(received),
        total: clamp(received),
    });

    report(EngineProgress {
        phase: EnginePhase::Verifying,
        received: clamp(received),
        total: clamp(received),
    });
    let digest = hex(&hash.finalize());
    if digest != manifest.sha256.to_ascii_lowercase() {
        return Err(EngineInstallError::ChecksumMismatch);
    }
    Ok(())
}

fn clamp(bytes: u64) -> u32 {
    u32::try_from(bytes).unwrap_or(u32::MAX)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Unpacks into `into`, refusing anything but plain files and folders that stay inside it, then
/// checks the engine is whole: a host program and an info file for this version.
fn unpack(
    archive: &Path,
    into: &Path,
    manifest: &EngineManifest,
) -> Result<(), EngineInstallError> {
    let file = File::open(archive).map_err(|error| EngineInstallError::io(archive, error))?;
    let mut entries = tar::Archive::new(GzDecoder::new(file));
    entries.set_preserve_permissions(true);
    entries.set_unpack_xattrs(false);
    let listing = entries
        .entries()
        .map_err(|error| EngineInstallError::IncompleteArchive(error.to_string()))?;
    for entry in listing {
        let mut entry =
            entry.map_err(|error| EngineInstallError::IncompleteArchive(error.to_string()))?;
        let kind = entry.header().entry_type();
        let name = entry
            .path()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !matches!(kind, EntryType::Regular | EntryType::Directory) {
            return Err(EngineInstallError::UnsafeArchive(format!(
                "{name} is a link or a special file"
            )));
        }
        let inside = entry
            .unpack_in(into)
            .map_err(|error| EngineInstallError::IncompleteArchive(error.to_string()))?;
        if !inside {
            return Err(EngineInstallError::UnsafeArchive(format!(
                "{name} points outside the engine folder"
            )));
        }
    }

    let host = into.join(HOST_NAME);
    if !host.is_file() {
        return Err(EngineInstallError::IncompleteArchive(format!(
            "there is no {HOST_NAME}"
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&host, fs::Permissions::from_mode(0o755))
            .map_err(|error| EngineInstallError::io(&host, error))?;
    }
    let info = fs::read_to_string(into.join(INFO_FILE))
        .map_err(|_| EngineInstallError::IncompleteArchive(format!("there is no {INFO_FILE}")))?;
    let info: serde_json::Value = serde_json::from_str(&info)
        .map_err(|error| EngineInstallError::IncompleteArchive(error.to_string()))?;
    if info["version"] != manifest.version.as_str()
        || info["protocol"].as_u64() != Some(u64::from(manifest.protocol))
    {
        return Err(EngineInstallError::IncompleteArchive(format!(
            "{INFO_FILE} does not match the manifest"
        )));
    }
    Ok(())
}

/// Puts `release` in place as `current`. An engine folder from development is moved aside, not
/// deleted.
fn switch_current(lynx_home: &Path, release: &Path) -> Result<(), EngineInstallError> {
    #[cfg(unix)]
    {
        switch_by_link(lynx_home, release)
    }
    #[cfg(not(unix))]
    {
        switch_by_rename(lynx_home, release)
    }
}

/// Moves a real `current` folder aside, to make room for a link.
#[cfg(unix)]
fn move_folder_aside(root: &Path, current: &Path) -> Result<(), EngineInstallError> {
    if let Ok(metadata) = fs::symlink_metadata(current)
        && metadata.is_dir()
    {
        let aside = root.join("previous-current");
        let _ = fs::remove_dir_all(&aside);
        fs::rename(current, &aside).map_err(|error| EngineInstallError::io(current, error))?;
    }
    Ok(())
}

/// Points `current` at `release`. The link is made beside it and renamed over, which replaces the
/// old one in a single step.
#[cfg(unix)]
fn switch_by_link(lynx_home: &Path, release: &Path) -> Result<(), EngineInstallError> {
    let current = current_dir(lynx_home);
    let root = engine_root(lynx_home);
    move_folder_aside(&root, &current)?;
    let link = root.join("current.new");
    let _ = fs::remove_file(&link);
    let target = release
        .file_name()
        .map(std::path::PathBuf::from)
        .ok_or_else(|| EngineInstallError::io(release, std::io::Error::other("no folder name")))?;
    std::os::unix::fs::symlink(&target, &link)
        .map_err(|error| EngineInstallError::io(&link, error))?;
    fs::rename(&link, &current).map_err(|error| EngineInstallError::io(&current, error))
}

/// Makes `release` the folder `current` itself, for a system where making a link takes
/// privileges. The old `current` is kept as `previous-current` until the next install, and put
/// back if the move fails. Nothing may be running from the old folder: Windows will not rename it.
#[cfg_attr(unix, allow(dead_code))]
fn switch_by_rename(lynx_home: &Path, release: &Path) -> Result<(), EngineInstallError> {
    let current = current_dir(lynx_home);
    let aside = engine_root(lynx_home).join("previous-current");
    let had_current = fs::symlink_metadata(&current).is_ok();
    if had_current {
        let _ = fs::remove_dir_all(&aside);
        fs::rename(&current, &aside).map_err(|error| EngineInstallError::io(&current, error))?;
    }
    if let Err(error) = fs::rename(release, &current) {
        if had_current {
            let _ = fs::rename(&aside, &current);
        }
        return Err(EngineInstallError::io(release, error));
    }
    Ok(())
}

/// Old releases are a few hundred megabytes each, so only the one in use stays. (A development
/// folder moved aside stays until the next install replaces it.)
fn remove_other_releases(root: &Path, keep: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(RELEASE_PREFIX) && entry.path() != keep {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::net::{Ipv4Addr, TcpListener};
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};

    use flate2::Compression;
    use flate2::write::GzEncoder;

    use super::*;
    use crate::browser::engine_manifest::PLATFORM;

    const VERSION: &str = "154.0.34-p2";

    /// A small archive shaped like the real one: a host, its info file and a stand-in library.
    fn archive(version: &str, protocol: u32, with_host: bool) -> Vec<u8> {
        let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        let mut add = |name: &str, body: &[u8], mode: u32| {
            let mut header = tar::Header::new_gnu();
            header.set_size(body.len() as u64);
            header.set_mode(mode);
            header.set_cksum();
            builder
                .append_data(&mut header, name, body)
                .expect("append");
        };
        if with_host {
            add(HOST_NAME, b"#!/bin/sh\n", 0o644);
        }
        let info = format!(r#"{{"version":"{version}","protocol":{protocol}}}"#);
        add(INFO_FILE, info.as_bytes(), 0o644);
        add("libcef.so", &[7u8; 4096], 0o644);
        builder.into_inner().expect("tar").finish().expect("gzip")
    }

    fn manifest_for(archive: &[u8], version: &str, protocol: u32) -> String {
        format!(
            r#"{{"version":"{version}","protocol":{protocol},"platform":"{PLATFORM}","cef":"154.0.34","archive":"engine.tar.gz","sha256":"{}","size":{}}}"#,
            hex(&Sha256::digest(archive)),
            archive.len()
        )
    }

    /// Serves a manifest and an archive, and counts how many archive requests came in.
    struct Server {
        manifest_url: String,
        archive_requests: Arc<AtomicUsize>,
    }

    fn serve(manifest: String, archive: Vec<u8>) -> Server {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let port = listener.local_addr().expect("addr").port();
        let archive_requests = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&archive_requests);
        let manifest = Arc::new(Mutex::new(manifest));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (counted, manifest, archive) =
                    (Arc::clone(&counted), Arc::clone(&manifest), archive.clone());
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let mut request = [0u8; 2048];
                    let read = stream.read(&mut request).unwrap_or(0);
                    let request = String::from_utf8_lossy(&request[..read]).into_owned();
                    let (kind, body) = if request.starts_with("GET /engine.tar.gz") {
                        counted.fetch_add(1, Ordering::SeqCst);
                        ("application/gzip", archive)
                    } else {
                        let text = manifest.lock().expect("manifest").clone();
                        ("application/json", text.into_bytes())
                    };
                    let head = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(head.as_bytes());
                    let _ = stream.write_all(&body);
                });
            }
        });
        Server {
            manifest_url: format!("http://127.0.0.1:{port}/manifest.json"),
            archive_requests,
        }
    }

    fn run(
        home: &Path,
        server: &Server,
        force: bool,
    ) -> (Result<(), EngineInstallError>, Vec<EnginePhase>) {
        let mut phases = Vec::new();
        let result = install(home, &server.manifest_url, force, &mut |progress| {
            if phases.last() != Some(&progress.phase) {
                phases.push(progress.phase);
            }
        });
        (result, phases)
    }

    #[test]
    fn an_engine_is_downloaded_checked_unpacked_and_put_in_place() {
        let home = tempfile::tempdir().unwrap();
        let bytes = archive(VERSION, PROTOCOL_VERSION, true);
        let server = serve(manifest_for(&bytes, VERSION, PROTOCOL_VERSION), bytes);

        let (result, phases) = run(home.path(), &server, false);

        result.expect("installed");
        assert_eq!(
            phases,
            [
                EnginePhase::Downloading,
                EnginePhase::Verifying,
                EnginePhase::Unpacking,
                EnginePhase::Done
            ]
        );
        let status = read_status(home.path());
        assert_eq!(status.state, EngineState::Ready);
        assert_eq!(status.version.as_deref(), Some(VERSION));
        let host = current_dir(home.path()).join(HOST_NAME);
        assert!(host.is_file());
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&host).unwrap().permissions().mode() & 0o111,
            0o111
        );
        assert!(
            fs::symlink_metadata(current_dir(home.path()))
                .unwrap()
                .is_symlink(),
            "current is a link to the release"
        );
        assert!(
            !engine_root(home.path())
                .join("downloads/engine.tar.gz.part")
                .exists()
        );
    }

    #[test]
    fn the_same_version_is_not_downloaded_again_unless_forced() {
        let home = tempfile::tempdir().unwrap();
        let bytes = archive(VERSION, PROTOCOL_VERSION, true);
        let server = serve(manifest_for(&bytes, VERSION, PROTOCOL_VERSION), bytes);
        run(home.path(), &server, false).0.expect("first");

        let (second, phases) = run(home.path(), &server, false);
        second.expect("second");
        assert_eq!(phases, [EnginePhase::Done]);
        assert_eq!(server.archive_requests.load(Ordering::SeqCst), 1);

        run(home.path(), &server, true).0.expect("forced");
        assert_eq!(server.archive_requests.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_download_that_does_not_match_its_checksum_installs_nothing() {
        let home = tempfile::tempdir().unwrap();
        let bytes = archive(VERSION, PROTOCOL_VERSION, true);
        let manifest = manifest_for(&bytes, VERSION, PROTOCOL_VERSION)
            .replace(&hex(&Sha256::digest(&bytes)), &"0".repeat(64));
        let server = serve(manifest, bytes);

        let (result, _) = run(home.path(), &server, false);

        assert!(matches!(result, Err(EngineInstallError::ChecksumMismatch)));
        assert_eq!(read_status(home.path()).state, EngineState::Missing);
        let leftovers: Vec<_> = fs::read_dir(engine_root(home.path()))
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            leftovers,
            ["downloads"],
            "only the empty download folder remains"
        );
    }

    #[test]
    fn an_engine_for_another_protocol_is_refused_before_anything_is_downloaded() {
        let home = tempfile::tempdir().unwrap();
        let bytes = archive("old", PROTOCOL_VERSION + 1, true);
        let server = serve(manifest_for(&bytes, "old", PROTOCOL_VERSION + 1), bytes);

        let (result, _) = run(home.path(), &server, false);

        assert!(matches!(
            result,
            Err(EngineInstallError::ProtocolMismatch { .. })
        ));
        assert_eq!(server.archive_requests.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn an_archive_without_a_host_is_refused_and_leaves_the_old_engine_alone() {
        let home = tempfile::tempdir().unwrap();
        let good = archive("old-1", PROTOCOL_VERSION, true);
        let first = serve(manifest_for(&good, "old-1", PROTOCOL_VERSION), good);
        run(home.path(), &first, false).0.expect("first install");

        let broken = archive(VERSION, PROTOCOL_VERSION, false);
        let second = serve(manifest_for(&broken, VERSION, PROTOCOL_VERSION), broken);
        let (result, _) = run(home.path(), &second, false);

        assert!(matches!(
            result,
            Err(EngineInstallError::IncompleteArchive(_))
        ));
        let status = read_status(home.path());
        assert_eq!(status.state, EngineState::Ready);
        assert_eq!(status.version.as_deref(), Some("old-1"));
    }

    #[test]
    fn an_archive_that_reaches_outside_its_folder_is_refused() {
        let home = tempfile::tempdir().unwrap();
        let mut tarball = Vec::new();
        {
            let mut header = tar::Header::new_gnu();
            header.set_size(1);
            header.set_mode(0o644);
            // `set_path` refuses "..", so the name is written into the header by hand.
            header.as_old_mut().name[..11].copy_from_slice(b"../escaped\0");
            header.set_cksum();
            let mut builder = tar::Builder::new(&mut tarball);
            builder.append(&header, &b"x"[..]).expect("append");
            builder.finish().expect("finish");
        }
        let mut gzip = GzEncoder::new(Vec::new(), Compression::fast());
        gzip.write_all(&tarball).unwrap();
        let bytes = gzip.finish().unwrap();
        let server = serve(manifest_for(&bytes, VERSION, PROTOCOL_VERSION), bytes);

        let (result, _) = run(home.path(), &server, false);

        assert!(
            matches!(result, Err(EngineInstallError::UnsafeArchive(_))),
            "{result:?}"
        );
        assert!(!home.path().join("engine/escaped").exists());
        assert!(!home.path().join("escaped").exists());
    }

    #[test]
    fn an_engine_folder_from_development_is_moved_aside_not_destroyed() {
        let home = tempfile::tempdir().unwrap();
        let dev = current_dir(home.path());
        fs::create_dir_all(&dev).unwrap();
        fs::write(dev.join(HOST_NAME), b"dev build").unwrap();
        let bytes = archive(VERSION, PROTOCOL_VERSION, true);
        let server = serve(manifest_for(&bytes, VERSION, PROTOCOL_VERSION), bytes);

        run(home.path(), &server, false).0.expect("installed");

        assert_eq!(read_status(home.path()).state, EngineState::Ready);
        assert_eq!(
            fs::read(
                engine_root(home.path())
                    .join("previous-current")
                    .join(HOST_NAME)
            )
            .unwrap(),
            b"dev build"
        );
    }

    #[test]
    fn a_new_release_replaces_the_old_one_and_the_old_folder_goes() {
        let home = tempfile::tempdir().unwrap();
        let one = archive("one", PROTOCOL_VERSION, true);
        run(
            home.path(),
            &serve(manifest_for(&one, "one", PROTOCOL_VERSION), one),
            false,
        )
        .0
        .expect("one");
        let two = archive("two", PROTOCOL_VERSION, true);
        run(
            home.path(),
            &serve(manifest_for(&two, "two", PROTOCOL_VERSION), two),
            false,
        )
        .0
        .expect("two");

        let root = engine_root(home.path());
        assert_eq!(read_status(home.path()).version.as_deref(), Some("two"));
        assert!(root.join("release-two").is_dir());
        assert!(!root.join("release-one").exists());
    }

    #[test]
    fn an_unreachable_manifest_is_a_network_error() {
        let home = tempfile::tempdir().unwrap();
        let free = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = free.local_addr().unwrap().port();
        drop(free);

        let result = install(
            home.path(),
            &format!("http://127.0.0.1:{port}/m.json"),
            false,
            &mut |_| {},
        );

        assert!(
            matches!(result, Err(EngineInstallError::Network { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn only_one_install_runs_at_a_time() {
        let lock = EngineInstallLock::default();
        let first = lock.acquire().expect("first");
        assert!(lock.acquire().is_none());
        drop(first);
        assert!(lock.acquire().is_some());
    }

    // Needs a published package, such as the one `bun run browser:engine -- --bundle` makes, served
    // over HTTP: `LYNX_ENGINE_MANIFEST_URL=http://127.0.0.1:8123/browser-engine-manifest-linux-x64.json
    // cargo test -p lynx-ide an_installed_package_runs -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs a served engine package"]
    fn an_installed_package_runs() {
        use crate::browser::browser_service::{BrowserEvents, BrowserService, Frame, FrameSink};
        use crate::browser::browser_state::BrowserState;

        struct Nowhere;
        impl BrowserEvents for Nowhere {
            fn state_changed(&self, _state: &BrowserState) {}
        }
        impl FrameSink for Nowhere {
            fn frame(&self, _frame: Frame) {}
        }

        let url = std::env::var(super::super::engine_manifest::MANIFEST_URL_ENV)
            .expect("the manifest url is in the environment");
        let home = tempfile::tempdir().unwrap();
        let mut reported = Vec::new();
        install(home.path(), &url, false, &mut |progress| {
            if reported.last() != Some(&progress.phase) {
                reported.push(progress.phase);
            }
        })
        .expect("installs");
        println!("phases: {reported:?}");
        assert_eq!(read_status(home.path()).state, EngineState::Ready);

        let service = Arc::new(BrowserService::new(
            home.path().to_path_buf(),
            Arc::new(Nowhere),
            Arc::new(Nowhere),
        ));
        service.resize(640, 480, 1.0);
        service
            .ensure_running()
            .expect("the installed engine starts");
        let answer = service
            .cdp(
                "Runtime.evaluate",
                serde_json::json!({ "expression": "6 * 7", "returnByValue": true }),
                Duration::from_secs(10),
            )
            .expect("the page answers");
        assert_eq!(answer["result"]["value"], 42);
        service.shutdown();
    }

    #[test]
    fn where_links_are_not_available_the_release_is_renamed_into_place() {
        let home = tempfile::tempdir().unwrap();
        let root = engine_root(home.path());
        let old = current_dir(home.path());
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("marker"), b"old").unwrap();
        let release = root.join(format!("{RELEASE_PREFIX}new"));
        fs::create_dir_all(&release).unwrap();
        fs::write(release.join("marker"), b"new").unwrap();

        switch_by_rename(home.path(), &release).expect("switched");

        assert_eq!(
            fs::read(current_dir(home.path()).join("marker")).unwrap(),
            b"new"
        );
        assert_eq!(
            fs::read(root.join("previous-current/marker")).unwrap(),
            b"old"
        );
        assert!(!release.exists());
        assert!(
            !fs::symlink_metadata(current_dir(home.path()))
                .unwrap()
                .is_symlink(),
            "current is the folder itself"
        );
    }

    #[test]
    fn a_rename_that_fails_puts_the_old_engine_back() {
        let home = tempfile::tempdir().unwrap();
        let old = current_dir(home.path());
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("marker"), b"old").unwrap();

        let result = switch_by_rename(
            home.path(),
            &engine_root(home.path()).join("release-missing"),
        );

        assert!(result.is_err());
        assert_eq!(
            fs::read(current_dir(home.path()).join("marker")).unwrap(),
            b"old"
        );
    }
}
