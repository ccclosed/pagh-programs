//! The applet modules.
//!
//! Each module is self-contained: it parses its own arguments, prints its own
//! diagnostics and returns an exit code (`0` success, `1` data error, `2` usage
//! error). There is no global option parser and no panic on bad input.

use alloc::string::String;

pub mod arch;
pub mod awk;
pub mod b2sum;
pub mod base32;
pub mod base58;
pub mod base64;
pub mod basenc;
pub mod basename;
pub mod bin2hex;
mod bre;
pub mod cal;
pub mod cat;
pub mod chmod;
pub mod csplit;
pub mod curl;
pub mod cksum;
pub mod cmp;
mod codec;
pub mod comm;
pub mod cp;
pub mod crc32;
pub mod cut;
pub mod date;
pub mod diff;
mod digest;
pub mod dirname;
pub mod du;
pub mod echo;
pub mod env;
pub mod expand;
pub mod expr;
pub mod factor;
pub mod false_applet;
pub mod file;
pub mod find;
pub mod fmt;
pub mod fold;
pub mod gzip;
pub mod grep;
pub mod gunzip;
pub mod head;
pub mod hex2bin;
pub mod hexdump;
mod hashsum;
pub mod hostname;
mod httpc;
pub mod id;
pub mod ln;
pub mod ls;
pub mod md5sum;
pub mod mkdir;
pub mod join;
pub mod kill;
pub mod mktemp;
pub mod mv;
pub mod nl;
pub mod nproc;
pub mod numfmt;
pub mod od;
pub mod paste;
pub mod patch;
pub mod printf;
pub mod pwd;
pub mod readlink;
pub mod realpath;
pub mod rev;
pub mod rmdir;
pub mod rm;
pub mod sed;
pub mod seq;
pub mod sha1sum;
pub mod sha224sum;
pub mod sha256sum;
pub mod sha384sum;
pub mod sha512sum;
pub mod shuf;
pub mod sleep;
pub mod split;
pub mod sort;
pub mod stat;
pub mod strings;
pub mod sum;
pub mod tac;
pub mod tail;
pub mod tar;
pub mod tee;
pub mod test;
pub mod touch;
pub mod tr;
pub mod tree;
pub mod true_applet;
pub mod truncate;
pub mod uname;
pub mod unexpand;
pub mod untar;
pub mod uniq;
pub mod uptime;
mod uu;
pub mod uudecode;
pub mod uuencode;
pub mod wc;
pub mod wget;
pub mod whoami;
pub mod which;
pub mod xxd;
pub mod yes;
pub mod command;
pub mod exit;
pub mod install;
pub mod more;
pub mod nice;
pub mod nm;
pub mod parallel;
pub mod shred;
pub mod time;
pub mod timeout;
pub mod type_applet;
pub mod watch;
pub mod xargs;
pub mod zcat;

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
    "cal",
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
    "md5sum",
    "sha1sum",
    "sha256sum",
    "sha512sum",
    "b2sum",
    "base64",
    "base32",
    "basenc",
    "diff",
    "cmp",
    "patch",
    "sed",
    "awk",
    "shuf",
    "numfmt",
    "whoami",
    "id",
    "env",
    "tee",
    "split",
    "csplit",
    "join",
    "fmt",
    "od",
    "xxd",
    "uuencode",
    "uudecode",
    "curl",
    "wget",
    "kill",
    "gzip",
    "gunzip",
    "tar",
    "untar",
    "timeout",
    "watch",
    "xargs",
    "parallel",
    "install",
    "shred",
    "command",
    "type",
    "time",
    "exit",
    "nice",
    "more",
    "nm",
    "sha224sum",
    "sha384sum",
    "crc32",
    "base58",
    "hex2bin",
    "bin2hex",
    "zcat",
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
        "cal" => cal::run(args),
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
        "hex2bin" => hex2bin::run(args),
        "bin2hex" => bin2hex::run(args),
        "grep" => grep::run(args),
        "rmdir" => rmdir::run(args),
        "tac" => tac::run(args),
        "sum" => sum::run(args),
        "cksum" => cksum::run(args),
        "crc32" => crc32::run(args),
        "md5sum" => md5sum::run(args),
        "sha1sum" => sha1sum::run(args),
        "sha224sum" => sha224sum::run(args),
        "sha256sum" => sha256sum::run(args),
        "sha384sum" => sha384sum::run(args),
        "sha512sum" => sha512sum::run(args),
        "b2sum" => b2sum::run(args),
        "base64" => base64::run(args),
        "base58" => base58::run(args),
        "base32" => base32::run(args),
        "basenc" => basenc::run(args),
        "diff" => diff::run(args),
        "cmp" => cmp::run(args),
        "patch" => patch::run(args),
        "sed" => sed::run(args),
        "awk" => awk::run(args),
        "shuf" => shuf::run(args),
        "numfmt" => numfmt::run(args),
        "whoami" => whoami::run(args),
        "id" => id::run(args),
        "env" => env::run(args),
        "tee" => tee::run(args),
        "split" => split::run(args),
        "csplit" => csplit::run(args),
        "join" => join::run(args),
        "fmt" => fmt::run(args),
        "od" => od::run(args),
        "xxd" => xxd::run(args),
        "uuencode" => uuencode::run(args),
        "uudecode" => uudecode::run(args),
        "curl" => curl::run(args),
        "wget" => wget::run(args),
        "kill" => kill::run(args),
        "gzip" => gzip::run(args),
        "gunzip" => gunzip::run(args),
        "tar" => tar::run(args),
        "untar" => untar::run(args),
        "timeout" => timeout::run(args),
        "watch" => watch::run(args),
        "xargs" => xargs::run(args),
        "parallel" => parallel::run(args),
        "install" => install::run(args),
        "shred" => shred::run(args),
        "command" => command::run(args),
        "type" => type_applet::run(args),
        "time" => time::run(args),
        "exit" => exit::run(args),
        "nice" => nice::run(args),
        "more" => more::run(args),
        "nm" => nm::run(args),
        "zcat" => zcat::run(args),
        _ => return None,
    })
}