#[macro_use]
extern crate clap;
extern crate reqwest;

use clap::{Arg, App};
use reqwest::header::UserAgent;
use std::fs::File;
use std::io;
use std::io::BufWriter;
use std::io::prelude::*;
use std::path::Path;
use std::process;

static DEFAULT_USER_AGENT: &'static str = concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"));

enum ExitStatus {
    UrlFailure = 1,
    OutputFailure = 2,
}

fn exit(exit_status: ExitStatus) -> ! {
    process::exit(exit_status as i32);
}

fn get_filename(url: &str) -> Option<&str> {
    url.rsplit('/').next()
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
                           .short("U")
                           .long("user-agent")
                           .takes_value(true)
                           .help("use value as user-agent header"))
                  .arg(Arg::with_name("url").required(true));
    let args = app.get_matches();
    let source_url = args.value_of("url").unwrap();
    let user_agent = args.value_of("user-agent")
                         .unwrap_or(DEFAULT_USER_AGENT); 

    let mut client = reqwest::Client::new().unwrap();
    client.gzip(true);
    // TODO: add redirect following options
    client.redirect(reqwest::RedirectPolicy::limited(1));
    let mut res = client.get(source_url)
                        .header(UserAgent(user_agent.to_owned()))
                        .send()
                        .unwrap_or_else(|e| {
                          let _ = writeln!(&mut std::io::stderr(), "{}", e);
                          process::exit(ExitStatus::UrlFailure as i32)
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
                io::copy(&mut res, &mut writer)?;
                writer.flush()?;
                Ok(())
            })
            .map_err(|e| {
                let _ = writeln!(&mut std::io::stderr(), "{}", e);
                exit(ExitStatus::OutputFailure);
            });
    } else {
        io::copy(&mut res, &mut io::stdout())
            .unwrap_or_else(|e| {
                let _ = writeln!(&mut std::io::stderr(), "{}", e);
                exit(ExitStatus::OutputFailure)
            });
    }

    ()
}
