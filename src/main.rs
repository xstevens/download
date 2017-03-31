extern crate clap;
extern crate reqwest;

use clap::{Arg, App};
use std::io;
use std::io::BufWriter;
use std::io::prelude::*;
use std::fs::File;
use std::path::Path;
use std::process;

enum ExitStatus {
    UrlFailure = 1,
    OutputFailure = 2,
}

fn exit(exit_status: ExitStatus) -> ! {
    process::exit(exit_status as i32);
}

fn main() {
    let args = App::new("download")
        .version("0.1.0")
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
        .arg(Arg::with_name("url").required(true))
        .get_matches();

    
    let source_url = args.value_of("url").unwrap();
    let mut res = reqwest::get(source_url)
        .unwrap_or_else(|_| process::exit(ExitStatus::UrlFailure as i32));
    if let Some(output_filename) = args.value_of("output") {
        let _ = File::create(Path::new(output_filename))
            .and_then(|output_file| {
                let mut writer = BufWriter::new(output_file);
                io::copy(&mut res, &mut writer)?;
                writer.flush()?;
                Ok(())
            })
            .map_err(|_| exit(ExitStatus::OutputFailure));
    } else {
        io::copy(&mut res, &mut io::stdout())
            .unwrap_or_else(|_| exit(ExitStatus::OutputFailure));
    }
    
    ()
}
