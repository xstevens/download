#[macro_use]
extern crate clap;
extern crate pbr;
extern crate crypto;
extern crate reqwest;

use clap::{Arg, App};
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


struct CopyResult {
    bytes_written: u64,
    sha1: String,
    sha256: String,
}

fn get_filename(url: &str) -> Option<&str> {
    url.rsplit('/').next()
}

fn write_status(writer: &mut Write, res: &reqwest::Response) {
    let _ = writeln!(writer, "{} {}", res.version(), res.status());
}

fn write_headers(writer: &mut Write, res: &reqwest::Response) {
    let _ = writeln!(writer, "{}", res.headers());
}

fn copy_with_progress<R: ?Sized, W: ?Sized>(reader: &mut R,
                                            writer: &mut W,
                                            progress: &mut ProgressBar<io::Stdout>)
                                            -> io::Result<CopyResult>
    where R: Read,
          W: Write
{
    let mut buf = [0; 8192];
    let mut written = 0;
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    loop {
        let len = match reader.read(&mut buf) {
            Ok(0) => {
                return Ok(CopyResult {
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
    let app = App::new("download")
        .version(crate_version!())
        .about("remote file downloader command-line interface")
        .arg(Arg::with_name("output")
                 .short("o")
                 .long("output")
                 .value_name("OUTPUT")
                 .help("output filename")
                 .takes_value(true))
        .arg(Arg::with_name("remote-name")
                 .short("O")
                 .long("remote-name")
                 .help("output to a file using the same name as the remote"))
        .arg(Arg::with_name("user-agent")
                 .short("A")
                 .long("user-agent")
                 .takes_value(true)
                 .help("use value as user-agent header"))
        .arg(Arg::with_name("max-redirects")
                 .long("max-redirects")
                 .takes_value(true)
                 .default_value("0")
                 .help("maximum number of redirects to follow"))
        .arg(Arg::with_name("verbose")
                 .short("v")
                 .long("verbose")
                 .help("enable verbose logging (useful for debugging)"))
        .arg(Arg::with_name("url").required(true));
    let args = app.get_matches();
    let source_url = args.value_of("url").unwrap();
    let user_agent = args.value_of("user-agent").unwrap_or(DEFAULT_USER_AGENT);
    let max_redirects = args.value_of("max-redirects")
        .unwrap_or_default()
        .parse::<usize>()
        .unwrap();
    let verbose_enabled = args.is_present("verbose");

    let mut client = reqwest::Client::new().unwrap();
    client.gzip(true);
    client.redirect(reqwest::RedirectPolicy::limited(max_redirects));
    let mut res = client
        .get(source_url)
        .header(UserAgent(user_agent.to_owned()))
        .send()
        .unwrap_or_else(|e| {
                            let _ = writeln!(&mut io::stderr(), "{}", e);
                            process::exit(EXIT_URL_FAILURE);
                        });

    // determine an output filename; if none are set then send to stdout
    let output_filename = {
        if args.is_present("remote-name") {
            get_filename(source_url)
        } else {
            args.value_of("output")
        }
    };

    if let Some(fname) = output_filename {
        let _ = File::create(Path::new(fname))
            .and_then(|output_file| {
                let mut writer = BufWriter::new(output_file);

                if verbose_enabled {
                    write_status(&mut io::stdout(), &res);
                    write_headers(&mut io::stdout(), &res);
                }

                // setup progress bar based on content-length
                let mut n_bytes: u64 = 0;
                match res.headers().get::<ContentLength>() {
                    Some(length) => {
                        n_bytes = length.0 as u64;
                    }
                    None => println!("Content-Length header missing"),
                }
                let mut pb = ProgressBar::new(n_bytes);
                pb.set_units(Units::Bytes);

                // copy file with progress updates
                let copy_result = copy_with_progress(&mut res, &mut writer, &mut pb)?;
                writer.flush()?;

                // print hash signatures
                println!("sha1({}) = {}", fname, copy_result.sha1);
                println!("sha256({}) = {}", fname, copy_result.sha256);

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
        
        if verbose_enabled {
            write_status(&mut writer, &res);
            write_headers(&mut writer, &res);
        }

        io::copy(&mut res, &mut writer).unwrap_or_else(|e| {
            let _ = writeln!(&mut io::stderr(), "{}", e);
            process::exit(EXIT_OUTPUT_FAILURE);
        });
    }

    ()
}
