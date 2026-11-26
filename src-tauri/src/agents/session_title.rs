//! The name an agent keeps for its conversation, so a tab can show it. Claude writes it into the
//! session's transcript. The file is watched while the session lives, and read again only when it grows.

use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;

use super::harness::HarnessId;

/// Keeps a title watch alive; dropping it stops the watch.
pub struct TitleWatch {
    _watcher: RecommendedWatcher,
}

/// Calls `on_title` with the name the agent gives the conversation now, and again each time it changes.
/// `Ok(None)` when the harness keeps no readable name.
pub fn watch_title(
    harness: HarnessId,
    resume_id: &str,
    on_title: impl FnMut(String) + Send + 'static,
) -> notify::Result<Option<TitleWatch>> {
    if !is_plain(resume_id) {
        return Ok(None);
    }
    match harness {
        HarnessId::Claude => watch_claude(resume_id, on_title),
        HarnessId::Codex => watch_codex(resume_id, on_title),
        HarnessId::Opencode | HarnessId::Omp => Ok(None),
    }
}

fn watch_claude(
    resume_id: &str,
    on_title: impl FnMut(String) + Send + 'static,
) -> notify::Result<Option<TitleWatch>> {
    let Some(projects) = claude_projects_dir() else {
        return Ok(None);
    };
    let file_name = format!("{resume_id}.jsonl");
    let first = find_transcript(&projects, &file_name);
    let mut titles = TranscriptTitles::default();
    let watch = follow(
        &projects,
        RecursiveMode::Recursive,
        move |path| path.file_name() == Some(OsStr::new(&file_name)),
        first,
        move |path| titles.read(path),
        on_title,
    )?;
    Ok(Some(watch))
}

/// Codex keeps each thread's name in the `threads` table of its state database. The file name carries
/// the schema version (5 in Codex 0.172): a Codex that moves the table to a new file stops the titles here.
fn watch_codex(
    resume_id: &str,
    on_title: impl FnMut(String) + Send + 'static,
) -> notify::Result<Option<TitleWatch>> {
    let Some(home) = home_of("CODEX_HOME", ".codex") else {
        return Ok(None);
    };
    let database = home.join("state_5.sqlite");
    let thread = resume_id.to_owned();
    let read_from = database.clone();
    // The folder is watched, not the file: the file may not exist yet, and only its two names pass.
    let watch = follow(
        &home,
        RecursiveMode::NonRecursive,
        is_codex_state,
        Some(database),
        move |_path| codex_name(&read_from, &thread),
        on_title,
    )?;
    Ok(Some(watch))
}

/// The database file and the write-ahead log that carries its newest commits. Other files in Codex's
/// folder change all the time and are not read.
fn is_codex_state(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(OsStr::to_str),
        Some("state_5.sqlite" | "state_5.sqlite-wal")
    )
}

/// The name Codex shows for a thread: the one the user gave with `/rename`, else the automatic title.
/// Opened read-only, so Lynx never takes a write lock on Codex's database.
fn codex_name(database: &Path, thread: &str) -> Option<String> {
    let connection = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let (name, title) = connection
        .query_row(
            "SELECT name, title FROM threads WHERE id = ?1",
            [thread],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .optional()
        .ok()??;
    clean(name).or_else(|| clean(title))
}

/// Watches `dir` for changes to the files `wanted` picks, and reports the title `read` finds in each
/// one that changed. `first` is read once, after the watch starts, so no change slips between the two.
fn follow(
    dir: &Path,
    mode: RecursiveMode,
    wanted: impl Fn(&Path) -> bool + Send + 'static,
    first: Option<PathBuf>,
    read: impl FnMut(&Path) -> Option<String> + Send + 'static,
    on_title: impl FnMut(String) + Send + 'static,
) -> notify::Result<TitleWatch> {
    let checker = Arc::new(Mutex::new(Checker {
        read: Box::new(read),
        on_title: Box::new(on_title),
        sent: None,
    }));
    let shared = Arc::clone(&checker);
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        let event = match event {
            Ok(event) => event,
            Err(error) => {
                log::error!("could not watch an agent title: {error}");
                return;
            }
        };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            return;
        }
        let mut checker = shared.lock().unwrap_or_else(PoisonError::into_inner);
        for path in event.paths.iter().filter(|path| wanted(path)) {
            checker.check(path);
        }
    })?;
    watcher.watch(dir, mode)?;
    if let Some(path) = first {
        checker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .check(&path);
    }
    Ok(TitleWatch { _watcher: watcher })
}

