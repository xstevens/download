# download
A remote file downloader.

## Build
```
cargo build --release
```

## Usage
```
$ download -h
download 0.1.0
remote file downloader command-line interface

USAGE:
    download [FLAGS] [OPTIONS] <url>

FLAGS:
    -h, --help           Prints help information
    -O, --remote-name    output to a file using the same name as the remote
    -V, --version        Prints version information

OPTIONS:
    -o, --output <OUTPUT>            output filename
    -U, --user-agent <user-agent>    use value as user-agent header

ARGS:
    <url>
```

## Examples
```
$ download -o rustup.sh https://sh.rustup.rs
```

```
$ download --user-agent "curl/7.51.0" http://wttr.in
```

## License
All aspects of this software are distributed under the MIT License. See LICENSE file for full license text.
