//! Acts as Lynx against a real host and checks what the app relies on: frames, input, text with
//! accents, scale, and shutdown. Run it with the engine installed (`bun run browser:engine`):
//!
//!   cargo run -p lynx-browser-host --example probe [path to lynx-browser-host]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use lynx_browser_protocol::link::{HostLink, LinkEvent};
use lynx_browser_protocol::message::{
    FromHost, KeyKind, MouseButton, PageCommand, PageEvent, PageId, ToHost,
};

const ANIMATION: &str = r#"<!doctype html><body style="margin:0;background:#222"><canvas id="c"></canvas>
<script>
const c = document.getElementById('c'); const fit = () => { c.width = innerWidth; c.height = innerHeight; };
fit(); addEventListener('resize', fit);
const x = c.getContext('2d'); let t = 0;
function draw() { t++; x.fillStyle = `hsl(${t % 360},70%,50%)`; x.fillRect(0, 0, c.width, c.height);
  for (let i = 0; i < 80; i++) { x.fillStyle = `hsl(${Math.random() * 360},80%,60%)`; x.fillRect(Math.random() * c.width, Math.random() * c.height, 60, 60); }
  requestAnimationFrame(draw); }
draw(); setInterval(() => { document.title = String(t); }, 250);
</script></body>"#;

const CLICK: &str = r#"<!doctype html><body style="margin:0"><div style="position:fixed;inset:0;background:#fff"></div>
<script>document.title = 'waiting'; addEventListener('mousedown', () => { document.title = 'clicked'; });</script></body>"#;

const BUTTONS: &str = r#"<!doctype html><title>buttons</title><body style="margin:0">
<div style="height:900px"></div>
<button id="go" style="position:absolute;top:1000px;left:40px;width:140px;height:40px"
  onclick="document.title='go-clicked'">Go on</button>
<select id="s"><option value="a">Alpha</option><option value="b">Beta</option></select>
<script>document.getElementById('s').addEventListener('change', e => { document.title = 'picked:' + e.target.value; });</script>
</body>"#;

const FIELDS: &str = r#"<!doctype html><body style="margin:0;background:#fff">
<input id="a" style="position:absolute;left:20px;top:20px;width:300px;height:40px;font-size:20px">
<input id="b" style="position:absolute;left:20px;top:100px;width:300px;height:40px;font-size:20px">
<script>document.title = 'waiting';
const a = document.getElementById('a'), b = document.getElementById('b');
const tell = () => { document.title = 'a=' + a.value + ';b=' + b.value; };
a.addEventListener('input', tell); b.addEventListener('input', tell);</script></body>"#;

const INPUT: &str = r#"<!doctype html><body style="margin:0;background:#fff">
<input id="i" style="position:absolute;left:20px;top:20px;width:300px;height:40px;font-size:20px">
<script>document.title = 'waiting';
const i = document.getElementById('i');
i.addEventListener('input', () => { document.title = 'text:' + i.value; });
i.addEventListener('keydown', (e) => { if (e.key === 'Enter') document.title = 'enter:' + i.value; });</script></body>"#;

fn data_url(html: &str) -> String {
    let mut url = String::from("data:text/html;charset=utf-8,");
    for byte in html.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            url.push(char::from(byte));
        } else {
            url.push_str(&format!("%{byte:02X}"));
        }
    }
    url
}

/// What the host has said about one page.
#[derive(Default)]
struct Seen {
    title: String,
    frames: u64,
    last_frame: Option<(u32, u32, Vec<u8>)>,
}

struct Probe {
    link: HostLink,
    events: Receiver<LinkEvent>,
    ready: bool,
    /// The page that commands go to, and that `title()`, `frames()` and `last_frame()` describe.
    page: PageId,
    seen: HashMap<PageId, Seen>,
    created: HashSet<PageId>,
    closed: HashSet<PageId>,
    scratch: Vec<u8>,
    failures: u32,
    cdp: HashMap<u32, (bool, serde_json::Value)>,
    next_cdp: u32,
}

