//! The connection between Lynx and the host: a local socket, which is a Unix domain socket on
//! Linux and macOS and a named pipe on Windows (`interprocess` papers over the difference). One
//! side listens and the host connects to the address it is given on its command line.

use std::io::{self, Read, Write};
use std::path::Path;
use std::sync::Arc;

use interprocess::local_socket::traits::{Listener as _, Stream as _};
#[cfg(unix)]
use interprocess::local_socket::{GenericFilePath, ToFsName};
#[cfg(windows)]
use interprocess::local_socket::{GenericNamespaced, ToNsName};
use interprocess::local_socket::{ListenerNonblockingMode, ListenerOptions, Name, Stream};

/// The address as a name the socket library takes: a file for a Unix socket, a pipe name on Windows.
fn name_of(address: &str) -> io::Result<Name<'_>> {
    #[cfg(unix)]
    {
        address.to_fs_name::<GenericFilePath>()
    }
    #[cfg(windows)]
    {
        address.to_ns_name::<GenericNamespaced>()
    }
}

/// An address nobody else can guess. A named pipe is not under a folder only its owner can enter,
/// so on Windows its name is what keeps another program from connecting.
#[cfg(windows)]
fn unguessable() -> String {
    use std::hash::{BuildHasher, Hasher};
    let random = || {
        std::collections::hash_map::RandomState::new()
            .build_hasher()
            .finish()
    };
    format!(
        "lynx-browser-{}-{:016x}{:016x}",
        std::process::id(),
        random(),
        random()
    )
}

pub struct Listener {
    inner: interprocess::local_socket::Listener,
    address: String,
}

impl Listener {
    /// Starts listening. On Unix the socket is a file in `dir`, which only its owner can enter.
    pub fn bind(dir: &Path) -> io::Result<Self> {
        #[cfg(unix)]
        let address = dir.join("link.sock").to_string_lossy().into_owned();
        #[cfg(windows)]
        let address = {
            let _ = dir;
            unguessable()
        };
        let inner = ListenerOptions::new()
            .name(name_of(&address)?)
            .create_sync()?;
        inner.set_nonblocking(ListenerNonblockingMode::Accept)?;
        Ok(Self { inner, address })
    }

    /// What the host is told to connect to.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The host that has connected, or None when none has yet.
    pub fn try_accept(&self) -> io::Result<Option<Connection>> {
        match self.inner.accept() {
            Ok(stream) => Ok(Some(Connection(Arc::new(stream)))),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error),
        }
    }
}

/// One end of the link. Clones share the connection, so one can read on its own thread while
/// another writes.
#[derive(Clone)]
pub struct Connection(Arc<Stream>);

impl Connection {
    pub fn connect(address: &str) -> io::Result<Self> {
        Ok(Self(Arc::new(Stream::connect(name_of(address)?)?)))
    }
}

impl Read for Connection {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        (&*self.0).read(buffer)
    }
}

impl Write for Connection {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        (&*self.0).write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        (&*self.0).flush()
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::{BufRead, BufReader};

    use super::*;

    #[test]
    fn a_host_connects_to_the_address_and_both_sides_can_talk_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let listener = Listener::bind(dir.path()).unwrap();
        assert!(listener.try_accept().unwrap().is_none(), "nobody yet");

        let address = listener.address().to_owned();
        let host = std::thread::spawn(move || {
            let mut connection = Connection::connect(&address).unwrap();
            writeln!(connection, "hello from the host").unwrap();
            let mut line = String::new();
            BufReader::new(connection.clone())
                .read_line(&mut line)
                .unwrap();
            line
        });

        let mut lynx = loop {
            if let Some(connection) = listener.try_accept().unwrap() {
                break connection;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let mut line = String::new();
        BufReader::new(lynx.clone()).read_line(&mut line).unwrap();
        assert_eq!(line, "hello from the host\n");
        writeln!(lynx, "hello from lynx").unwrap();
        assert_eq!(host.join().unwrap(), "hello from lynx\n");
    }

    #[test]
    fn connecting_to_nothing_fails() {
        let dir = tempfile::tempdir().unwrap();
        let address = dir
            .path()
            .join("missing.sock")
            .to_string_lossy()
            .into_owned();
        assert!(Connection::connect(&address).is_err());
    }
}
