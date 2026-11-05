//! `install` — copy files and give them an explicit mode.
//!
//! Usage: `install [-m MODE] [-D] [-f] SOURCE... DEST`. With one source,
//! `DEST` may be a new file or an existing directory; with several sources it
//! must be a directory. `-D` creates missing destination directories (and the
//! parent of a file destination); `-f` permits replacing an existing file.
//! Without `-f` an existing destination is an error, so an accidental
//! overwrite is visible in the exit status.
//!
//! `-m` takes an octal mode (default `0755`) and applies it to the copy.
//! **The mode is a per-run override**: `fchmodat` in kernel 0.16.1 records the
//! bits only for the calling process, so a later `stat` sees the on-disk mode
//! again. This is documented in `fs::set_permissions` and the README, not
//! hidden here. There is no `-o`/`-g` ownership option: the ABI has no
//! uid/gid syscall.
//!
//! Exit: 0 success, 1 copy/stat/mode error, 2 usage. Every source is attempted
//! even after a failure; the status is 1 if any failed.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::path;
use libpagh::{eprintln, fs, Errno};

use crate::util;

const USAGE: &str = "usage: install [-m MODE] [-D] [-f] SOURCE... DEST";

pub fn run(args: &[String]) -> i32 {
    let mut mode: Option<u32> = None;
    let mut create_parents = false;
    let mut force = false;
    let mut operands: Vec<&str> = Vec::new();
    let mut index = 0usize;
    let mut parse_options = true;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && (arg == "-m" || arg == "--mode") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match parse_mode(value) {
                Some(parsed) => mode = Some(parsed),
                None => {
                    eprintln!("install: invalid mode: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options {
            if let Some(value) = arg.strip_prefix("--mode=") {
                match parse_mode(value) {
                    Some(parsed) => mode = Some(parsed),
                    None => {
                        eprintln!("install: invalid mode: '{value}'");
                        return util::EXIT_USAGE;
                    }
                }
                continue;
            }
            if let Some(value) = arg.strip_prefix("-m") {
                if !value.is_empty() {
                    match parse_mode(value) {
                        Some(parsed) => mode = Some(parsed),
                        None => {
                            eprintln!("install: invalid mode: '{value}'");
                            return util::EXIT_USAGE;
                        }
                    }
                    continue;
                }
            }
        }
        if parse_options && (arg == "-D" || arg == "--create-leading") {
            create_parents = true;
            continue;
        }
        if parse_options && (arg == "-f" || arg == "--force") {
            force = true;
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }
        parse_options = false;
        if util::push(&mut operands, arg).is_err() {
            eprintln!("install: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if operands.len() < 2 {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    let dest = operands[operands.len() - 1];
    let sources = &operands[..operands.len() - 1];
    let dest_stat = fs::stat(dest).ok();
    let mut dest_is_dir = dest_stat.as_ref().map(|stat| stat.is_dir()).unwrap_or(false);
    let wants_dir = sources.len() > 1 || dest.ends_with('/');

    if !dest_is_dir && wants_dir {
        if dest_stat.is_some() {
            eprintln!("install: {dest}: Not a directory");
            return util::EXIT_FAILURE;
        }
        if !create_parents {
            util::report("install", dest, Errno::ENOENT);
            return util::EXIT_FAILURE;
        }
        if let Err(err) = util::create_dir_all(dest) {
            util::report("install", dest, err);
            return util::EXIT_FAILURE;
        }
        dest_is_dir = true;
    }

    let install_mode = mode.unwrap_or(0o755);
    let mut status = util::EXIT_OK;
    for source in sources {
        let target = if dest_is_dir {
            util::join_path(dest, path::basename(source))
        } else {
            String::from(dest)
        };

        if create_parents {
            let parent = path::dirname(&target);
            if parent != "/" {
                if let Err(err) = util::create_dir_all(parent) {
                    util::report("install", target.as_str(), err);
                    status = util::EXIT_FAILURE;
                    continue;
                }
            }
        }

        let source_stat = match fs::stat(source) {
            Ok(stat) => stat,
            Err(err) => {
                util::report("install", source, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        if !source_stat.is_file() {
            util::report("install", source, Errno::EISDIR);
            status = util::EXIT_FAILURE;
            continue;
        }

        match fs::stat(target.as_str()) {
            Ok(stat) => {
                if stat.is_dir() {
                    util::report("install", target.as_str(), Errno::EISDIR);
                    status = util::EXIT_FAILURE;
                    continue;
                }
                if !force {
                    util::report("install", target.as_str(), Errno::EEXIST);
                    status = util::EXIT_FAILURE;
                    continue;
                }
            }
            Err(Errno::ENOENT) => {}
            Err(err) => {
                util::report("install", target.as_str(), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        }

        let data = match fs::read_to_end(source) {
            Ok(data) => data,
            Err(err) => {
                util::report("install", source, err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        if let Err(err) = fs::write(target.as_str(), &data) {
            util::report("install", target.as_str(), err);
            status = util::EXIT_FAILURE;
            continue;
        }
        if let Err(err) = fs::set_permissions(target.as_str(), install_mode) {
            // The copy exists; report the mode failure but do not delete it.
            util::report("install", target.as_str(), err);
            status = util::EXIT_FAILURE;
        }
    }
    status
}

fn parse_mode(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
        return None;
    }
    let mode = u32::from_str_radix(text, 8).ok()?;
    if mode > 0o7777 {
        return None;
    }
    Some(mode)
}