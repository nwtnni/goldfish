use std::fs;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path;

use anyhow::anyhow;
use anyhow::Context;
use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;

#[derive(Debug)]
pub struct Log(fs::File);

impl Log {
    /// Load the log file at `path`, or create one if it doesn't exist.
    ///
    /// WARNING: this function does **not** verify that `path` is a valid log file.
    pub fn load<P: AsRef<path::Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(&parent)
                .with_context(|| anyhow!("Could not create directory: '{}'", parent.display()))?;
        }

        fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&path)
            .map(Log)
            .with_context(|| anyhow!("Could not open log file: '{}'", path.display()))
    }

    /// Append `entry` to the underlying log file.
    pub fn append<E: AsRef<[u8]>>(&mut self, entry: E) -> io::Result<()> {
        let entry = entry.as_ref();
        self.0.write_all(entry)?;
        self.0.write_u16::<LittleEndian>(entry.len() as u16)?;
        Ok(())
    }
    
    /// Clear the underlying log file.
    pub fn clear(&mut self) -> io::Result<()> {
        self.0.seek(io::SeekFrom::Start(0))?;
        self.0.set_len(0)?;
        Ok(())
    }

    /// Finalize changes by flushing to disk.
    pub fn sync(&mut self) -> io::Result<()> {
        self.0.flush()?;
        self.0.sync_data()?;
        Ok(())
    }

    /// Return a pseudo-iterator over this log's entries in **reverse** order,
    /// assuming this was created from a valid log file.
    ///
    /// The iterator lends out slices from an internal buffer to reduce
    /// allocations, and therefore can't implement the existing [`Iterator`][it]
    /// trait properly. We'll need [generic associated types][gat] before we
    /// can express this at the trait level.
    ///
    /// That said, you can traverse this iterator manually using `while let` syntax:
    ///
    /// ```no_compile
    /// while let Some(prev) = iter.prev()? {
    ///     // `prev` is a reference scoped to this block
    /// }
    /// ```
    ///
    /// [it]: https://doc.rust-lang.org/std/iter/trait.Iterator.html
    /// [gat]: https://github.com/rust-lang/rfcs/blob/master/text/1598-generic_associated_types.md
    pub fn iter(&mut self) -> Iter<'_> {
        Iter {
            buf: Vec::new(),
            pos: self.0
                .seek(io::SeekFrom::End(-2))
                .unwrap_or(0),
            log: &mut self.0
        }
    }
}

/// Implements a reverse iterator over the underlying log file.
pub struct Iter<'h> {
    buf: Vec<u8>,
    pos: u64,
    log: &'h mut fs::File,
}

impl<'h> Iter<'h> {
    /// Read the previous log entry.
    pub fn prev(&mut self) -> io::Result<Option<&[u8]>> {
        if self.pos == 0 {
            return Ok(None);
        }

        // |   /|0x01|0x00|   /|   b|   a|   r|0x04|0x00|
        //                                    ^

        let len = self.log.read_u16::<LittleEndian>()?;

        // |   /|0x01|0x00|   /|   b|   a|   r|0x04|0x00|
        //                                              ^

        self.log.seek(io::SeekFrom::Current(-2 - (len as i64)))?;

        // |   /|0x01|0x00|   /|   b|   a|   r|0x04|0x00|
        //                ^

        self.buf.clear();
        self.buf.resize(len as usize, 0);
        self.log.read_exact(&mut self.buf[..])?;

        // |   /|0x01|0x00|   /|   b|   a|   r|0x04|0x00|
        //                                    ^

        self.pos = self.log
            .seek(io::SeekFrom::Current(-2 - (len as i64)))
            .unwrap_or(0);

        // |   /|0x01|0x00|   /|   b|   a|   r|0x04|0x00|
        //      ^
        
        Ok(Some(&self.buf))
    }

    /// Return the number of bytes between the beginning of the log file and
    /// the current position.
    pub fn len(&self) -> u64 {
        self.pos
    }
}
