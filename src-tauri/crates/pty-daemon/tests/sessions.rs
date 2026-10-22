//! Real shells behind a real daemon, on Unix and Windows alike.

use std::path::Path;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use lynx_pty_daemon::client::Client;
use lynx_pty_daemon::message::{Message, Request};
use lynx_pty_daemon::server::{DaemonError, Options, is_running, serve};

const WAIT: Duration = Duration::from_secs(15);

#[cfg(unix)]
fn shell() -> (String, Vec<String>) {
    ("/bin/sh".to_owned(), Vec::new())
}
#[cfg(windows)]
fn shell() -> (String, Vec<String>) {
    ("cmd.exe".to_owned(), Vec::new())
}

/// A line to type, and the text only its output has: the typed text echoes the expression, the
/// result is what the shell computed.
#[cfg(unix)]
fn echo(tag: &str) -> (String, String) {
    (format!("echo {tag}-$((20+22))\r"), format!("{tag}-42"))
}
#[cfg(windows)]
fn echo(tag: &str) -> (String, String) {
    (format!("echo {tag}-%OS%\r"), format!("{tag}-Windows_NT"))
}

/// Output without control sequences or whitespace; ConPTY redraws with cursor moves.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            match chars.next() {
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(n) = chars.next() {
                        if n == '\u{7}' {
                            break;
                        }
                        if n == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                }
                Some('(' | ')') => {
                    chars.next();
                }
                _ => {}
            }
        } else if !c.is_whitespace() && !c.is_control() {
            out.push(c);
        }
    }
    out
}

struct Daemon {
    dir: tempfile::TempDir,
    done: mpsc::Receiver<Result<(), DaemonError>>,
    _thread: JoinHandle<()>,
}

fn start(idle: Duration) -> Daemon {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().to_owned();
    let (sender, done) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let _ = sender.send(serve(
            &path,
            &Options {
                idle,
                prepare: |_| {},
            },
        ));
    });
    Daemon {
        dir,
        done,
        _thread: thread,
    }
}

