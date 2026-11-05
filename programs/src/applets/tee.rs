//! `tee` — copy standard input to standard output and to files.
//!
//! Usage: `tee [-a] [-i FILE] [FILE...]`. Every named output file is created
//! (truncated) or, with `-a`, appended to; the same bytes always go to stdout.
//! A file that cannot be opened or written is reported on fd 2 and the other
//! destinations continue, with exit status 1.
//!
//! # Reading stdin in this environment
//!
//! The standard form reads fd 0. The pagh console's fd 0 has no EOF: it
//! returns `EAGAIN` until a byte is queued and keeps doing so forever, so a
//! console `tee` runs until the machine is reset. The kernel shell has no
//! pipes or redirection either. For a testable file-to-file copy this port
//! adds `-i FILE` (or `--input=FILE`) as the input source; `-i -` means stdin.
//! This is an extension, not GNU behaviour, and is documented in the README.
//!
//! Exit: 0 success, 1 open/read/write error, 2 usage.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;
use libpagh::fs::File;
use libpagh::sys;

use crate::util;

const USAGE: &str = "usage: tee [-a] [-i FILE] [FILE...]";

struct Sink {
    name: String,
    file: File,
}

pub fn run(args: &[String]) -> i32 {
    let mut append = false;
    let mut input_name: Option<&str> = None;
    let mut outputs: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg == "-a" {
            append = true;
            continue;
        }
        if parse_options && (arg == "-i" || arg == "--input") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            input_name = Some(value);
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("--input=") {
                if value.is_empty() {
                    eprintln!("{USAGE}");
                    return util::EXIT_USAGE;
                }
                input_name = Some(value);
                continue;
            }
            if let Some(value) = arg.strip_prefix("-i") {
                if !value.is_empty() {
                    input_name = Some(value);
                    continue;
                }
            }
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut outputs, arg).is_err() {
            eprintln!("tee: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let source = input_name.unwrap_or("-");
    let input = match util::Input::open(source) {
        Ok(input) => input,
        Err(err) => {
            util::report("tee", util::display_name(source), err);
            return util::EXIT_FAILURE;
        }
    };

    let flags = if append {
        sys::O_WRONLY | sys::O_CREAT | sys::O_APPEND
    } else {
        sys::O_WRONLY | sys::O_CREAT | sys::O_TRUNC
    };

    let mut sinks: Vec<Sink> = Vec::new();
    let mut status = util::EXIT_OK;
    for name in outputs {
        match File::open_with(name, flags, 0o666) {
            Ok(file) => {
                if util::push(
                    &mut sinks,
                    Sink {
                        name: String::from(name),
                        file,
                    },
                )
                .is_err()
                {
                    eprintln!("tee: out of memory");
                    return util::EXIT_FAILURE;
                }
            }
            Err(err) => {
                util::report("tee", name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }

    let mut buffer = [0u8; 8192];
    loop {
        let read = match util::read_retry(input.fd(), &mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) => {
                util::report("tee", util::display_name(source), err);
                return util::EXIT_FAILURE;
            }
        };
        let chunk = &buffer[..read];
        if let Err(err) = util::write_all(1, chunk) {
            eprintln!("tee: write error: {err}");
            return util::EXIT_FAILURE;
        }
        for sink in &mut sinks {
            if let Err(err) = util::write_all(sink.file.as_raw_fd(), chunk) {
                util::report("tee", &sink.name, err);
                status = util::EXIT_FAILURE;
            }
        }
    }
    status
}
