//! Frames travel through a memory-mapped file that both processes map. The host writes a frame
//! into a free slot and announces it on the socket; Lynx copies it out and answers with
//! `FrameDone`. A frame is the whole view every time, so a dropped frame costs nothing but time.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use memmap2::MmapMut;

/// Two slots: the host paints into one while Lynx still copies out of the other.
pub const SLOTS: usize = 2;
/// The largest view in device pixels. Bigger frames are dropped rather than grow the file.
pub const MAX_WIDTH: u32 = 4096;
pub const MAX_HEIGHT: u32 = 2560;
const BYTES_PER_PIXEL: usize = 4;
pub const SLOT_BYTES: usize = MAX_WIDTH as usize * MAX_HEIGHT as usize * BYTES_PER_PIXEL;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("frame memory: {0}")]
    Io(#[from] io::Error),
    #[error("slot {0} does not exist")]
    BadSlot(usize),
    #[error("a {0} byte frame does not fit a slot of {SLOT_BYTES} bytes")]
    TooLarge(usize),
}

pub struct FrameBuffer {
    map: MmapMut,
}

impl FrameBuffer {
    /// Creates the file at `path`, sized for every slot. Untouched pages cost no memory.
    pub fn create(path: &Path) -> Result<Self, FrameError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)?;
        file.set_len((SLOTS * SLOT_BYTES) as u64)?;
        Self::map(&file)
    }

    /// Maps a file made by [`FrameBuffer::create`] in another process.
    pub fn open(path: &Path) -> Result<Self, FrameError> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Self::map(&file)
    }

    fn map(file: &File) -> Result<Self, FrameError> {
        // SAFETY: the file is private to the two processes that cooperate through the protocol:
        // the host writes only slots Lynx has released, and Lynx reads only slots it was told of.
        let map = unsafe { MmapMut::map_mut(file)? };
        Ok(Self { map })
    }

    fn range(slot: usize, length: usize) -> Result<std::ops::Range<usize>, FrameError> {
        if slot >= SLOTS {
            return Err(FrameError::BadSlot(slot));
        }
        if length > SLOT_BYTES {
            return Err(FrameError::TooLarge(length));
        }
        let start = slot * SLOT_BYTES;
        Ok(start..start + length)
    }

    pub fn write(&mut self, slot: usize, pixels: &[u8]) -> Result<(), FrameError> {
        let range = Self::range(slot, pixels.len())?;
        self.map[range].copy_from_slice(pixels);
        Ok(())
    }

    pub fn read(&self, slot: usize, length: usize) -> Result<&[u8], FrameError> {
        let range = Self::range(slot, length)?;
        Ok(&self.map[range])
    }
}

/// Which slots the host may paint into. A frame with no free slot is dropped, and remembered, so
/// the host asks the engine to paint again once a slot comes back.
#[derive(Debug, Default)]
pub struct SlotPool {
    busy: [bool; SLOTS],
    dropped: bool,
}

impl SlotPool {
    pub fn acquire(&mut self) -> Option<usize> {
        let free = self.busy.iter().position(|busy| !busy);
        match free {
            Some(slot) => self.busy[slot] = true,
            None => self.dropped = true,
        }
        free
    }

    /// Gives a slot back. True when a frame was dropped meanwhile and must be painted again.
    pub fn release(&mut self, slot: usize) -> bool {
        if let Some(busy) = self.busy.get_mut(slot) {
            *busy = false;
        }
        std::mem::take(&mut self.dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_written_by_one_mapping_is_read_by_another() {
        let dir = std::env::temp_dir().join(format!("lynx-frames-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("frames");
        let mut host = FrameBuffer::create(&path).unwrap();
        let app = FrameBuffer::open(&path).unwrap();
        host.write(1, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        assert_eq!(app.read(1, 8).unwrap(), &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(app.read(0, 4).unwrap(), &[0, 0, 0, 0]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn slots_and_sizes_are_checked() {
        let dir = std::env::temp_dir().join(format!("lynx-frames-bounds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut buffer = FrameBuffer::create(&dir.join("frames")).unwrap();
        assert!(matches!(
            buffer.write(SLOTS, &[0]),
            Err(FrameError::BadSlot(_))
        ));
        assert!(matches!(
            buffer.read(0, SLOT_BYTES + 1),
            Err(FrameError::TooLarge(_))
        ));
        assert!(buffer.write(0, &vec![7; SLOT_BYTES]).is_ok());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn creating_over_an_existing_file_is_refused() {
        let dir = std::env::temp_dir().join(format!("lynx-frames-twice-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("frames");
        let _first = FrameBuffer::create(&path).unwrap();
        assert!(FrameBuffer::create(&path).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_pool_hands_out_each_slot_once() {
        let mut pool = SlotPool::default();
        let first = pool.acquire().unwrap();
        let second = pool.acquire().unwrap();
        assert_ne!(first, second);
        assert_eq!(pool.acquire(), None);
    }

    #[test]
    fn releasing_after_a_dropped_frame_asks_for_a_repaint() {
        let mut pool = SlotPool::default();
        let first = pool.acquire().unwrap();
        let _second = pool.acquire().unwrap();
        assert_eq!(pool.acquire(), None); // dropped
        assert!(
            pool.release(first),
            "the dropped frame has to be painted again"
        );
        assert!(!pool.release(first), "and only once");
        assert!(pool.acquire().is_some());
    }

    #[test]
    fn releasing_without_a_drop_asks_for_nothing() {
        let mut pool = SlotPool::default();
        let slot = pool.acquire().unwrap();
        assert!(!pool.release(slot));
    }

    #[test]
    fn an_unknown_slot_is_ignored() {
        let mut pool = SlotPool::default();
        assert!(!pool.release(9));
    }
}
