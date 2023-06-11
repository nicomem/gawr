use std::path::PathBuf;

use clap::{
    builder::{PossibleValue, PossibleValuesParser},
    command, value_parser, Arg, ArgAction, ArgMatches, Command, ValueEnum, ValueHint,
};
use config::{builder::DefaultState, Config, ConfigBuilder, Environment, File, FileFormat};
use miette::{Context, IntoDiagnostic};
use regex::Regex;
use serde::{de::Visitor, Deserialize};

use crate::{
    my_regex,
    result::Result,
    types::{Bitrate, Extension},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Split {
    Full,
    Clips,
}

impl ValueEnum for Split {
    fn value_variants<'a>() -> &'a [Self] {
        &[Split::Full, Split::Clips]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        Some(match self {
            Split::Full => PossibleValue::new("full"),
            Split::Clips => PossibleValue::new("slow"),
        })
    }
}

#[derive(Debug)]
pub struct TracingLevel(pub tracing::Level);

const TRACING_LEVEL_LIST: &[&str] = &["ERROR", "WARN", "INFO", "DEBUG", "TRACE"];

struct TracingLevelVisitor;

impl<'de> Visitor<'de> for TracingLevelVisitor {
    type Value = TracingLevel;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a valid format level string")
    }

    fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(TracingLevel(match v.to_lowercase().as_str() {
            "trace" => tracing::Level::TRACE,
            "debug" => tracing::Level::DEBUG,
            "info" => tracing::Level::INFO,
            "warn" => tracing::Level::WARN,
            "error" => tracing::Level::ERROR,
            _ => return Err(E::custom(format!("{v} is not a valid log level"))),
        }))
    }
}

impl<'de> Deserialize<'de> for TracingLevel {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(TracingLevelVisitor)
    }
}

#[derive(Debug)]
pub struct AppArgs {
    pub ids: Vec<String>,
    pub clip_regex: Vec<Regex>,
    pub out: PathBuf,
    pub cache: PathBuf,
    pub split: Split,
    pub ext: Extension,
    pub shuffle: bool,
    pub cores: usize,
    pub log: TracingLevel,
    pub bitrate: Bitrate,
}

pub fn parse_cli() -> Result<AppArgs> {
    // Parse the command line arguments
    let clap_args = clap_app().get_matches();

    // Read the configuration file & environment
    let mut builder = Config::builder()
        .add_source(
            File::new(
                clap_args.get_one::<String>("config").unwrap(),
                FileFormat::Toml,
            )
            .required(false),
        )
        .add_source(
            Environment::with_prefix("GAWR")
                .ignore_empty(true)
                .try_parsing(true)
                .list_separator("<~>")
                .with_list_parse_key("id")
                .with_list_parse_key("clip_regex"),
        )
        .set_default("ext", "ogg")
        .into_diagnostic()?
        .set_default("cores", 0)
        .into_diagnostic()?
        .set_default("log", "INFO")
        .into_diagnostic()?
        .set_default("bitrate", 96)
        .into_diagnostic()?;

    override_list::<String>(&mut builder, &clap_args, "id")?;
    override_list::<String>(&mut builder, &clap_args, "clip_regex")?;
    override_single::<String>(&mut builder, &clap_args, "out")?;
    override_single::<String>(&mut builder, &clap_args, "cache")?;
    override_single::<String>(&mut builder, &clap_args, "split")?;
    override_single::<String>(&mut builder, &clap_args, "ext")?;
    override_single::<bool>(&mut builder, &clap_args, "shuffle")?;
    override_single::<u16>(&mut builder, &clap_args, "cores")?;
    override_single::<String>(&mut builder, &clap_args, "log")?;
    override_single::<u16>(&mut builder, &clap_args, "bitrate")?;

    let config = builder.build().into_diagnostic()?;

    let clip_regex = match config.get::<Vec<String>>("clip_regex") {
        Ok(v) => v
            .into_iter()
            .map(|s| {
                Ok(Regex::new(&s)
                    .into_diagnostic()
                    .wrap_err("Error while parsing regex")?)
            })
            .collect::<Result<_>>()?,
        Err(config::ConfigError::NotFound(_)) => my_regex::get_default_re_list().to_vec(),
        Err(e) => return Err(e).into_diagnostic()?,
    };

    Ok(AppArgs {
        ids: config.get("id").into_diagnostic()?,
        clip_regex,
        out: config.get("out").into_diagnostic()?,
        cache: config.get("cache").into_diagnostic()?,
        split: config.get("split").into_diagnostic()?,
        ext: config.get("ext").into_diagnostic()?,
        shuffle: config.get("shuffle").into_diagnostic()?,
        cores: config.get("cores").into_diagnostic()?,
        log: config.get("log").into_diagnostic()?,
        bitrate: config.get("bitrate").into_diagnostic()?,
    })
}

