use std::sync::OnceLock;

use regex::Regex;

// To whomever reads this asking what an abomination this is
// ...
// at least it is not one big uncommented regex string

/// An optional index from an enumerated list
macro_rules! opt_idx {
    () => {
        r#"(?:\d+\. *)?"#
    };
}
/// The clip title
macro_rules! title {
    () => {
        r#"(?P<title>.+)"#
    };
}
/// The clip timestamp
macro_rules! tstamp_start {
    () => {
        r#"(?P<time>[0-9]+(:[0-9]+)+)"#
    };
}
/// An optional separator
macro_rules! opt_sep {
    () => {
        r#" *.? +"#
    };
}
/// An optional second timestamp, indicating the end of the clip
macro_rules! opt_tstamp_end {
    () => {
        concat!(opt_sep!(), r#"(?:[0-9]+(:[0-9]+)+)?"#)
    };
}
macro_rules! timestamp {
    () => {
        concat!(tstamp_start!(), opt_tstamp_end!())
    };
}
/// Pattern 1: An optional index, the timestamp, an optional separator, the title
/// Example: "6:66 Music That Will Make You Go Insane !!!"
const PATTERN1: &str = concat!("^", opt_idx!(), timestamp!(), opt_sep!(), title!(), "$");

/// Pattern 2: An optional index, the title, an optional separator, the timestamp
/// Example: "5. My Very Cool Title - 05:49"
const PATTERN2: &str = concat!("^", opt_idx!(), title!(), opt_sep!(), timestamp!(), "$");

static DEFAULT_RE_LIST: OnceLock<[Regex; 2]> = OnceLock::new();

pub fn get_default_re_list() -> &'static [Regex] {
    DEFAULT_RE_LIST.get_or_init(|| [Regex::new(PATTERN1).unwrap(), Regex::new(PATTERN2).unwrap()])
}