type Reader = Box<dyn FnMut(&Path) -> Option<String> + Send>;
type OnTitle = Box<dyn FnMut(String) + Send>;

/// Holds the reader and the title last reported, so a change that leaves the title as it was is silent.
struct Checker {
    read: Reader,
    on_title: OnTitle,
    sent: Option<String>,
}

impl Checker {
    fn check(&mut self, path: &Path) {
        let Some(title) = (self.read)(path) else {
            return;
        };
        if self.sent.as_ref() == Some(&title) {
            return;
        }
        self.sent = Some(title.clone());
        (self.on_title)(title);
    }
}

/// Claude's transcript: each `ai-title` is the automatic name, and each `custom-title` is the one
/// the user gave with `/rename`, which wins. Both are last-wins, as Claude reads them.
#[derive(Default)]
struct TranscriptTitles {
    tail: Tail,
    custom: Option<String>,
    ai: Option<String>,
}

impl TranscriptTitles {
    fn read(&mut self, path: &Path) -> Option<String> {
        for line in self.tail.lines(path).unwrap_or_default() {
            if !(line.contains("\"ai-title\"") || line.contains("\"custom-title\"")) {
                continue;
            }
            let Ok(record) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let text =
                |key: &str| clean(record.get(key).and_then(Value::as_str).map(str::to_owned));
            match record.get("type").and_then(Value::as_str) {
                Some("custom-title") => self.custom = text("customTitle"),
                Some("ai-title") => self.ai = text("aiTitle"),
                _ => {}
            }
        }
        self.custom.clone().or_else(|| self.ai.clone())
    }
}

/// A file that only grows, read from where the last read stopped. A line still being written waits
/// for its newline, so it is read whole later.
#[derive(Default)]
struct Tail {
    offset: u64,
}

impl Tail {
    fn lines(&mut self, path: &Path) -> io::Result<Vec<String>> {
        let mut file = File::open(path)?;
        // shortcut: a file that shrank is read again from the start, and a title it no longer holds stays
        // until a newer record replaces it; Claude only appends, so this does not happen in practice.
        if file.metadata()?.len() < self.offset {
            self.offset = 0;
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let Some(last_newline) = bytes.iter().rposition(|&byte| byte == b'\n') else {
            return Ok(Vec::new());
        };
        self.offset += last_newline as u64 + 1;
        Ok(String::from_utf8_lossy(&bytes[..=last_newline])
            .lines()
            .map(str::to_owned)
            .collect())
    }
}

/// Where Claude keeps its transcripts: `CLAUDE_CONFIG_DIR` when set, else `~/.claude`.
fn claude_projects_dir() -> Option<PathBuf> {
    Some(home_of("CLAUDE_CONFIG_DIR", ".claude")?.join("projects"))
}

/// An agent's own folder: the variable when set, else the folder under the home directory.
fn home_of(variable: &str, folder: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|home| home.join(folder)))
}

/// A name the user or agent actually wrote: blank text does not count.
fn clean(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// Claude names each project folder after the working directory, so the transcript is looked up by
/// its file name in every folder instead of recomputing that name.
fn find_transcript(projects: &Path, file_name: &str) -> Option<PathBuf> {
    std::fs::read_dir(projects)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join(file_name))
        .find(|path| path.is_file())
}

