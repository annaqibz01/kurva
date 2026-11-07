//! Command-line specification: usage text, defaults, presets and verbosity.

use anyhow::{bail, Result};

/// Palette ceiling enforced by the quantizer.
pub(super) const MAX_COLORS: usize = 256;
/// Lowest ALTO-v3 decimal lattice scale.
pub(super) const MIN_PRECISION: u32 = 2;
/// Highest ALTO-v3 decimal lattice scale.
pub(super) const MAX_PRECISION: u32 = 3;

pub(super) const DEFAULT_COLORS: usize = 4;
pub(super) const DEFAULT_ERROR_THRESHOLD: f64 = 2.0;
pub(super) const DEFAULT_CORNER_THRESHOLD: f64 = 120.0;
pub(super) const DEFAULT_EPSILON: f64 = 1.0;
pub(super) const DEFAULT_PRECISION: u32 = MIN_PRECISION;

pub const USAGE: &str = "\
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
        --precision <2|3>          Coordinate decimal scale [default: 2]
        --preset <NAME>            logo | illustration | fast parameter bundle
    -v, --verbose                  Log per-stage timings and metrics
    -q, --quiet                    Suppress informational output
    -h, --help                     Print this message
    -V, --version                  Print version";

/// Log verbosity selected on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

/// Turnkey parameter bundles for common artwork classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Logo,
    Illustration,
    Fast,
}

impl Preset {
    pub(super) fn parse(value: &str) -> Result<Self> {
        match value {
            "logo" => Ok(Self::Logo),
            "illustration" => Ok(Self::Illustration),
            "fast" => Ok(Self::Fast),
            other => bail!("[ERROR] Unknown preset: {other} (expected logo, illustration or fast)"),
        }
    }

    /// `(colors, error_threshold, corner_threshold, epsilon)` for this artwork class.
    pub fn tuning(self) -> (usize, f64, f64, f64) {
        match self {
            Self::Logo => (8, 1.5, 110.0, 0.8),
            Self::Illustration => (16, 2.5, 130.0, 1.2),
            Self::Fast => (4, 3.0, 120.0, 1.5),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Logo => "logo",
            Self::Illustration => "illustration",
            Self::Fast => "fast",
        }
    }
}
