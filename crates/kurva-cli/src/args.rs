//! Zero-dependency command-line parser for the `kurva` binary.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};

/// Palette ceiling enforced by the quantizer.
const MAX_COLORS: usize = 256;

/// Default maximum palette size (PRD FR-7.1).
const DEFAULT_COLORS: usize = 4;
/// Default Bezier fitting tolerance in pixels.
const DEFAULT_ERROR_THRESHOLD: f64 = 2.0;
/// Default cusp classification angle in degrees.
const DEFAULT_CORNER_THRESHOLD: f64 = 120.0;
/// Default RDP polyline tolerance in pixels.
const DEFAULT_EPSILON: f64 = 1.0;

const USAGE: &str = "\
kurva - raster image to SVG vectorizer

USAGE:
    kurva -i <INPUT> -o <OUTPUT> [OPTIONS]

OPTIONS:
    -i, --input <PATH>             Input raster image (PNG or JPEG)
    -o, --output <PATH>            Output SVG file path
    -c, --colors <INT>             Maximum palette colors [default: 4, range: 1..=256]
        --error-threshold <FLOAT>  Bezier fitting tolerance [default: 2.0]
        --corner-threshold <FLOAT> Cusp angle in degrees [default: 120.0]
        --epsilon <FLOAT>          RDP polyline tolerance in pixels [default: 1.0]
    -h, --help                     Print this message";

/// Resolved invocation parameters.
#[derive(Debug, Clone)]
pub struct Args {
    pub input: PathBuf,
    pub output: PathBuf,
    pub colors: usize,
    pub error_threshold: f64,
    pub corner_threshold: f64,
    pub epsilon: f64,
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

/// Parse `argv` (without the program name) into [`Args`].
///
/// Returns `Ok(None)` when help was requested, so the caller exits successfully
/// without running the pipeline.
pub fn parse(argv: Vec<String>) -> Result<Option<Args>> {
    let mut input: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut colors = DEFAULT_COLORS;
    let mut error_threshold = DEFAULT_ERROR_THRESHOLD;
    let mut corner_threshold = DEFAULT_CORNER_THRESHOLD;
    let mut epsilon = DEFAULT_EPSILON;
    let mut index = 0;
    while index < argv.len() {
        let flag = argv[index].as_str();
        if flag == "-h" || flag == "--help" {
            println!("{USAGE}");
            return Ok(None);
        }
        let value = argv
            .get(index + 1)
            .with_context(|| format!("[ERROR] Missing value for argument: {flag}"))?;
        match flag {
            "-i" | "--input" => input = Some(PathBuf::from(value)),
            "-o" | "--output" => output = Some(PathBuf::from(value)),
            "-c" | "--colors" => colors = number::<usize>(flag, value)?.clamp(1, MAX_COLORS),
            "--error-threshold" => error_threshold = number(flag, value)?,
            "--corner-threshold" => corner_threshold = number(flag, value)?,
            "--epsilon" => epsilon = number(flag, value)?,
            other => bail!("[ERROR] Unknown argument: {other}\n\n{USAGE}"),
        }
        index += 2;
    }
    let (Some(input), Some(output)) = (input, output) else {
        bail!("[ERROR] Both --input and --output are required\n\n{USAGE}");
    };
    Ok(Some(Args {
        input,
        output,
        colors,
        error_threshold,
        corner_threshold,
        epsilon,
    }))
}

#[cfg(test)]
mod tests {
    use super::parse;

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    fn parsed(items: &[&str]) -> super::Args {
        let outcome = parse(argv(items));
        let args = match outcome {
            Ok(Some(args)) => args,
            Ok(None) => panic!("help requested for pipeline arguments"),
            Err(error) => panic!("expected valid arguments, got {error}"),
        };
        args
    }

    #[test]
    fn defaults_follow_the_prd() {
        let args = parsed(&["-i", "in.png", "-o", "out.svg"]);
        assert_eq!(args.colors, 4);
        assert!((args.error_threshold - 2.0).abs() < f64::EPSILON);
        assert!((args.corner_threshold - 120.0).abs() < f64::EPSILON);
        assert!((args.epsilon - 1.0).abs() < f64::EPSILON);
        assert_eq!(args.input.to_string_lossy(), "in.png");
        assert_eq!(args.output.to_string_lossy(), "out.svg");
    }

    #[test]
    fn colors_are_clamped_into_the_palette_range() {
        let low = parsed(&["-i", "a.png", "-o", "b.svg", "-c", "0"]);
        assert_eq!(low.colors, 1);
        let high = parsed(&["--input", "a.png", "--output", "b.svg", "--colors", "4096"]);
        assert_eq!(high.colors, 256);
        let exact = parsed(&["-i", "a.png", "-o", "b.svg", "-c", "16"]);
        assert_eq!(exact.colors, 16);
    }

    #[test]
    fn numeric_overrides_are_applied() {
        let args = parsed(&[
            "-i",
            "a.png",
            "-o",
            "b.svg",
            "--error-threshold",
            "0.5",
            "--corner-threshold",
            "90",
            "--epsilon",
            "0.25",
        ]);
        assert!((args.error_threshold - 0.5).abs() < f64::EPSILON);
        assert!((args.corner_threshold - 90.0).abs() < f64::EPSILON);
        assert!((args.epsilon - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn required_arguments_are_enforced() {
        assert!(parse(Vec::new()).is_err());
        assert!(parse(argv(&["-i", "a.png"])).is_err());
        assert!(parse(argv(&["-o", "b.svg"])).is_err());
    }

    #[test]
    fn malformed_input_is_rejected() {
        assert!(parse(argv(&["-i", "a.png", "-o", "b.svg", "-c"])).is_err());
        assert!(parse(argv(&["-i", "a.png", "-o", "b.svg", "-c", "abc"])).is_err());
        assert!(parse(argv(&["-i", "a.png", "-o", "b.svg", "--bogus", "1"])).is_err());
        assert!(parse(argv(&["-i", "a.png", "-o", "b.svg", "--epsilon", ""])).is_err());
    }

    #[test]
    fn help_short_circuits_the_pipeline() {
        assert!(matches!(parse(argv(&["-h"])), Ok(None)));
        assert!(matches!(parse(argv(&["--help"])), Ok(None)));
    }
}
