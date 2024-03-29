#[macro_use]
extern crate clap;
extern crate data_encoding;
extern crate hyper;
extern crate pbr;
extern crate reqwest;
extern crate digest;
extern crate sha2;
extern crate sha1;

use clap::{App, Arg};
use data_encoding::HEXLOWER;
use pbr::{ProgressBar, Units};

use reqwest::header;
use reqwest::tls;
use hyper::Uri;
use digest::Digest;
use sha1::Sha1;
use sha2::Sha256;
use std::fs::File;
use std::io;
use std::io::BufWriter;
use std::io::prelude::*;
use std::path::Path;
use std::process;

static DEFAULT_USER_AGENT: &str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
const EXIT_URL_FAILURE: i32 = 1;
const EXIT_OUTPUT_FAILURE: i32 = 2;

struct DownloadResult {
    bytes_written: u64,
    sha1: String,
    sha256: String,
}

fn get_filename(url: &str) -> Option<&str> {
    url.rsplit('/').next()
}

fn write_status(writer: &mut dyn Write, resp: &reqwest::blocking::Response) {
    let _ = writeln!(writer, "{:?} {}", resp.version(), resp.status());
}

fn write_headers(writer: &mut dyn Write, resp: &reqwest::blocking::Response) {
    for (key, value) in resp.headers().iter() {
        let _ = writeln!(writer, "{}: {}", key, value.to_str().unwrap_or(""));
    }
}

fn http_download(
    url: &str,
    user_agent: &str,
    max_redirects: usize,
) -> reqwest::Result<reqwest::blocking::Response> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(max_redirects))
        .min_tls_version(tls::Version::TLS_1_2)
        .build()?;

    let ua_header = header::HeaderValue::from_str(user_agent).unwrap();
    let resp = client
        .get(url)
        .header(header::USER_AGENT, ua_header)
        .send()?;

    Ok(resp)
}

fn download_with_progress<R: ?Sized, W: ?Sized>(
    reader: &mut R,
    writer: &mut W,
    progress: &mut ProgressBar<io::Stdout>,
) -> io::Result<DownloadResult>
where
    R: Read,
    W: Write,
{
    let mut buf = [0; 8192];
    let mut written = 0;
    let mut sha1_hasher = Sha1::new();
    let mut sha256_hasher = Sha256::new();
    loop {
        let len = match reader.read(&mut buf) {
            Ok(0) => {
                return Ok(DownloadResult {
                    bytes_written: written,
                    sha1: HEXLOWER.encode(sha1_hasher.finalize().as_slice()),
                    sha256: HEXLOWER.encode(sha256_hasher.finalize().as_slice()),
                })
            }
            Ok(len) => len,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        // write buf to writer
        writer.write_all(&buf[..len])?;

        // add buf to hash digests
        sha1_hasher.update(&buf[..len]);
        sha256_hasher.update(&buf[..len]);

        // increment progress and bytes written
        progress.add(len as u64);
        written += len as u64;
    }
}

fn main() {
    let args = App::new("download")
        .version(crate_version!())
        .about("remote file downloader command-line interface")
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("OUTPUT")
                .help("output filename")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("remote-name")
                .short("O")
                .long("remote-name")
                .help("output to a file using the same name as the remote"),
        )
        .arg(
            Arg::with_name("user-agent")
                .short("A")
                .long("user-agent")
                .takes_value(true)
                .help("use value as user-agent header"),
        )
        .arg(
            Arg::with_name("max-redirects")
                .long("max-redirects")
                .takes_value(true)
                .default_value("0")
                .help("maximum number of redirects to follow"),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("enable verbose logging (useful for debugging)"),
        )
        .arg(Arg::with_name("url").required(true))
        .get_matches();

    let url = args.value_of("url").unwrap();
    let user_agent = args.value_of("user-agent").unwrap_or(DEFAULT_USER_AGENT);
    let max_redirects = args.value_of("max-redirects")
        .unwrap_or_default()
        .parse::<usize>()
        .unwrap();
    let verbose = args.is_present("verbose");

    // determine an output filename; if none are set then send to stdout
    let uri = url.parse::<Uri>().unwrap();
    let output_path = {
        if args.is_present("remote-name") {
            get_filename(uri.path())
                .map(|filename| Path::new(filename))
        } else {
            args.value_of("output")
                .map(|path| Path::new(path))
        }
    };

    // setup client for downloading and send request
    let mut resp = http_download(url, user_agent, max_redirects).unwrap_or_else(|e| {
        let _ = writeln!(&mut io::stderr(), "{}", e);
        process::exit(EXIT_URL_FAILURE);
    });

    // process response
    if let Some(file_path) = output_path {
        let _ = File::create(file_path)
            .and_then(|output_file| {
                let mut writer = BufWriter::new(output_file);

                if verbose {
                    write_status(&mut io::stdout(), &resp);
                    write_headers(&mut io::stdout(), &resp);
                }

                // setup progress bar based on content-length
                let n_bytes: u64 = resp.headers()
                    .get(header::CONTENT_LENGTH)
                    .and_then(|content_len| content_len.to_str().ok())
                    .and_then(|content_len| content_len.parse().ok())
                    .unwrap_or(0);
                let mut pb = ProgressBar::new(n_bytes);
                pb.set_units(Units::Bytes);

                // copy file with progress updates
                let result = download_with_progress(&mut resp, &mut writer, &mut pb)?;
                writer.flush()?;

                // print hash digests
                println!(
                    "sha1({}) = {}",
                    file_path.display(),
                    result.sha1
                );
                println!(
                    "sha256({}) = {}",
                    file_path.display(),
                    result.sha256,
                );

                pb.finish_print("Done.");

                Ok(())
            })
            .map_err(|e| {
                let _ = writeln!(&mut io::stderr(), "{}", e);
                process::exit(EXIT_OUTPUT_FAILURE);
            });
    } else {
        let stdout = io::stdout();
        let lock = stdout.lock();
        let mut writer = BufWriter::new(lock);

        if verbose {
            write_status(&mut writer, &resp);
            write_headers(&mut writer, &resp);
        }

        io::copy(&mut resp, &mut writer).unwrap_or_else(|e| {
            let _ = writeln!(&mut io::stderr(), "{}", e);
            process::exit(EXIT_OUTPUT_FAILURE);
        });
    }
}
