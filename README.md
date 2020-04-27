# lru

This is a basic LRU cache backed by an append-only log file. I use it along
with [`cd`][cd], [`neovim`][nvim], and [`fzf`][fzf], in order to have recently-viewed
directories and recently-edited files available via fuzzy search. It's meant to
be a more transparent alternative to tools like [`z`][z], and doesn't try to
predict or rank anything.

`lru` implements first-class support for caching directory and file paths, in
the sense that it will a) [canonicalize][cn] paths and b) check if the path
is a file or directory before storing it in the cache. This means the paths
should be valid from any working directory, so you can always `cd` to or edit them.
This behavior can be enabled by passing the `--type` (`-t`) flag to `lru put`, e.g.

```bash
> lru --cache files put --type file src/foo.rs
> lru --cache files put --type file src/
> lru --cache files get
/home/nwtnni/projects/example/src/foo.rs
```

## Usage

`lru` is fairly low-level and meant to be called from a shell script or alias.
For example, here's the Bash function I use to change directories:

```bash
# Open a directory, list its contents, and cache its path.
#
# If no directory is provided, then use `fzf` to select from the directory cache.
o() {
  if [ -z "$1" ]; then
    dir=$(lru --cache directories get | fzf)
    if [ ! -z "$dir" ]; then o "$dir"; fi
  else
    cd "$1" && ls && lru --cache directories put --type dir "$1"
  fi
}
```

More detailed options can be viewed with the `--help` flag, i.e. `lru --help`.

## Implementation

Log entries are just the data followed by a `u16` length footer. This allows
append operations to blindly write to the end of the file. Reading the log is
done backwards, starting at the last two bytes and seeking back to get each
piece of data, which allows us to avoid reading the whole file and efficiently
build the set of most recent entries.

(I'm not entirely sure what performance impact reading a file backwards has on,
say, caching or prefetching. This would require some benchmarking.)

[cd]: https://en.wikipedia.org/wiki/Cd_(command)
[cn]: https://doc.rust-lang.org/std/path/struct.Path.html#method.canonicalize
[fzf]: https://github.com/junegunn/fzf
[nvim]: https://neovim.io/
[z]: https://github.com/rupa/z