impl Probe {
    /// Handles events until `done` says so or the time is up. Frames are copied and handed back.
    fn wait(&mut self, limit: Duration, mut done: impl FnMut(&Probe) -> bool) -> bool {
        let started = Instant::now();
        while started.elapsed() < limit {
            if done(self) {
                return true;
            }
            match self.events.recv_timeout(Duration::from_millis(20)) {
                Ok(LinkEvent::Message(FromHost::Ready)) => self.ready = true,
                Ok(LinkEvent::Message(FromHost::PageCreated { page })) => {
                    self.created.insert(page);
                }
                Ok(LinkEvent::Message(FromHost::PageClosed { page })) => {
                    self.closed.insert(page);
                }
                Ok(LinkEvent::Message(FromHost::Page { page, event })) => self.on_page(page, event),
                Ok(LinkEvent::Closed) => return false,
                Err(_) => {}
            }
        }
        done(self)
    }

    fn on_page(&mut self, page: PageId, event: PageEvent) {
        match event {
            PageEvent::Frame {
                slot,
                width,
                height,
            } => {
                let seen = self.seen.entry(page).or_default();
                seen.frames += 1;
                if self
                    .link
                    .copy_frame(slot, width, height, &mut self.scratch)
                    .is_ok()
                {
                    seen.last_frame = Some((width, height, self.scratch.clone()));
                }
                let _ = self.link.send(&ToHost::FrameDone { slot });
            }
            PageEvent::Title { title } => self.seen.entry(page).or_default().title = title,
            PageEvent::CdpResult { id, ok, result } => {
                self.cdp.insert(id, (ok, result));
            }
            _ => {}
        }
    }

    fn title(&self) -> &str {
        self.seen.get(&self.page).map_or("", |seen| &seen.title)
    }

    fn frames(&self) -> u64 {
        self.seen.get(&self.page).map_or(0, |seen| seen.frames)
    }

    fn last_frame(&self) -> Option<&(u32, u32, Vec<u8>)> {
        self.seen
            .get(&self.page)
            .and_then(|seen| seen.last_frame.as_ref())
    }

    fn check(&mut self, ok: bool, what: &str) {
        println!("  [{}] {what}", if ok { "ok" } else { "FAIL" });
        if !ok {
            self.failures += 1;
        }
    }

    /// One DevTools call, answered or not within five seconds.
    fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Option<(bool, serde_json::Value)> {
        self.next_cdp += 1;
        let id = self.next_cdp;
        self.send(PageCommand::Cdp {
            id,
            method: method.to_owned(),
            params,
        });
        self.wait(Duration::from_secs(5), |probe| probe.cdp.contains_key(&id));
        self.cdp.remove(&id)
    }

    fn send(&self, command: PageCommand) {
        let page = self.page;
        let _ = self.link.send(&ToHost::Page { page, command });
    }

    fn click(&self, x: i32, y: i32) {
        // A real pointer always arrives before it presses.
        self.send(PageCommand::MouseMove {
            x,
            y,
            modifiers: 0,
            leave: false,
        });
        std::thread::sleep(Duration::from_millis(50));
        for pressed in [true, false] {
            self.send(PageCommand::MouseClick {
                x,
                y,
                button: MouseButton::Left,
                pressed,
                clicks: 1,
                modifiers: 0,
            });
        }
    }
}

/// Copy and paste, which need to be asked for by name: type, select all, copy, paste elsewhere.
fn probe_editing(probe: &mut Probe) {
    use lynx_browser_protocol::message::EditCommand;
    probe.send(PageCommand::Navigate {
        url: data_url(FIELDS),
    });
    probe.wait(Duration::from_secs(5), |probe| probe.title() == "waiting");
    probe.send(PageCommand::Focus { focused: true });
    probe.click(60, 40);
    probe.wait(Duration::from_millis(300), |_| false);
    probe.send(PageCommand::Text {
        text: "copiar ção".into(),
    });
    probe.wait(Duration::from_secs(3), |probe| {
        probe.title() == "a=copiar ção;b="
    });
    probe.send(PageCommand::Edit {
        command: EditCommand::SelectAll,
    });
    probe.send(PageCommand::Edit {
        command: EditCommand::Copy,
    });
    probe.wait(Duration::from_millis(300), |_| false);
    probe.click(60, 120);
    probe.wait(Duration::from_millis(300), |_| false);
    probe.send(PageCommand::Edit {
        command: EditCommand::Paste,
    });
    let pasted = probe.wait(Duration::from_secs(3), |probe| {
        probe.title() == "a=copiar ção;b=copiar ção"
    });
    probe.check(
        pasted,
        &format!(
            "select all, copy and paste move text between fields (title is {:?})",
            probe.title()
        ),
    );
}

