//! `untar` — the extraction name for [`super::tar`].
//!
//! `untar` is exactly `tar -x`: it parses the same `-f`, `-C`, `-v` and
//! `MEMBER...` arguments and rejects a conflicting `-c`/`-t`. The separate
//! name exists because users and scripts invoke it directly; the archive code
//! is shared, not duplicated.

use alloc::string::String;

use super::tar;

pub fn run(args: &[String]) -> i32 {
    tar::run_extract(args)
}
