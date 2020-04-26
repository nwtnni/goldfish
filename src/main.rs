use std::env;
use std::fs;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::str;
use std::path;

use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use fxhash::FxBuildHasher;
use indexmap::IndexSet;

/// Maximum number of directories in the LRU cache that this tool will output.
const CACHE_SIZE: usize = 100;

/// Maximum size (in bytes) of stale log entries before the log is rewritten.
const COMPACTION_THRESHOLD: u64 = 8 * 1024;

fn main() -> io::Result<()> {

    let mut history = History::load()?;

    match env::args().nth(1) {
    | Some(path) => {
        match path::Path::new(&path).canonicalize() {
        | Ok(path) if path.is_dir() => {
            history.insert(path.as_os_str().as_bytes())?;
            history.flush()?;
        }
        | _ => (),
        }
    }
    | None => {
        let mut paths = IndexSet::with_hasher(FxBuildHasher::default());
        let mut entries = history.iter_mut();

        // Scan backward through the log
        while let Some(entry) = entries.next()? {
            match str::from_utf8(&entry) {
            | Ok(path) if !paths.contains(&*path) => {
                paths.insert(path.to_owned());
            }
            | _ => (),
            }

            if paths.len() > CACHE_SIZE {
                break;
            }
        }

        // Write to stdout for consumption by other tools
        {
            let stdout = io::stdout();
            let mut stdout = stdout.lock();
            for path in &paths {
                writeln!(&mut stdout, "{}", path)?;
            }
            stdout.flush()?;
        }

        // If the excess memory usage is above the threshold, then compact
        // the log by rewriting only the relevant entries.
        if entries.len() > COMPACTION_THRESHOLD {
            history.clear()?;
            for path in paths.into_iter().rev() {
                history.insert(path)?;
            }
            history.flush()?;
        }
    }
    }

    Ok(())
}

struct History {
    log: fs::File,
}

impl History {
    pub fn load() -> io::Result<Self> {
        let mut path = dirs::data_local_dir()
            .expect("Could not find local data directory");
        path.push("dvd");
        fs::create_dir_all(&path)?;
        path.push("history");
        fs::OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&path)
            .map(|file| History { log: file })
    }

    pub fn insert<P: AsRef<[u8]>>(&mut self, path: P) -> io::Result<()> {
        let path = path.as_ref();
        self.log.write_all(path)?;
        self.log.write_u16::<LittleEndian>(path.len() as u16)?;
        Ok(())
    }
    
    pub fn clear(&mut self) -> io::Result<()> {
        self.log.seek(io::SeekFrom::Start(0))?;
        self.log.set_len(0)?;
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.log.flush()
    }

    pub fn iter_mut(&mut self) -> HistoryIter<'_> {
        HistoryIter {
            buf: Vec::new(),
            pos: self.log
                .seek(io::SeekFrom::End(-2))
                .unwrap_or(0),
            log: &mut self.log
        }
    }
}

struct HistoryIter<'h> {
    buf: Vec<u8>,
    pos: u64,
    log: &'h mut fs::File,
}

impl<'h> HistoryIter<'h> {
    pub fn next(&mut self) -> io::Result<Option<&[u8]>> {
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

    pub fn len(&self) -> u64 {
        self.pos
    }
}