/// What the agent tools rely on: the accessibility tree, a node's box, a trusted click on it.
fn probe_devtools(probe: &mut Probe) {
    use serde_json::json;
    probe.send(PageCommand::Navigate {
        url: data_url(BUTTONS),
    });
    probe.wait(Duration::from_secs(5), |probe| probe.title() == "buttons");

    let sum = probe.call(
        "Runtime.evaluate",
        json!({ "expression": "6 * 7", "returnByValue": true }),
    );
    let value = sum
        .as_ref()
        .map(|(_, result)| result["result"]["value"].clone());
    probe.check(
        value == Some(json!(42)),
        &format!("Runtime.evaluate answers ({sum:?})"),
    );

    let tree = probe.call("Accessibility.getFullAXTree", json!({}));
    let nodes = tree
        .as_ref()
        .and_then(|(_, result)| result["nodes"].as_array().cloned())
        .unwrap_or_default();
    let button = nodes
        .iter()
        .find(|node| node["role"]["value"] == "button" && node["name"]["value"] == "Go on");
    probe.check(
        button.is_some(),
        &format!(
            "the accessibility tree lists the button ({} nodes)",
            nodes.len()
        ),
    );
    let Some(backend) = button.and_then(|node| node["backendDOMNodeId"].as_i64()) else {
        return;
    };

    probe.call(
        "DOM.scrollIntoViewIfNeeded",
        json!({ "backendNodeId": backend }),
    );
    let boxed = probe.call("DOM.getBoxModel", json!({ "backendNodeId": backend }));
    let quad = boxed
        .as_ref()
        .and_then(|(_, result)| result["model"]["content"].as_array().cloned())
        .unwrap_or_default();
    probe.check(
        quad.len() == 8,
        &format!("the box model comes back ({boxed:?})"),
    );
    if quad.len() == 8 {
        let at = |index: usize| quad[index].as_f64().unwrap_or(0.0);
        let x = ((at(0) + at(2) + at(4) + at(6)) / 4.0) as i32;
        let y = ((at(1) + at(3) + at(5) + at(7)) / 4.0) as i32;
        println!("  button centre at {x},{y}");
        probe.click(x, y);
        let clicked = probe.wait(Duration::from_secs(3), |probe| {
            probe.title() == "go-clicked"
        });
        probe.check(clicked, "a click at the box centre presses the button");
    }
}

