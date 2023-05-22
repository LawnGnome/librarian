# Librarian

This tool (by default) will mirror the entirety of crates.io for you.

Unsurprisingly, this requires quite a lot of space. Assume you need a terabyte,
as of my writing this in December 2023. (Strongly consider using a compressed
btrfs or other filesystem to host this, in which case you can probably cut it by
half.)

## Usage

First we need to build this (no shocks here):

```sh
cargo build --release
```

Then we need to check out the Git crate index (in this example, into the `./index`
directory, which will be created if it doesn't exist):

```sh
./target/release/librarian -i ./index index-update
```

Finally, we need to download the crates (in this example, into `./corpus`, which
will again be created if it doesn't exist):

```sh
./target/release/librarian -i ./index populate -c ./corpus
```

Note that the corpus will have some extra levels based on the first 1-2
characters of the crate name, just to not stress your filesystem _too_ much.

The `index-update` and `populate` commands can be run again to update existing
indices and corpora: you don't have to do a full redownload each time.
