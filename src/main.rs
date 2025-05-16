extern crate clap;
extern crate data_encoding;
extern crate digest;
extern crate hyper;
extern crate pbr;
extern crate reqwest;
extern crate sha1;
extern crate sha2;

use clap::Parser;
use data_encoding::HEXLOWER;
use pbr::{ProgressBar, Units};

use digest::Digest;
use reqwest::header;
use reqwest::tls;
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

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None, arg_required_else_help(true))]
struct Cli {
    /// Output to file path
    #[clap(short, long, value_name = "FILE")]
    output: Option<String>,

    /// Output to a file using the same name as the remote
    #[clap(short('O'), long)]
    remote_name: bool,

    /// Use value as User-Agent header
    #[clap(short('A'), long, default_value = DEFAULT_USER_AGENT)]
    user_agent: String,

    /// Maximum number of redirects to follow
    #[clap(long, default_value = "0")]
    max_redirects: usize,

    /// Enable verbose logging
    #[clap(short, long)]
    verbose: bool,

    #[arg(required(true))]
    url: reqwest::Url,
}

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
    url: reqwest::Url,
    user_agent: &str,
    max_redirects: usize,
) -> reqwest::Result<reqwest::blocking::Response> {
    let client = reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(max_redirects))
        .min_tls_version(tls::Version::TLS_1_2)
        .connect_timeout(std::time::Duration::from_secs(5))
        .use_rustls_tls()
        .https_only(true)
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
                });
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
    // parse CLI args
    let cli = Cli::parse();

    // determine an output filename; if none are set then send to stdout
    // TODO: this feels janky to have to clone and long-live store this to avoid borrow-checker annoyances below
    let llpath = cli.output.clone().unwrap_or_default();
    let output_path = {
        if cli.remote_name {
            get_filename(cli.url.path()).map(|filename| Path::new(filename))
        } else {
            cli.output.map(|_| Path::new(llpath.as_str()))
        }
    };

    // setup client for downloading and send request
    let mut resp = http_download(cli.url.clone(), cli.user_agent.as_str(), cli.max_redirects)
        .unwrap_or_else(|e| {
            let _ = writeln!(&mut io::stderr(), "{}", e);
            process::exit(EXIT_URL_FAILURE);
        });

    // process response
    if let Some(file_path) = output_path {
        let _ = File::create(file_path)
            .and_then(|output_file| {
                let mut writer = BufWriter::new(output_file);

                if cli.verbose {
                    write_status(&mut io::stdout(), &resp);
                    write_headers(&mut io::stdout(), &resp);
                }

                // setup progress bar based on content-length
                let n_bytes: u64 = resp
                    .headers()
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
                println!("sha1({}) = {}", file_path.display(), result.sha1);
                println!("sha256({}) = {}", file_path.display(), result.sha256,);

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

        if cli.verbose {
            write_status(&mut writer, &resp);
            write_headers(&mut writer, &resp);
        }

        io::copy(&mut resp, &mut writer).unwrap_or_else(|e| {
            let _ = writeln!(&mut io::stderr(), "{}", e);
            process::exit(EXIT_OUTPUT_FAILURE);
        });
    }
}
