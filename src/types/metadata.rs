use std::{collections::HashMap, fmt::Display, ops::Deref};

#[derive(Debug)]
pub struct Metadata(HashMap<String, String>);

impl Metadata {
    pub fn new(data: HashMap<String, String>) -> Self {
        Self(data)
    }
}

impl Display for Metadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "{{")?;
        for (k, v) in self.iter() {
            writeln!(f, "\t{k}: {v}")?;
        }
        writeln!(f, "}}")?;
        Ok(())
    }
}

impl Deref for Metadata {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
