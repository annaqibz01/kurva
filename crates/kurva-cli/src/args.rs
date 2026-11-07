//! Zero-dependency command-line parser for the `kurva` binary.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};

mod spec;
mod validate;

pub use spec::{Preset, Verbosity, USAGE};
use validate::Raw;

/// Resolved invocation parameters.
#[derive(Debug, Clone)]
pub struct CliConfig {
    pub input: PathBuf,
    pub output: PathBuf,
    pub colors: usize,
    pub error_threshold: f64,
    pub corner_threshold: f64,
    pub epsilon: f64,
    pub precision: u32,
    pub preset: Option<Preset>,
    pub verbosity: Verbosity,
}

/// Outcome of parsing: run the pipeline, or print information and stop.
#[derive(Debug)]
pub enum Command {
    Run(Box<CliConfig>),
    Help,
    Version,
}

/// Parse a numeric flag value, attributing failures to the offending argument.
fn number<T: std::str::FromStr>(flag: &str, value: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    value
        .parse::<T>()
        .map_err(|error| anyhow!("[ERROR] Invalid value for {flag}: {value} ({error})"))
}

impl CliConfig {
    /// Parse the argument list (excluding the program name).
    pub fn parse(argv: impl IntoIterator<Item = String>) -> Result<Command> {
        let argv: Vec<String> = argv.into_iter().collect();
        let mut raw = Raw::default();
        let mut index = 0;
        while index < argv.len() {
            let flag = argv[index].as_str();
            match flag {
                "-h" | "--help" => return Ok(Command::Help),
                "-V" | "--version" => return Ok(Command::Version),
                "-v" | "--verbose" => {
                    raw.verbose = true;
                    index += 1;
                    continue;
                }
                "-q" | "--quiet" => {
                    raw.quiet = true;
                    index += 1;
                    continue;
                }
                _ => {}
            }
            let value = argv
                .get(index + 1)
                .ok_or_else(|| anyhow!("[ERROR] Missing value for argument: {flag}"))?;
            match flag {
                "-i" | "--input" => raw.input = Some(PathBuf::from(value)),
                "-o" | "--output" => raw.output = Some(PathBuf::from(value)),
                "-c" | "--colors" => raw.colors = Some(number::<usize>(flag, value)?),
                "--error-threshold" => raw.error_threshold = Some(number(flag, value)?),
                "--corner-threshold" => raw.corner_threshold = Some(number(flag, value)?),
                "--epsilon" => raw.epsilon = Some(number(flag, value)?),
                "--precision" => raw.precision = Some(number(flag, value)?),
                "--preset" => raw.preset = Some(Preset::parse(value)?),
                other => bail!("[ERROR] Unknown argument: {other}\n\n{USAGE}"),
            }
            index += 2;
        }
        raw.resolve().map(|config| Command::Run(Box::new(config)))
    }
}

#[cfg(test)]
mod tests;
