//! The first twelve applets, in implementation order.
//!
//! Each module is self-contained: it parses its own arguments, prints its own
//! diagnostics and returns an exit code (`0` success, `1` data error, `2` usage
//! error). There is no global option parser and no panic on bad input.

use alloc::string::String;

pub mod basename;
pub mod cat;
pub mod dirname;
pub mod echo;
pub mod false_applet;
pub mod head;
pub mod pwd;
pub mod seq;
pub mod tail;
pub mod true_applet;
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
        _ => return None,
    })
}