fn override_list<T>(
    builder: &mut ConfigBuilder<DefaultState>,
    clap_args: &ArgMatches,
    id: &str,
) -> Result<()>
where
    T: Clone + Send + Sync + 'static,
    config::ValueKind: From<T>,
{
    // Try to parse as a list
    if let Some(vals) = clap_args.try_get_many::<T>(id).ok().flatten() {
        *builder = std::mem::take(builder)
            .set_override(id, vals.cloned().collect::<Vec<T>>())
            .into_diagnostic()?;
    } else if let Some(val) = clap_args.try_get_one::<T>(id).into_diagnostic()? {
        // If that does not work, try to parse as a single value
        *builder = std::mem::take(builder)
            .set_override(id, vec![val.clone()])
            .into_diagnostic()?;
    }

    Ok(())
}

fn override_single<T>(
    builder: &mut ConfigBuilder<DefaultState>,
    clap_args: &ArgMatches,
    id: &str,
) -> Result<()>
where
    T: Clone + Send + Sync + 'static,
    config::ValueKind: From<T>,
{
    let arg_opt = match clap_args.try_get_one::<T>(id) {
        Ok(a) => a,
        Err(e) => match e {
            clap::parser::MatchesError::Downcast { .. } => {
                return Err(e)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("Argument {id} has invalid type"))?;
            }
            _ => {
                return Err(e)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("Error with value provided for argument {id}"))?
            }
        },
    };

    if let Some(val) = arg_opt {
        *builder = std::mem::take(builder)
            .set_override(id, val.clone())
            .into_diagnostic()?;
    }

    Ok(())
}

fn arg_base(name: &'static str) -> Arg {
    Arg::new(name).long(name).required(false)
}

fn arg_list(name: &'static str) -> Arg {
    arg_base(name).action(ArgAction::Append)
}

fn arg_single(name: &'static str) -> Arg {
    arg_base(name).action(ArgAction::Set)
}

fn arg_bool(name: &'static str) -> Arg {
    arg_base(name).action(ArgAction::SetTrue)
}

fn clap_app() -> Command {
    command!()
        .arg(
            arg_single("config")
                .default_value(".gawr.toml")
                .value_hint(ValueHint::FilePath)
                .help(help::CONFIG),
        )
        .arg(arg_list("id").help(help::ID))
        .arg(
            arg_single("out")
                .value_hint(ValueHint::DirPath)
                .help(help::OUT),
        )
        .arg(
            arg_single("cache")
                .value_hint(ValueHint::DirPath)
                .help(help::CACHE),
        )
        .arg(
            arg_single("split")
                .value_parser(value_parser!(Split))
                .ignore_case(true)
                .help(help::SPLIT),
        )
        .arg(
            arg_single("ext")
                .value_parser(value_parser!(Extension))
                .ignore_case(true)
                .help(help::EXT),
        )
        .arg(arg_list("clip_regex").help(help::CLIP_REGEX))
        .arg(arg_bool("shuffle").help(help::SHUFFLE))
        .arg(arg_single("cores").help(help::CORES))
        .arg(
            arg_single("log")
                .value_parser(PossibleValuesParser::new(TRACING_LEVEL_LIST))
                .ignore_case(true)
                .help(help::LOG),
        )
        .arg(arg_single("bitrate").help(help::BITRATE))
}

mod help {
    pub const CONFIG: &str = "The path to the TOML config file";
    pub const ID: &str = "The IDs of playlists or videos";
    pub const OUT: &str = "The path to the output directory";
    pub const CACHE: &str =
        "The path to the cache file, avoiding processing multiple times the same videos";
    pub const SPLIT: &str =
        "Either keep the entire video or create clips based on timestamps in the description";
    pub const EXT: &str =
        "The file extension to use for the output files. Defines the file container format to use";

    pub const CLIP_REGEX: &str = indoc::indoc! {"
        Regular expressions to extract timestamps from description.
        Must capture `time` and `title` groups (starting timestamp & clip title).
        
        For every description line, every pattern will be tested until one matches.
        A default pattern that should handle most cases is used if none is provided.
        
        Must use the [Regex crate syntax](https://docs.rs/regex/latest/regex/#syntax)
    "};

    pub const SHUFFLE: &str = "Randomize the order in which the videos are downloaded. Do not influence how clips are processed";
    pub const CORES: &str = indoc::indoc! {"
        Assume the machine has this number of cores. Used to modify the number of worker threads spawned.

        When using a value of 0 (default), auto-detect the number of cores from the system
    "};
    pub const LOG: &str = "The logging level to use";
    pub const BITRATE: &str =
        "The audio bitrate to use for output files. Must follow the `ffmpeg` bitrate format";
}
