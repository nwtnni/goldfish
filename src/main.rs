use std::env;
use std::fs;
use std::io;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::path;

use anyhow::anyhow;
use anyhow::Context;
use fxhash::FxHashSet;

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
            history.write_all(path.as_os_str().as_bytes())?;
            history.write(&[b'\n'])?;
            history.flush()?;
        }
    }
    | None => {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();

        BufReader::new(history)
            .lines()
            .filter_map(Result::ok)
            .collect::<FxHashSet<_>>()
            .into_iter()
            .try_for_each(|path| {
                match path::Path::new(&path).strip_prefix(&home) {
                | Ok(path) if path == path::Path::new("") => writeln!(&mut stdout, "~"),
                | Ok(path) => writeln!(&mut stdout, "~/{}", path.display()),
                | Err(_) => writeln!(&mut stdout, "{}", path),
                }
            })?;

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
