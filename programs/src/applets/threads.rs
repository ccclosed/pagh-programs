//! `threads` — four native threads incrementing one counter under a futex Mutex.
//!
//! A small, honest smoke test for the 0.13+ thread/futex ABI: if the kernel or
//! libpagh drops a wakeup, the printed counter is not 1000 and the applet exits
//! non-zero.

use alloc::sync::Arc;
use alloc::vec::Vec;

use libpagh::sync::Mutex;
use libpagh::{eprintln, print, println, thread};

use crate::util;

const USAGE: &str = "usage: threads";

pub fn run(args: &[alloc::string::String]) -> i32 {
    if let Some(arg) = args.first() {
        if arg == "-h" || arg == "--help" {
            println!("{USAGE}");
            return util::EXIT_OK;
        }
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }
    let counter = Arc::new(Mutex::new(0u32));
    let mut handles: Vec<thread::JoinHandle> = Vec::new();
    for _ in 0..4 {
        let counter = Arc::clone(&counter);
        match thread::Builder::new().stack_size(32 * 1024).spawn(move || {
            for _ in 0..250 {
                *counter.lock() += 1;
            }
            0
        }) {
            Ok(handle) => handles.push(handle),
            Err(error) => {
                libpagh::eprintln!("threads: spawn failed: {error}");
                return 1;
            }
        }
    }
    for handle in handles {
        if let Err(error) = handle.join() {
            libpagh::eprintln!("threads: join failed: {error}");
            return 1;
        }
    }
    let value = *counter.lock();
    print!("counter={}\n", value);
    if value == 1000 { 0 } else { 1 }
}
