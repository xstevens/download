# download
A remote file downloader.

## Build
```
cargo build --release
```

## Usage
```
$ target/release/download -h
download 0.1.0
remote file downloader command-line interface

USAGE:
    download [FLAGS] [OPTIONS] <url>

FLAGS:
    -h, --help           Prints help information
    -O, --remote-name    output to a file using the same name as the remote
    -V, --version        Prints version information

OPTIONS:
    -o, --output <OUTPUT>    output filename

ARGS:
    <url>
```

## Example
```
download -o rustup.sh https://sh.rustup.rs
```

## License
All aspects of this software are distributed under the MIT License. See LICENSE file for full license text.
