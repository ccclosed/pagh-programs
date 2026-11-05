//! The applet modules.
//!
//! Each module is self-contained: it parses its own arguments, prints its own
//! diagnostics and returns an exit code (`0` success, `1` data error, `2` usage
//! error). There is no global option parser and no panic on bad input.

use alloc::string::String;

pub mod arch;
pub mod basename;
pub mod cat;
pub mod comm;
pub mod cut;
pub mod date;
pub mod dirname;
pub mod echo;
pub mod expand;
pub mod false_applet;
pub mod fold;
pub mod grep;
pub mod head;
pub mod hexdump;
pub mod hostname;
pub mod ls;
pub mod nl;
pub mod nproc;
pub mod paste;
pub mod pwd;
pub mod readlink;
pub mod realpath;
pub mod rev;
pub mod seq;
pub mod sleep;
pub mod sort;
pub mod stat;
pub mod strings;
pub mod tail;
pub mod tr;
pub mod true_applet;
pub mod uname;
pub mod unexpand;
pub mod uniq;
pub mod uptime;
pub mod wc;
pub mod yes;

/// Every applet in the bundle. `tools/build_repo.py` parses this list to build
/// the package manifest, so it is the single source of truth for `commands=`.
pub const NAMES: &[&str] = &[
    "echo",
    "true",
    "false",
    "yes",
    "pwd",
    "basename",
    "dirname",
    "seq",
    "cat",
    "wc",
    "head",
    "tail",
    "ls",
    "stat",
    "uname",
    "hostname",
    "arch",
    "nproc",
    "sleep",
    "uptime",
    "date",
    "readlink",
    "realpath",
    "uniq",
    "sort",
    "nl",
    "fold",
    "expand",
    "unexpand",
    "rev",
    "cut",
    "tr",
    "comm",
    "paste",
    "strings",
    "hexdump",
    "grep",
];

/// Run an applet by name; `None` means "no such applet".
pub fn dispatch(name: &str, args: &[String]) -> Option<i32> {
    Some(match name {
        "echo" => echo::run(args),
        "true" => true_applet::run(args),
        "false" => false_applet::run(args),
        "yes" => yes::run(args),
        "pwd" => pwd::run(args),
        "basename" => basename::run(args),
        "dirname" => dirname::run(args),
        "seq" => seq::run(args),
        "cat" => cat::run(args),
        "wc" => wc::run(args),
        "head" => head::run(args),
        "tail" => tail::run(args),
        "ls" => ls::run(args),
        "stat" => stat::run(args),
        "uname" => uname::run(args),
        "hostname" => hostname::run(args),
        "arch" => arch::run(args),
        "nproc" => nproc::run(args),
        "sleep" => sleep::run(args),
        "uptime" => uptime::run(args),
        "date" => date::run(args),
        "readlink" => readlink::run(args),
        "realpath" => realpath::run(args),
        "uniq" => uniq::run(args),
        "sort" => sort::run(args),
        "nl" => nl::run(args),
        "fold" => fold::run(args),
        "expand" => expand::run(args),
        "unexpand" => unexpand::run(args),
        "rev" => rev::run(args),
        "cut" => cut::run(args),
        "tr" => tr::run(args),
        "comm" => comm::run(args),
        "paste" => paste::run(args),
        "strings" => strings::run(args),
        "hexdump" => hexdump::run(args),
        "grep" => grep::run(args),
        _ => return None,
    })
}