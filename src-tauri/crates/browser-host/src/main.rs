//! `lynx-browser-host`: runs the browser engine for Lynx. Lynx starts it with
//! `--socket <path> --frames <path> --cache <folder>`; the host connects to the socket, renders
//! the pages Lynx asks for off-screen into the shared frame memory, and obeys the commands it
//! receives. When the connection closes, it quits.

// On Windows the host is a program of its own with no console: a console would flash on screen
// for it and for each of the engine's helper processes, which are this same program.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod engine;
mod input;
mod sandbox;

use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use cef::*;
use lynx_browser_protocol::frame_buffer::FrameBuffer;
use lynx_browser_protocol::message::{ToHost, read_message};
use lynx_browser_protocol::transport::Connection;

use engine::{Event, HostApp};

/// The longest the loop sleeps without giving the engine a turn.
const MAX_IDLE: Duration = Duration::from_millis(20);

struct Arguments {
    socket: PathBuf,
    frames: PathBuf,
    cache: PathBuf,
}

impl Arguments {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let (mut socket, mut frames, mut cache) = (None, None, None);
        while let Some(name) = args.next() {
            let slot = match name.as_str() {
                "--socket" => &mut socket,
                "--frames" => &mut frames,
                "--cache" => &mut cache,
                _ => continue,
            };
            *slot = args.next().map(PathBuf::from);
        }
        let need = |value: Option<PathBuf>, name: &str| value.ok_or(format!("missing {name}"));
        Ok(Self {
            socket: need(socket, "--socket")?,
            frames: need(frames, "--frames")?,
            cache: need(cache, "--cache")?,
        })
    }
}

fn path_string(path: &Path) -> CefString {
    CefString::from(path.to_string_lossy().as_ref())
}

fn main() -> ExitCode {
    // CEF refuses every other call until the API version it was built against is declared.
    let _ = api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let args = cef::args::Args::new();
    let mut app = HostApp::new();

    // Chromium starts its renderer, GPU and utility processes by running this program again with
    // a `--type=` flag. Those processes do their work inside this call and then exit.
    let helper = execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if helper >= 0 {
        return ExitCode::from(helper as u8);
    }

    match run(&args, &mut app) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lynx-browser-host: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &cef::args::Args, app: &mut App) -> Result<(), String> {
    let arguments = Arguments::parse(std::env::args().skip(1))?;
    let link = Connection::connect(&arguments.socket.to_string_lossy()).map_err(|error| {
        format!(
            "cannot reach Lynx at {}: {error}",
            arguments.socket.display()
        )
    })?;
    let frames = FrameBuffer::open(&arguments.frames).map_err(|error| error.to_string())?;

    let (events, inbox) = mpsc::channel::<Event>();
    engine::EVENTS
        .set(events.clone())
        .map_err(|_| "started twice".to_owned())?;

    // Commands arrive on a thread of their own and are handed to the main thread, which is the
    // engine's UI thread.
    let reader_link = link.clone();
    thread::spawn(move || {
        let mut reader = BufReader::new(reader_link);
        while let Ok(Some(command)) = read_message::<ToHost>(&mut reader) {
            if events.send(Event::Command(command)).is_err() {
                return;
            }
        }
        let _ = events.send(Event::LinkClosed);
    });

    let executable_dir = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .parent()
        .map(Path::to_path_buf)
        .ok_or("the host has no folder")?;
    let sandbox = sandbox::decide();
    if sandbox.enabled {
        eprintln!("lynx-browser-host: the sandbox is on");
    } else {
        eprintln!(
            "lynx-browser-host: running WITHOUT the sandbox: {}",
            sandbox.reason
        );
    }
    let settings = Settings {
        no_sandbox: i32::from(!sandbox.enabled),
        windowless_rendering_enabled: 1,
        external_message_pump: 1,
        root_cache_path: path_string(&arguments.cache),
        resources_dir_path: path_string(&executable_dir),
        locales_dir_path: path_string(&executable_dir.join("locales")),
        ..Default::default()
    };
    std::fs::create_dir_all(&arguments.cache).map_err(|error| error.to_string())?;
    if initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(app),
        std::ptr::null_mut(),
    ) != 1
    {
        return Err("the engine did not start".into());
    }

    let started = Instant::now();
    while !engine::context_ready() {
        if started.elapsed() > Duration::from_secs(15) {
            return Err("the engine did not finish starting".into());
        }
        do_message_loop_work();
        thread::sleep(Duration::from_millis(5));
    }

    engine::connect(frames, link);
    engine::announce_ready();
    serve(&inbox);

    engine::close_all_pages();
    let closing = Instant::now();
    while !engine::pages_closed() && closing.elapsed() < Duration::from_secs(3) {
        do_message_loop_work();
        thread::sleep(Duration::from_millis(5));
    }
    cef::shutdown();
    Ok(())
}

/// The main loop: commands from Lynx, and passes of the engine's own message loop.
fn serve(inbox: &mpsc::Receiver<Event>) {
    let mut next_pass = Instant::now();
    loop {
        let wait = next_pass
            .saturating_duration_since(Instant::now())
            .min(MAX_IDLE);
        match inbox.recv_timeout(wait) {
            Ok(Event::Command(ToHost::Shutdown) | Event::LinkClosed) => return,
            Ok(Event::Command(command)) => engine::handle(command),
            Ok(Event::Pump(delay)) => next_pass = next_pass.min(Instant::now() + delay),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if Instant::now() >= next_pass {
            do_message_loop_work();
            next_pass = Instant::now() + MAX_IDLE;
        }
    }
}