/// The id comes from the agent's hook and is typed into a file name, so only plain characters pass.
fn is_plain(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Write;

    fn append(path: &Path, text: &str) {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    #[test]
    fn the_name_the_user_gave_wins_over_the_automatic_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        append(
            &path,
            "{\"type\":\"ai-title\",\"aiTitle\":\"Automatico\"}\n{\"type\":\"custom-title\",\"customTitle\":\"Meu nome\"}\n{\"type\":\"ai-title\",\"aiTitle\":\"Automatico 2\"}\n",
        );
        let mut titles = TranscriptTitles::default();
        assert_eq!(titles.read(&path).as_deref(), Some("Meu nome"));
    }

    #[test]
    fn a_line_still_being_written_waits_for_its_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        append(&path, "{\"type\":\"ai-title\",\"aiTitle\":\"Pela");
        let mut titles = TranscriptTitles::default();
        assert_eq!(titles.read(&path), None);
        append(&path, " metade\"}\n");
        assert_eq!(titles.read(&path).as_deref(), Some("Pela metade"));
    }

    #[test]
    fn a_transcript_read_again_keeps_only_what_is_new() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        append(&path, "{\"type\":\"ai-title\",\"aiTitle\":\"Um\"}\n");
        let mut titles = TranscriptTitles::default();
        assert_eq!(titles.read(&path).as_deref(), Some("Um"));
        append(
            &path,
            "{\"type\":\"user\"}\n{\"type\":\"ai-title\",\"aiTitle\":\"Dois\"}\n",
        );
        assert_eq!(titles.read(&path).as_deref(), Some("Dois"));
    }

    #[test]
    fn codex_name_prefers_the_user_name_and_reads_beside_an_open_writer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state_5.sqlite");
        let writer = Connection::open(&path).unwrap();
        writer.pragma_update(None, "journal_mode", "WAL").unwrap();
        writer
            .execute_batch(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, name TEXT, title TEXT);
                 INSERT INTO threads VALUES ('a', 'Meu nome', 'automatico');
                 INSERT INTO threads VALUES ('b', NULL, 'automatico');
                 INSERT INTO threads VALUES ('c', '  ', 'so automatico');",
            )
            .unwrap();
        // The writer stays open, as Codex keeps it while it runs, so the -wal and -shm files exist.
        assert_eq!(codex_name(&path, "a").as_deref(), Some("Meu nome"));
        assert_eq!(codex_name(&path, "b").as_deref(), Some("automatico"));
        assert_eq!(codex_name(&path, "c").as_deref(), Some("so automatico"));
        assert_eq!(codex_name(&path, "missing"), None);
        drop(writer);
        // Once the writer is gone and the log folded in, a read-only open still finds the name.
        assert_eq!(codex_name(&path, "a").as_deref(), Some("Meu nome"));
    }

    #[test]
    fn a_rename_in_the_codex_database_is_reported_while_watched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state_5.sqlite");
        let writer = Connection::open(&path).unwrap();
        writer.pragma_update(None, "journal_mode", "WAL").unwrap();
        writer
            .execute_batch(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, name TEXT, title TEXT);
                 INSERT INTO threads VALUES ('a', NULL, 'automatico');",
            )
            .unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let read_from = path.clone();
        let _watch = follow(
            dir.path(),
            RecursiveMode::NonRecursive,
            is_codex_state,
            Some(path.clone()),
            move |_| codex_name(&read_from, "a"),
            move |title| {
                let _ = sender.send(title);
            },
        )
        .unwrap();
        let wait = std::time::Duration::from_secs(10);
        assert_eq!(receiver.recv_timeout(wait).unwrap(), "automatico");
        writer
            .execute("UPDATE threads SET name = 'Meu nome' WHERE id = 'a'", [])
            .unwrap();
        assert_eq!(receiver.recv_timeout(wait).unwrap(), "Meu nome");
    }
}
