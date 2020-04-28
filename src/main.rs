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
#[structopt(name = "goldfish")]
struct Opt {
    /// Cache to read from or write to
    #[structopt(short, long)]
    cache: Option<String>,

    /// Directory to store cache files
    #[structopt(short, long)]
    dir: Option<path::PathBuf>,

    /// Operation to perform on cache
    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt)]
enum Command {
    /// Clean the underlying log file by removing stale entries
    Clean {
        /// Maximum cache entries to retain
        ///
        /// If this is zero, then the file will be deleted
        #[structopt(default_value = "256")]
        count: usize,
    },

    /// Print all newline-separated cache entries
    Get {
        /// Maximum bytes of stale cache entries before compaction
        #[structopt(short, long, default_value = "8192")]
        threshold: u64,

        /// Maximum cache entries to print
        #[structopt(default_value = "256")]
        count: usize,
    },

    /// Update the cache with `entry` as the most recent
    Put {
        /// Validate and transform `entry` as `type`
        #[structopt(short, long)]
        r#type: Option<Type>,

        /// Cache entry to put
        entry: String,
    },
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

    path.push("goldfish");

    match opt.cache {
    | None => path.push("default"),
    | Some(cache) => path.push(cache),
    }

    let log = log::Log::load(path)?;

    match opt.command {
    | Command::Clean { count } => clean(log, count, None)
        .context("Could not clean cache"),
    | Command::Get { count, threshold } => get(log, count, threshold)
        .context("Could not get cache contents"),
    | Command::Put { r#type, entry } => put(log, r#type, entry)
        .context("Could not put cache entry"),
    }
}

fn clean(mut log: log::Log, count: usize, entries: Option<IndexSet<String>>) -> anyhow::Result<()> {
    if count == 0 {
        return log.delete();
    }

    let entries = match entries {
    | None => log.entries(count)?,
    | Some(entries) => entries,
    };

    log.clear()?;
    entries.into_iter()
        .rev()
        .try_for_each(|entry| log.append(entry))?;
    log.sync()
}

fn get(mut log: log::Log, count: usize, threshold: u64) -> anyhow::Result<()> {

    let entries = log.entries(count)?;

    // Write to `stdout`
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    for entry in &entries {
        writeln!(&mut stdout, "{}", entry)?;
    }
    stdout.flush()?;
    drop(stdout);

    // Compact the log by rewriting only the relevant entries
    if log.position()? > threshold {
        clean(log, count, Some(entries))?;
    }

    Ok(())
}

fn put(mut log: log::Log, r#type: Option<Type>, entry: String) -> anyhow::Result<()> {
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
    log.sync()
}
