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

    let mut history = load_history()?;

    match env::args().nth(1) {
    | Some(path) => {
        if let Ok(path) = path::Path::new(&path).canonicalize() {
            if path.is_dir() {
                let path = path.as_os_str().as_bytes();
                history.write_all(path)?;
                history.write_u16::<LittleEndian>(path.len() as u16)?;
                history.flush()?;
            }
        }
    }
    | None => {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        let mut paths = IndexSet::with_hasher(FxBuildHasher::default());

        let mut buf = Vec::new();
        let mut pos = history
            .seek(io::SeekFrom::End(-2))
            .unwrap_or(0);

        // Scan backward through the log
        while pos > 0 && paths.len() < CACHE_SIZE {

            // |   /|0x01|0x00|   /|   b|   a|   r|0x04|0x00|
            //                                    ^

            let len = history.read_u16::<LittleEndian>()?;

            // |   /|0x01|0x00|   /|   b|   a|   r|0x04|0x00|
            //                                              ^

            history.seek(io::SeekFrom::Current(-2 - (len as i64)))?;

            // |   /|0x01|0x00|   /|   b|   a|   r|0x04|0x00|
            //                ^

            buf.clear();
            buf.resize(len as usize, 0);
            history.read_exact(&mut buf[..])?;

            // |   /|0x01|0x00|   /|   b|   a|   r|0x04|0x00|
            //                                    ^

            pos = history
                .seek(io::SeekFrom::Current(-2 -(len as i64)))
                .unwrap_or(0);

            // |   /|0x01|0x00|   /|   b|   a|   r|0x04|0x00|
            //      ^

            if let Ok(path) = str::from_utf8(&buf) {
                if !paths.contains(&*path) {
                    paths.insert(path.to_owned());
                }
            }
        }

        // If the excess memory usage is above the threshold, then compact
        // the log by rewriting only the relevant entries.
        let compact = pos > COMPACTION_THRESHOLD;

        if compact {
            history.seek(io::SeekFrom::Start(0))?;
            history.set_len(0)?;
        }

        paths.into_iter()
            .rev()
            .try_for_each(|path| {
                // Write to log
                if compact {
                    history.write_all(path.as_bytes())?;
                    history.write_u16::<LittleEndian>(path.len() as u16)?;
                }

                // Write to stdout for consumption by other tools
                writeln!(&mut stdout, "{}", path)
            })?;

        if compact {
            history.flush()?;
        }

        stdout.flush()?;
    }
    }

    Ok(())
}

fn load_history() -> io::Result<fs::File> {
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
}