/// Two pages in one host: each has its own frames, title, input and DevTools, a hidden page
/// paints nothing while the other goes on, and closing one leaves the other alone.
fn probe_two_pages(probe: &mut Probe) {
    use serde_json::json;
    let size_of = |probe: &Probe, page: PageId| {
        probe
            .seen
            .get(&page)
            .and_then(|seen| seen.last_frame.as_ref())
            .map(|(width, height, _)| (*width, *height))
    };

    probe.page = 1;
    probe.send(PageCommand::Visible { visible: true });
    probe.send(PageCommand::Resize {
        width: 600,
        height: 400,
        scale: 1.0,
    });
    probe.send(PageCommand::Navigate {
        url: data_url(ANIMATION),
    });
    let sized = probe.wait(Duration::from_secs(5), |probe| {
        size_of(probe, 1) == Some((600, 400))
    });
    probe.check(sized, "page 1 paints at 600x400");

    let _ = probe.link.send(&ToHost::CreatePage { page: 2 });
    let made = probe.wait(Duration::from_secs(10), |probe| probe.created.contains(&2));
    probe.check(made, "a second page is created while the first paints");
    probe.page = 2;
    probe.send(PageCommand::Resize {
        width: 300,
        height: 200,
        scale: 1.0,
    });
    probe.send(PageCommand::Visible { visible: true });
    probe.send(PageCommand::Navigate {
        url: data_url(CLICK),
    });
    let titled = probe.wait(Duration::from_secs(5), |probe| probe.title() == "waiting");
    probe.check(titled, "page 2 reports its own title");
    let one_title = probe.seen.get(&1).map(|seen| seen.title.clone());
    probe.check(
        one_title.as_deref() != Some("waiting"),
        "page 1 did not take page 2's title",
    );
    let small = probe.wait(Duration::from_secs(5), |probe| {
        size_of(probe, 2) == Some((300, 200))
    });
    probe.check(small, "page 2 frames come back at its own size, 300x200");
    let kept = size_of(probe, 1) == Some((600, 400));
    probe.check(kept, "page 1 keeps its own size beside page 2");

    probe.click(100, 100);
    let clicked = probe.wait(Duration::from_secs(3), |probe| probe.title() == "clicked");
    probe.check(clicked, "a click on page 2 reaches page 2");
    let one_title = probe.seen.get(&1).map(|seen| seen.title.clone());
    probe.check(
        one_title.as_deref() != Some("clicked"),
        "and does not reach page 1",
    );

    // DevTools: each page answers for itself. Page 1 defines `c` (its canvas), page 2 does not.
    let on_two = probe.call(
        "Runtime.evaluate",
        json!({ "expression": "typeof c", "returnByValue": true }),
    );
    probe.page = 1;
    let on_one = probe.call(
        "Runtime.evaluate",
        json!({ "expression": "typeof c", "returnByValue": true }),
    );
    let kind = |answer: &Option<(bool, serde_json::Value)>| {
        answer
            .as_ref()
            .map(|(_, result)| result["result"]["value"].clone())
    };
    probe.check(
        kind(&on_one) == Some(json!("object")) && kind(&on_two) == Some(json!("undefined")),
        &format!("DevTools calls reach the page they name ({on_one:?} / {on_two:?})"),
    );

    // A hidden page paints nothing; the shown one goes on, from the same frame slots.
    probe.page = 2;
    probe.send(PageCommand::Navigate {
        url: data_url(ANIMATION),
    });
    probe.wait(Duration::from_secs(3), |probe| probe.frames() > 5);
    probe.page = 1;
    probe.send(PageCommand::Visible { visible: false });
    probe.wait(Duration::from_millis(500), |_| false);
    let before = |probe: &Probe, page| probe.seen.get(&page).map_or(0, |seen| seen.frames);
    let (one_before, two_before) = (before(probe, 1), before(probe, 2));
    probe.wait(Duration::from_secs(2), |_| false);
    let (one_gained, two_gained) = (before(probe, 1) - one_before, before(probe, 2) - two_before);
    probe.check(
        one_gained <= 3,
        &format!("a hidden page paints nothing ({one_gained} frames in 2 s)"),
    );
    probe.check(
        two_gained > 20,
        &format!("the page that is shown keeps painting ({two_gained} frames in 2 s)"),
    );
    save_page(probe, 2, "probe-two-pages-second");

    probe.send(PageCommand::Visible { visible: true });
    let resumed = before(probe, 1);
    let back = probe.wait(Duration::from_secs(3), |probe| {
        before(probe, 1) > resumed + 5
    });
    probe.check(back, "a page shown again paints again");
    save_page(probe, 1, "probe-two-pages-first");

    // Closing one page leaves the other alone, and commands for a closed page are ignored.
    let _ = probe.link.send(&ToHost::ClosePage { page: 2 });
    let closed = probe.wait(Duration::from_secs(5), |probe| probe.closed.contains(&2));
    probe.check(closed, "closing page 2 is reported");
    probe.page = 2;
    probe.send(PageCommand::Navigate {
        url: data_url(CLICK),
    });
    probe.page = 1;
    let still = before(probe, 1);
    let going = probe.wait(Duration::from_secs(3), |probe| before(probe, 1) > still + 5);
    probe.check(going, "page 1 keeps painting after page 2 closed");
    let _ = probe.link.send(&ToHost::ClosePage { page: 99 });
    let unknown = probe.wait(Duration::from_secs(3), |probe| probe.closed.contains(&99));
    probe.check(unknown, "closing a page that does not exist is answered");

    // A page made after another closed works the same.
    let _ = probe.link.send(&ToHost::CreatePage { page: 3 });
    let third = probe.wait(Duration::from_secs(10), |probe| probe.created.contains(&3));
    probe.check(third, "a page made after another one closed is created");
    probe.page = 3;
    probe.send(PageCommand::Resize {
        width: 200,
        height: 150,
        scale: 1.0,
    });
    probe.send(PageCommand::Navigate {
        url: data_url(CLICK),
    });
    let third_title = probe.wait(Duration::from_secs(5), |probe| probe.title() == "waiting");
    probe.check(third_title, "and it reports its own title");
    let _ = probe.link.send(&ToHost::ClosePage { page: 3 });
    probe.wait(Duration::from_secs(5), |probe| probe.closed.contains(&3));
    probe.page = 1;
}

