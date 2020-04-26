use std::env;
use std::fs;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::str;
use std::path;

use anyhow::anyhow;
use anyhow::Context;
use byteorder::LittleEndian;
use byteorder::ReadBytesExt;
use byteorder::WriteBytesExt;
use fxhash::FxBuildHasher;
use indexmap::IndexSet;

fn main() -> anyhow::Result<()> {

    let home = dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not find home directory"))?;

    let home = home
        .canonicalize()
        .with_context(|| anyhow!("Could not canonicalize home directory: '{}'", home.display()))?;

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

        let mut buf = Vec::new();
        let mut pos = history.seek(io::SeekFrom::End(0))?;
        let mut paths: IndexSet<String, _> = IndexSet::with_hasher(FxBuildHasher::default());

        while pos > 0 && paths.len() < 5 {
            history.seek(io::SeekFrom::Current(-2))?;

            let len = history.read_u16::<LittleEndian>()?;

            buf.clear();
            buf.resize(len as usize, 0);

            history.seek(io::SeekFrom::Current(-2 - (len as i64)))?;
            history.read_exact(&mut buf[..])?;
            pos = history.seek(io::SeekFrom::Current(-(len as i64)))?;

            if let Ok(path) = str::from_utf8(&buf) {
                if !paths.contains(&*path) {
                    paths.insert(path.to_owned());
                }
            }
        }

        // If the excess space is above the threshold, then compact
        // the log by rewriting only the relevant entries.
        let compact = pos > 2u64.pow(10);

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
                match path::Path::new(&path).strip_prefix(&home) {
                | Ok(path) if path == path::Path::new("") => writeln!(&mut stdout, "~"),
                | Ok(path) => writeln!(&mut stdout, "~/{}", path.display()),
                | Err(_) => writeln!(&mut stdout, "{}", path),
                }
            })?;

        if compact {
            history.flush()?;
        }

        stdout.flush()?;
    }
    }

    Ok(())
}

fn load_history() -> anyhow::Result<fs::File> {
    let mut path = dirs::data_local_dir()
        .ok_or_else(|| anyhow!("Could not find local data directory"))?;

    path.push("dvd");

    fs::create_dir_all(&path)
        .with_context(|| anyhow!("Could not create local data directory: '{}'", path.display()))?;

    path.push("history");

    fs::OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(&path)
        .with_context(|| anyhow!("Could not open local data directory: '{}'", path.display()))
}
