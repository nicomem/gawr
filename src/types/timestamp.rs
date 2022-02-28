use std::{fmt::Display, ops::Deref};

use heck::ToTitleCase;
use regex::Regex;

#[derive(Debug)]
pub struct Timestamp {
    pub t_start: String,
    pub title: String,
}

impl Timestamp {
    pub fn to_seconds(tstamp: &str) -> u64 {
        let mut sec = 0;
        for n in tstamp.split(':').map(|s| s.parse::<u64>().unwrap()) {
            sec = 60 * sec + n;
        }
        sec
    }
}

impl Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:>8} - {}", self.t_start, self.title)
    }
}

#[derive(Debug)]
pub struct Timestamps(Vec<Timestamp>);

impl Timestamps {
    pub fn new(data: Vec<Timestamp>) -> Self {
        Self(data)
    }

    pub fn extract_timestamps(description: &str, clip_regex: &[Regex]) -> Self {
        // For every line, try every regex until one matches
        let captures = description
            .lines()
            .map(str::trim)
            .flat_map(|line| clip_regex.iter().flat_map(|re| re.captures(line)).next());

        // For every line that matched one regex, construct the timestamp
        let timestamps = captures
            .map(|cap| {
                let title = cap.name("title").unwrap().as_str();
                let t_start = cap.name("time").unwrap().as_str();

                // Remove potentially problematic characters from the title
                let title = title
                    .split(['\'', '"', '/', '\\', '|', '~', '$', '#'])
                    .map(|s| s.trim())
                    .collect::<Vec<_>>()
                    .join(" ");

                Timestamp {
                    t_start: t_start.to_owned(),
                    title: title.to_title_case(),
                }
            })
            .collect();

        Timestamps::new(timestamps)
    }
}

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
