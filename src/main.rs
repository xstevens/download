#[macro_use]
extern crate clap;
extern crate pbr;
extern crate crypto;
extern crate reqwest;

use clap::{App, Arg};
use crypto::digest::Digest;
use crypto::sha1::Sha1;
use crypto::sha2::Sha256;
use pbr::{ProgressBar, Units};
use reqwest::header::UserAgent;
use reqwest::header::ContentLength;
use std::fs::File;
use std::io;
use std::io::BufWriter;
use std::io::prelude::*;
use std::path::Path;
use std::process;

static DEFAULT_USER_AGENT: &'static str =
    concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));
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

fn write_status(writer: &mut Write, resp: &reqwest::Response) {
    // TODO: get version back in reqwest 0.7... maybe can be cast to hyper type
    let _ = writeln!(writer, "{}", resp.status());
}

fn write_headers(writer: &mut Write, resp: &reqwest::Response) {
    let _ = writeln!(writer, "{}", resp.headers());
}

fn http_download(url: &str, user_agent: &str, max_redirects: usize) -> reqwest::Result<reqwest::Response> {
    let client = reqwest::Client::builder()
        .gzip(true)
        .redirect(reqwest::RedirectPolicy::limited(max_redirects))
        .build()?;

    let resp = client
        .get(url)
        .header(UserAgent::new(user_agent.to_owned()))
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
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    loop {
        let len = match reader.read(&mut buf) {
            Ok(0) => {
                return Ok(DownloadResult {
                    bytes_written: written,
                    sha1: sha1.result_str(),
                    sha256: sha256.result_str(),
                })
            }
            Ok(len) => len,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        // write buf to writer
        writer.write_all(&buf[..len])?;

        // add buf to hash digests
        sha1.input(&buf[..len]);
        sha256.input(&buf[..len]);

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
    let user_agent = args.value_of("user-agent")
        .unwrap_or(DEFAULT_USER_AGENT);
    let max_redirects = args.value_of("max-redirects")
        .unwrap_or_default()
        .parse::<usize>()
        .unwrap();
    let verbose = args.is_present("verbose");

    // determine an output filename; if none are set then send to stdout
    let output_path = {
        if args.is_present("remote-name") {
            get_filename(url).and_then(|filename| Some(Path::new(filename)))
        } else {
            args.value_of("output")
                .and_then(|path| Some(Path::new(path)))
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
                let mut n_bytes: u64 = 0;
                match resp.headers().get::<ContentLength>() {
                    Some(length) => {
                        n_bytes = length.0 as u64;
                    }
                    None => println!("Content-Length header missing"),
                }
                let mut pb = ProgressBar::new(n_bytes);
                pb.set_units(Units::Bytes);

                // copy file with progress updates
                let result = download_with_progress(&mut resp, &mut writer, &mut pb)?;
                writer.flush()?;

                // print hash signatures
                println!("sha1({}) = {}", file_path.display(), result.sha1);
                println!("sha256({}) = {}", file_path.display(), result.sha256);

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

    ()
}
