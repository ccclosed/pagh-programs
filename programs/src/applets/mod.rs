//! The applet modules.
//!
//! Each module is self-contained: it parses its own arguments, prints its own
//! diagnostics and returns an exit code (`0` success, `1` data error, `2` usage
//! error). There is no global option parser and no panic on bad input.

use alloc::string::String;

pub mod arch;
pub mod awk;
pub mod base32;
pub mod base64;
pub mod basename;
mod bre;
pub mod cat;
pub mod chmod;
pub mod cksum;
pub mod cmp;
mod codec;
pub mod comm;
pub mod cp;
pub mod cut;
pub mod date;
pub mod diff;
pub mod dirname;
pub mod du;
pub mod echo;
pub mod expand;
pub mod expr;
pub mod factor;
pub mod false_applet;
pub mod file;
pub mod find;
pub mod fold;
pub mod grep;
pub mod head;
pub mod hexdump;
pub mod hostname;
pub mod ln;
pub mod ls;
pub mod mkdir;
pub mod mktemp;
pub mod mv;
pub mod nl;
pub mod nproc;
pub mod numfmt;
pub mod paste;
pub mod printf;
pub mod pwd;
pub mod readlink;
pub mod realpath;
pub mod rev;
pub mod rmdir;
pub mod rm;
pub mod sed;
pub mod seq;
pub mod shuf;
pub mod sleep;
pub mod sort;
pub mod stat;
pub mod strings;
pub mod sum;
pub mod tac;
pub mod tail;
pub mod test;
pub mod touch;
pub mod tr;
pub mod tree;
pub mod true_applet;
pub mod truncate;
pub mod uname;
pub mod unexpand;
pub mod uniq;
pub mod uptime;
pub mod wc;
pub mod which;
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
    "rm",
    "mkdir",
    "touch",
    "cp",
    "mv",
    "ln",
    "truncate",
    "chmod",
    "find",
    "du",
    "tree",
    "file",
    "printf",
    "expr",
    "test",
    "factor",
    "which",
    "mktemp",
    "rmdir",
    "tac",
    "sum",
    "cksum",
    "base64",
    "base32",
    "diff",
    "cmp",
    "sed",
    "awk",
    "shuf",
    "numfmt",
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
        "rm" => rm::run(args),
        "mkdir" => mkdir::run(args),
        "touch" => touch::run(args),
        "cp" => cp::run(args),
        "mv" => mv::run(args),
        "ln" => ln::run(args),
        "truncate" => truncate::run(args),
        "chmod" => chmod::run(args),
        "find" => find::run(args),
        "du" => du::run(args),
        "tree" => tree::run(args),
        "file" => file::run(args),
        "printf" => printf::run(args),
        "expr" => expr::run(args),
        "test" => test::run(args, false),
        "[" => test::run(args, true),
        "factor" => factor::run(args),
        "which" => which::run(args),
        "mktemp" => mktemp::run(args),
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
        "rmdir" => rmdir::run(args),
        "tac" => tac::run(args),
        "sum" => sum::run(args),
        "cksum" => cksum::run(args),
        "base64" => base64::run(args),
        "base32" => base32::run(args),
        "diff" => diff::run(args),
        "cmp" => cmp::run(args),
        "sed" => sed::run(args),
        "awk" => awk::run(args),
        "shuf" => shuf::run(args),
        "numfmt" => numfmt::run(args),
        _ => return None,
    })
}