use std::{fmt::Display, ops::Deref};

use convert_case::{Case, Casing};
use regex::Regex;

#[derive(Debug)]
pub struct Timestamp {
    pub t_start: String,
    pub title: String,
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:>8} - {}", self.t_start, self.title)
    }
}

#[derive(Debug)]
pub struct Timestamps(Vec<Timestamp>);

impl Deref for Timestamps {
    type Target = Vec<Timestamp>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Display for Timestamps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "[")?;
        for v in self.0.iter() {
            writeln!(f, "\t{v}")?;
        }
        writeln!(f, "]")?;
        Ok(())
    }
}

pub fn timestamp_to_seconds(tstamp: &str) -> u64 {
    let mut sec = 0;
    for n in tstamp.split(':').map(|s| s.parse::<u64>().unwrap()) {
        sec = 60 * sec + n;
    }
    sec
}

pub fn extract_timestamps(description: &str) -> Timestamps {
    let re = Regex::new(r#"([0-9]+(:[0-9]+)+) *-? +(.+)"#).unwrap();

    let timestamps = re
        .captures_iter(description)
        .map(|cap| {
            let title = cap.get(3).unwrap().as_str();
            let title = title.replace('"', "");

            Timestamp {
                t_start: cap.get(1).unwrap().as_str().to_owned(),
                title: title.to_case(Case::Title),
            }
        })
        .collect();

    Timestamps(timestamps)
}