fn save_page(probe: &mut Probe, page: PageId, name: &str) {
    let before = std::mem::replace(&mut probe.page, page);
    save(probe, name);
    probe.page = before;
}

fn save(probe: &Probe, name: &str) {
    if let Some((width, height, bgra)) = probe.last_frame() {
        let rgba: Vec<u8> = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], 255])
            .collect();
        let _ = std::fs::create_dir_all("out");
        if let Some(image) = image::RgbaImage::from_raw(*width, *height, rgba) {
            let _ = image.save(format!("out/{name}.png"));
        }
    }
}

fn ticks(pid: u32) -> u64 {
    let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
        return 0;
    };
    let Some((_, rest)) = stat.rsplit_once(')') else {
        return 0;
    };
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let read = |index: usize| {
        fields
            .get(index)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
    };
    read(11) + read(12)
}

fn main() {
    let host = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("LYNX_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".lynx")
                });
            home.join("engine/current/lynx-browser-host")
        });
    println!("[probe against {}]", host.display());

    let (sender, events) = mpsc::channel();
    let started = Instant::now();
    let link = HostLink::spawn(&host, move |event| {
        let _ = sender.send(event);
    })
    .unwrap_or_else(|error| panic!("could not start the host: {error}"));
    let pid = link.pid();
    let mut probe = Probe {
        link,
        events,
        ready: false,
        page: 1,
        seen: HashMap::new(),
        created: HashSet::new(),
        closed: HashSet::new(),
        scratch: Vec::new(),
        failures: 0,
        cdp: HashMap::new(),
        next_cdp: 0,
    };

    let ready = probe.wait(Duration::from_secs(30), |probe| probe.ready);
    probe.check(
        ready,
        &format!(
            "the host is ready after {} ms",
            started.elapsed().as_millis()
        ),
    );
    let _ = probe.link.send(&ToHost::CreatePage { page: 1 });
    let made = probe.wait(Duration::from_secs(10), |probe| probe.created.contains(&1));
    probe.check(made, "the first page is created on request");
    probe.send(PageCommand::Resize {
        width: 800,
        height: 500,
        scale: 1.0,
    });
    probe.send(PageCommand::Visible { visible: true });
    probe.send(PageCommand::Navigate {
        url: data_url(ANIMATION),
    });
    let first = probe.wait(Duration::from_secs(20), |probe| probe.frames() > 0);
    probe.check(
        first,
        &format!("first frame after {} ms", started.elapsed().as_millis()),
    );

    // Frame rate and cost under a busy animation.
    let (frames_before, cpu_before, timer) = (probe.frames(), ticks(pid), Instant::now());
    probe.wait(Duration::from_secs(5), |_| false);
    let seconds = timer.elapsed().as_secs_f64();
    let fps = (probe.frames() - frames_before) as f64 / seconds;
    let cpu = (ticks(pid) - cpu_before) as f64 / 100.0 / seconds * 100.0;
    println!(
        "  frames over the link: {fps:.1}/s, host main process {cpu:.0}% CPU, page counter at {}",
        probe.title()
    );
    probe.check(fps > 20.0, "at least 20 frames per second reach Lynx");

    if let Some((width, height, bgra)) = &probe.last_frame() {
        let rgba: Vec<u8> = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], 255])
            .collect();
        let _ = std::fs::create_dir_all("out");
        if let Some(image) = image::RgbaImage::from_raw(*width, *height, rgba) {
            let _ = image.save("out/probe-frame.png");
            println!("  last frame {width}x{height} saved to out/probe-frame.png");
        }
    }

    probe.send(PageCommand::Navigate {
        url: data_url(CLICK),
    });
    probe.wait(Duration::from_secs(5), |probe| probe.title() == "waiting");
    probe.click(100, 100);
    let clicked = probe.wait(Duration::from_secs(3), |probe| probe.title() == "clicked");
    probe.check(clicked, "a click reaches the page");

    probe.send(PageCommand::Navigate {
        url: data_url(INPUT),
    });
    probe.wait(Duration::from_secs(5), |probe| probe.title() == "waiting");
    probe.send(PageCommand::Focus { focused: true });
    probe.click(60, 40);
    // A person takes longer than a few milliseconds to start typing.
    probe.wait(Duration::from_millis(300), |_| false);
    probe.send(PageCommand::Text {
        text: "ção ã".into(),
    });
    let typed = probe.wait(Duration::from_secs(3), |probe| {
        probe.title() == "text:ção ã"
    });
    probe.check(
        typed,
        &format!(
            "text with accents arrives intact (title is {:?})",
            probe.title()
        ),
    );
    save(&probe, "probe-input-after-text");
    // The same field through plain key events: tells the input-method path from the key path.
    for (code, character) in [(65, 'a'), (66, 'b')] {
        probe.send(PageCommand::Key {
            kind: KeyKind::Down,
            windows_code: code,
            native_code: code,
            modifiers: 0,
            character: character as u16,
        });
        probe.send(PageCommand::Key {
            kind: KeyKind::Up,
            windows_code: code,
            native_code: code,
            modifiers: 0,
            character: 0,
        });
    }
    let keys = probe.wait(Duration::from_secs(3), |probe| {
        probe.title().ends_with("ab")
    });
    probe.check(
        keys,
        &format!(
            "plain key events type into the field (title is {:?})",
            probe.title()
        ),
    );
    save(&probe, "probe-input-after-keys");
    probe.send(PageCommand::Key {
        kind: KeyKind::Down,
        windows_code: 13,
        native_code: 36,
        modifiers: 0,
        character: 13,
    });
    probe.send(PageCommand::Key {
        kind: KeyKind::Up,
        windows_code: 13,
        native_code: 36,
        modifiers: 0,
        character: 0,
    });
    let enter = probe.wait(Duration::from_secs(3), |probe| {
        probe.title().starts_with("enter:")
    });
    probe.check(enter, "the Enter key reaches the page");

    probe_devtools(&mut probe);
    probe_editing(&mut probe);

    // The navigation matters: the host applies a new scale to a page that loads after it, not to one
    // already showing.
    probe.send(PageCommand::Navigate {
        url: data_url(ANIMATION),
    });
    probe.send(PageCommand::Resize {
        width: 400,
        height: 250,
        scale: 2.0,
    });
    let scaled = probe.wait(Duration::from_secs(5), |probe| {
        probe
            .last_frame()
            .is_some_and(|(w, h, _)| (*w, *h) == (800, 500))
    });
    probe.check(
        scaled,
        "a 400x250 view at scale 2 comes back as 800x500 pixels",
    );

    probe_two_pages(&mut probe);

    drop(probe);
    std::thread::sleep(Duration::from_millis(500));
    let gone = !std::path::Path::new(&format!("/proc/{pid}")).exists();
    println!(
        "  [{}] the host is gone after Lynx lets go of it",
        if gone { "ok" } else { "FAIL" }
    );
}
