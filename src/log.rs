use std::fs;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::path;
use std::str;

use anyhow::anyhow;
use anyhow::Context;
use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use indexmap::IndexSet;

macro_rules! try_with_context {
    (CONTEXT: $context:expr; $($statement:stmt;)*) => {
        (|| -> io::Result<()> {
            $($statement)*
            Ok(())
        })().with_context(|| $context)
    }
}

#[derive(Debug)]
pub struct Log {
    path: path::PathBuf,
    file: fs::File,
}

impl Log {
    /// Load the log file at `path`, or create one if it doesn't exist.
    ///
    /// WARNING: this function does **not** verify that `path` is a valid log file.
    pub fn load(path: path::PathBuf) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(&parent)
                .with_context(|| anyhow!("Could not create directory: '{}'", parent.display()))?;
        }

        fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&path)
            .with_context(|| anyhow!("Could not open log file: '{}'", path.display()))
            .map(|file| Log { file, path })
    }

    /// Append `entry` to the underlying log file.
    pub fn append<E: AsRef<[u8]>>(&mut self, entry: E) -> anyhow::Result<()> {
        try_with_context! {
            CONTEXT: anyhow!("Could not append to log file: `{}`", self.path.display());
            let entry = entry.as_ref();
            self.file.write_all(entry)?;
            self.file.write_u16::<LittleEndian>(entry.len() as u16)?;
        }
    }
    
    /// Clear the underlying log file.
    pub fn clear(&mut self) -> anyhow::Result<()> {
        try_with_context! {
            CONTEXT: anyhow!("Could not clear log file: `{}`", self.path.display());
            let _ = self.file.seek(io::SeekFrom::Start(0))?;
            self.file.set_len(0)?;
        }
    }

    /// Delete the underlying log file.
    pub fn delete(self) -> anyhow::Result<()> {
        fs::remove_file(&self.path)
            .with_context(|| anyhow!("Could not delete log file: `{}`", self.path.display()))
    }

    /// Finalize changes by flushing to disk.
    pub fn sync(&mut self) -> anyhow::Result<()> {
        try_with_context! {
            CONTEXT: anyhow!("Could not flush log file to disk: `{}`", self.path.display());
            self.file.flush()?;
            self.file.sync_data()?;
        }
    }

    /// Return an ordered set of the latest entries in the log.
    pub fn entries(&mut self, count: usize) -> anyhow::Result<IndexSet<String>> {
        let mut cache = IndexSet::with_capacity(count);
        let mut iter = self.iter();

        // Scan backward through the log
        while let Some(entry) = iter.prev()?  {
            match str::from_utf8(&entry) {
            | Ok(entry) if !cache.contains(&*entry) => {
                cache.insert(entry.to_owned());
            }
            | _ => (),
            }
            if cache.len() == count {
                break;
            }
        }

        Ok(cache)
    }

    /// Return the number of bytes between the beginning of the log file and
    /// the current seek position.
    pub fn position(&mut self) -> anyhow::Result<u64> {
        self.file
            .seek(io::SeekFrom::Current(0))
            .with_context(|| anyhow!("Could not seek in log file: `{}`", self.path.display()))
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
    fn iter(&mut self) -> Iter<'_> {
        Iter {
            buf: Vec::new(),
            pos: self.file
                .seek(io::SeekFrom::End(-2))
                .unwrap_or(0),
            log: &mut self.file
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
}