fn connect(dir: &Path) -> Client {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Ok(client) = Client::connect(dir) {
            return client;
        }
        assert!(Instant::now() < deadline, "the daemon never accepted");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn create(client: &mut Client, cwd: &Path) -> String {
    let (program, args) = shell();
    client
        .send(&Request::Create {
            id: String::new(),
            cwd: cwd.to_string_lossy().into_owned(),
            program,
            args,
            env: Vec::new(),
            cols: 80,
            rows: 24,
        })
        .expect("send");
    match client.next(WAIT) {
        Some(Message::Created { id, .. }) => id,
        other => panic!("expected Created, got {other:?}"),
    }
}

/// Reads until what the session printed contains `needle`. Returns the highest `seq` seen.
fn read_until(client: &Client, needle: &str, seen: &mut String) -> u32 {
    let deadline = Instant::now() + WAIT;
    let mut seq = 0;
    while !plain(seen).contains(needle) {
        let left = deadline.saturating_duration_since(Instant::now());
        match client.next(left) {
            Some(Message::Snapshot { data, last_seq, .. }) => {
                seen.push_str(&data);
                seq = seq.max(last_seq);
            }
            Some(Message::Output { data, seq: s, .. }) => {
                seen.push_str(&data);
                seq = seq.max(s);
            }
            Some(_) => {}
            None => panic!("never saw {needle:?}; got {:?}", plain(seen)),
        }
    }
    seq
}

fn write(client: &mut Client, id: &str, data: &str) {
    client
        .send(&Request::Write {
            id: id.to_owned(),
            data: data.to_owned(),
        })
        .expect("send");
}

#[test]
fn a_session_outlives_its_client_and_a_new_one_gets_the_output_so_far() {
    let daemon = start(Duration::from_millis(300));
    let mut first = connect(daemon.dir.path());
    let id = create(&mut first, daemon.dir.path());
    first
        .send(&Request::Attach { id: id.clone() })
        .expect("send");
    let (line, marker) = echo("before");
    write(&mut first, &id, &line);
    read_until(&first, &marker, &mut String::new());
    drop(first);

    // A second client, as the app would be after a restart.
    let mut second = connect(daemon.dir.path());
    second
        .send(&Request::Attach { id: id.clone() })
        .expect("send");
    let mut seen = String::new();
    let last_seq = read_until(&second, &marker, &mut seen);
    assert!(last_seq >= 1, "the snapshot says where the stream goes on");

    let (line, marker) = echo("after");
    write(&mut second, &id, &line);
    let later = read_until(&second, &marker, &mut seen);
    assert!(later > last_seq, "live output continues after the snapshot");

    second.send(&Request::Kill { id }).expect("send");
    // With the only session gone, the daemon leaves by itself.
    assert!(matches!(daemon.done.recv_timeout(WAIT), Ok(Ok(()))));
    assert!(!is_running(daemon.dir.path()));
}

#[test]
fn a_second_daemon_in_the_same_folder_is_refused() {
    let daemon = start(Duration::from_secs(30));
    let _client = connect(daemon.dir.path());
    assert!(is_running(daemon.dir.path()));

    let second = serve(
        daemon.dir.path(),
        &Options {
            idle: Duration::from_secs(1),
            prepare: |_| {},
        },
    );

    assert!(matches!(second, Err(DaemonError::AlreadyRunning)));
}

#[test]
fn a_daemon_with_no_session_leaves_after_the_idle_time() {
    let daemon = start(Duration::from_millis(200));
    assert!(matches!(daemon.done.recv_timeout(WAIT), Ok(Ok(()))));
    assert!(!is_running(daemon.dir.path()));
}

#[test]
fn bad_requests_get_an_error_not_a_crash() {
    let daemon = start(Duration::from_secs(30));
    let mut client = connect(daemon.dir.path());

    client
        .send(&Request::Write {
            id: "nope".to_owned(),
            data: "x".to_owned(),
        })
        .expect("send");
    assert!(matches!(client.next(WAIT), Some(Message::Error { .. })));

    let (program, args) = shell();
    client
        .send(&Request::Create {
            id: String::new(),
            cwd: "/definitely/not/here".to_owned(),
            program,
            args,
            env: Vec::new(),
            cols: 80,
            rows: 24,
        })
        .expect("send");
    assert!(matches!(client.next(WAIT), Some(Message::Error { .. })));
}

/// A client that asks for output and never reads it must not stop the session, or the next client.
#[cfg(unix)]
#[test]
fn a_client_that_stops_reading_does_not_hold_the_session() {
    use lynx_pty_daemon::client::write_request;

    let daemon = start(Duration::from_secs(30));
    let mut owner = connect(daemon.dir.path());
    // More than a socket buffer holds, so a blocked client would stall the session; then a marker.
    let command = "yes | head -c 4000000; echo finished-$((2+3))";
    let args = vec!["-c".to_owned(), command.to_owned()];
    owner
        .send(&Request::Create {
            id: String::new(),
            cwd: daemon.dir.path().to_string_lossy().into_owned(),
            program: "/bin/sh".to_owned(),
            args,
            env: Vec::new(),
            cols: 80,
            rows: 24,
        })
        .expect("send");
    let Some(Message::Created { id, .. }) = owner.next(WAIT) else {
        panic!("the session was not created");
    };

    // Attaches, then never reads: its writer thread blocks, its queue fills, and it is dropped.
    let mut stuck = lynx_pty_daemon::client::connect(daemon.dir.path()).expect("connect");
    write_request(&mut stuck, &Request::Attach { id: id.clone() }).expect("attach");

    let mut late = connect(daemon.dir.path());
    late.send(&Request::Attach { id: id.clone() })
        .expect("send");
    // Reads come in small pieces, so the marker can straddle two of them. Its end is kept
    // apart, and the output is ASCII, so cutting it by bytes is safe.
    let mut tail = String::new();
    let deadline = Instant::now() + WAIT;
    loop {
        match late.next(deadline.saturating_duration_since(Instant::now())) {
            Some(Message::Snapshot { data, .. } | Message::Output { data, .. }) => {
                tail.push_str(&data);
                if tail.len() > 64 {
                    tail = tail.split_off(tail.len() - 64);
                }
                if tail.contains("finished-5") {
                    break;
                }
            }
            Some(_) => {}
            None => panic!("the session stopped before it finished"),
        }
    }
}

#[cfg(unix)]
#[test]
fn killing_a_session_ends_its_process() {
    let daemon = start(Duration::from_secs(30));
    let mut client = connect(daemon.dir.path());
    let id = create(&mut client, daemon.dir.path());
    client
        .send(&Request::Attach { id: id.clone() })
        .expect("send");
    let (line, marker) = echo("alive");
    write(&mut client, &id, &line);
    read_until(&client, &marker, &mut String::new());

    client
        .send(&Request::Kill { id: id.clone() })
        .expect("send");

    let deadline = Instant::now() + WAIT;
    loop {
        match client.next(deadline.saturating_duration_since(Instant::now())) {
            Some(Message::Exit { id: exited, .. }) if exited == id => break,
            Some(_) => {}
            None => panic!("the killed session never reported its exit"),
        }
    }
}

/// A daemon answers only a connection that shows its token; anything else gets one refusal.
#[test]
fn a_connection_without_the_token_is_refused_and_sees_no_session() {
    use lynx_browser_protocol::transport::Connection;
    use lynx_pty_daemon::client::write_request;

    let daemon = start(Duration::from_secs(30));
    let mut owner = connect(daemon.dir.path());
    let id = create(&mut owner, daemon.dir.path());
    let address = std::fs::read_to_string(daemon.dir.path().join("address")).expect("address");

    let mut stranger = Connection::connect(&address).expect("connect");
    write_request(&mut stranger, &Request::List).expect("send");
    let refusal = read_message(&mut stranger);
    assert!(matches!(refusal, Message::Error { .. }), "got {refusal:?}");

    let mut wrong = Connection::connect(&address).expect("connect");
    write_request(
        &mut wrong,
        &Request::Hello {
            token: "not the token".to_owned(),
        },
    )
    .expect("send");
    assert!(matches!(read_message(&mut wrong), Message::Error { .. }));
    assert!(
        owner.send(&Request::Attach { id }).is_ok(),
        "the owner is unaffected"
    );
}

fn read_message(connection: &mut lynx_browser_protocol::transport::Connection) -> Message {
    use std::io::BufRead;
    let mut line = String::new();
    std::io::BufReader::new(connection)
        .read_line(&mut line)
        .expect("a line");
    serde_json::from_str(&line).expect("a message")
}

/// The folder and the token are the only ways in, so both belong to the owner alone.
#[cfg(unix)]
#[test]
fn the_folder_socket_and_token_are_private_to_their_owner() {
    use std::os::unix::fs::PermissionsExt;
    let daemon = start(Duration::from_secs(30));
    let _client = connect(daemon.dir.path());
    let mode = |name: &str| {
        std::fs::metadata(daemon.dir.path().join(name))
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    };
    assert_eq!(mode("."), 0o700);
    assert_eq!(mode("token"), 0o600);
    assert_eq!(mode("link.sock"), 0o600);
}

/// A client picks the id, and a later connection reattaches to that session by it.
#[cfg(unix)]
#[test]
fn a_session_can_be_reattached_by_the_id_its_client_chose() {
    let daemon = start(Duration::from_secs(30));
    let mut first = connect(daemon.dir.path());
    let (program, args) = shell();
    first
        .send(&Request::Create {
            id: "chosen-by-the-app".to_owned(),
            cwd: daemon.dir.path().to_string_lossy().into_owned(),
            program,
            args,
            env: Vec::new(),
            cols: 80,
            rows: 24,
        })
        .expect("send");
    assert!(
        matches!(first.next(WAIT), Some(Message::Created { ref id, .. }) if id == "chosen-by-the-app")
    );
    first
        .send(&Request::Attach {
            id: "chosen-by-the-app".to_owned(),
        })
        .expect("send");
    let (line, marker) = echo("kept");
    write(&mut first, "chosen-by-the-app", &line);
    read_until(&first, &marker, &mut String::new());
    drop(first);

    let mut later = connect(daemon.dir.path());
    later
        .send(&Request::Attach {
            id: "chosen-by-the-app".to_owned(),
        })
        .expect("send");
    let mut seen = String::new();
    read_until(&later, &marker, &mut seen);
}

/// Shutdown ends every session and the daemon leaves, so its program can be replaced.
#[cfg(unix)]
#[test]
fn shutdown_ends_every_session_and_the_daemon_leaves() {
    let daemon = start(Duration::from_secs(30));
    let mut client = connect(daemon.dir.path());
    let id = create(&mut client, daemon.dir.path());
    client
        .send(&Request::Attach { id: id.clone() })
        .expect("send");

    client.send(&Request::Shutdown).expect("send");

    assert!(matches!(daemon.done.recv_timeout(WAIT), Ok(Ok(()))));
    assert!(!is_running(daemon.dir.path()));
}
