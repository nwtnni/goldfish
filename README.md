# goldfish

This is a basic LRU cache backed by an append-only log file. I use it along
with [`cd`][cd], [`neovim`][nvim], and [`fzf`][fzf], in order to have recently-viewed
directories and recently-edited files available via fuzzy search. It's meant to
be a more transparent alternative to tools like [`z`][z], and therefore doesn't
try to predict or rank anything. It also supports multiple caches via the
`--cache` flag.

`goldfish` implements first-class support for caching directory and file paths, in
the sense that it will a) [canonicalize][cn] paths and b) check if the path
is a file or directory before storing it in the cache. This means the paths
should be valid from any working directory, so you can always `cd` to or edit them.
This behavior can be enabled by passing the `--type` (`-t`) flag to `goldfish put`, e.g.

```bash
> goldfish --cache files put --type file src/foo.rs
> goldfish --cache files put --type file src/
> goldfish --cache files get
/home/nwtnni/projects/example/src/foo.rs
```

## Usage

`goldfish` is fairly low-level and meant to be called from a shell script or alias.
For example, here's the Bash function I use to change directories:

```bash
# Open a directory, list its contents, and cache its path.
#
# If no directory is provided, then use `fzf` to select from the directory cache.
o() {
  if [ -z "$1" ]; then
    dir=$(goldfish --cache directories get | fzf)
    if [ ! -z "$dir" ]; then o "$dir"; fi
  else
    goldfish --cache directories put --type dir "$1" && cd "$1" && ls
  fi
}
```

More detailed options can be viewed with the `--help` flag, i.e. `goldfish --help`.

## Installation

Currently requires a [Rust installation][rust], and is only available from either:

1. [crates.io][crates]

```bash
> cargo install goldfish
```

2. Building from source

```bash
> git clone https://github.com/nwtnni/goldfish.git
> cargo build --release
> ./target/release/goldfish
```

## Implementation

Log entries are just the data followed by a `u16` length footer. This allows
append operations to blindly write to the end of the file. Reading the log is
done backwards, starting at the last two bytes and seeking back to get each
piece of data, which allows us to avoid reading the whole file and efficiently
build the set of most recent entries.

(I'm not entirely sure what performance impact reading a file backwards has on,
say, caching or prefetching. This would require some benchmarking.)

### Disclaimer

There are several articles stating that goldfish actually have [decent][tg] [memory][abc]
[spans][gft], contrary to the popular myth, but I couldn't find many sources. The closest
I found was [this radio transcript][rn] and [this science project][amnh].

[abc]: https://www.abc.net.au/news/2008-02-19/goldfish-three-second-memory-myth-busted/1046710
[amnh]: https://web.archive.org/web/20200312011658/https://www.amnh.org/learn-teach/curriculum-collections/young-naturalist-awards/winning-essays/2015/goldfish-as-a-model-for-understanding-learning-and-memory-more-complex-than-you-think
[cd]: https://en.wikipedia.org/wiki/Cd_(command)
[cn]: https://doc.rust-lang.org/std/path/struct.Path.html#method.canonicalize
[crates]: https://crates.io/
[fzf]: https://github.com/junegunn/fzf
[gft]: https://thegoldfishtank.com/goldfish-info/myth/goldfish-memory-three-second-memory-myth/
[nvim]: https://neovim.io/
[rn]: https://www.abc.net.au/radionational/programs/greatmomentsinscience/goldfish-memory/9949576
[rust]: https://rustup.rs/
[tg]: https://www.telegraph.co.uk/news/science/science-news/4158477/Fishs-memories-last-for-months-say-scientists.html
[z]: https://github.com/rupa/z
