use std::ffi;
use std::io;
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::str;
use std::path;

use anyhow::anyhow;
use anyhow::Context;
use indexmap::IndexSet;
use structopt::StructOpt;

mod log;

/// Implements a basic LRU cache for file paths or text.
#[derive(StructOpt)]
#[structopt(name = "lru")]
struct Opt {
    /// Maximum entries in the cache
    #[structopt(short, long, default_value = "100")]
    count: usize,

    /// Directory to store cache files
    #[structopt(short, long)]
    dir: Option<path::PathBuf>,

    /// Cache file (relative to `dir`) to read from or write to
    #[structopt(short, long)]
    file: Option<path::PathBuf>,

    /// Maximum bytes of stale cache entries before compaction
    #[structopt(short = "b", long, default_value = "1024")]
    threshold: u64,

    /// Validation to filter cache entries
    #[structopt(short, long)]
    r#type: Option<Type>,

    /// Update the cache with `entry` or else report all entries in the cache
    entry: Option<String>,
}

#[derive(Copy, Clone)]
enum Type {
    Directory,
    File,
}

impl Type {
    fn validate(self, path: &path::Path) -> bool {
        match self {
        | Type::Directory => path.is_dir(),
        | Type::File => path.is_file(),
        }
    }
}

impl str::FromStr for Type {
    type Err = anyhow::Error;
    fn from_str(string: &str) -> anyhow::Result<Self> {
        match string {
        | "d" | "dir" | "directory" => Ok(Type::Directory),
        | "f" | "file" => Ok(Type::File),
        | other => Err(anyhow!("Unknown type: `{}`", other))
            .context(r#"Expected one of ["d", "dir", "directory", "f", "file"]"#)
        }
    }
}

fn main() -> anyhow::Result<()> {

    let opt = Opt::from_args();

    let mut path = opt
        .dir
        .or_else(dirs::data_local_dir)
        .ok_or_else(|| anyhow!("Could not retrieve data local diretory"))
        .context("Try passing the `-d` or `--dir` flag to manually specify a directory")?;

    path.push("lru");

    match (opt.file, opt.r#type) {
    | (None, None) => path.push("history"),
    | (None, Some(Type::Directory)) => path.push("directories"),
    | (None, Some(Type::File)) => path.push("files"),
    | (Some(file), _) => path.push(file),
    }

    let log = log::Log::load(&path)?;

    match opt.entry {
    | Some(entry) => append(log, entry, opt.r#type)
        .with_context(|| anyhow!("Could not write to log file: `{}`", path.display())),
    | None => report(log, opt.count, opt.threshold)
        .with_context(|| anyhow!("Could not report from log file: `{}`", path.display())),
    }
}

fn append(mut log: log::Log, entry: String, r#type: Option<Type>) -> io::Result<()> {
    let entry = match r#type {
    | None => ffi::OsString::from(entry),
    | Some(r#type) => {
        match path::Path::new(&entry).canonicalize() {
        | Ok(path) if r#type.validate(&path) => path.into_os_string(),
        | Ok(_) | Err(_) => return Ok(()),
        }
    }
    };
    log.append(entry.as_bytes())?;
    log.sync()?;
    Ok(())
}

fn report(mut log: log::Log, count: usize, threshold: u64) -> io::Result<()> {

    let mut cache = IndexSet::new();
    let mut entries = log.iter();

    // Scan backward through the log
    while let Some(entry) = entries.prev()?  {
        match str::from_utf8(&entry) {
        | Ok(entry) if !cache.contains(&*entry) => {
            cache.insert(entry.to_owned());
        }
        | _ => (),
        }
        if cache.len() > count {
            break;
        }
    }

    // Write to `stdout`
    {
        let stdout = io::stdout();
        let mut stdout = stdout.lock();
        for entry in &cache {
            writeln!(&mut stdout, "{}", entry)?;
        }
        stdout.flush()?;
    }

    // Compact the log by rewriting only the relevant entries
    if entries.len() > threshold {
        log.clear()?;
        for entry in cache.into_iter().rev() {
            log.append(entry)?;
        }
        log.sync()?;
    }

    Ok(())
}
