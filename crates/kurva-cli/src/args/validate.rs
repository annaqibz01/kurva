//! Validation and default resolution for parsed flags.

use std::path::PathBuf;

use anyhow::{bail, Result};

use super::spec::{
    Preset, Verbosity, DEFAULT_COLORS, DEFAULT_CORNER_THRESHOLD, DEFAULT_EPSILON,
    DEFAULT_ERROR_THRESHOLD, DEFAULT_PRECISION, MAX_COLORS, MAX_PRECISION, MIN_PRECISION, USAGE,
};
use super::CliConfig;

/// Accumulator for one parse pass; unset numeric fields inherit from the preset.
#[derive(Debug, Default)]
pub(super) struct Raw {
    pub(super) input: Option<PathBuf>,
    pub(super) output: Option<PathBuf>,
    pub(super) colors: Option<usize>,
    pub(super) error_threshold: Option<f64>,
    pub(super) corner_threshold: Option<f64>,
    pub(super) epsilon: Option<f64>,
    pub(super) precision: Option<u32>,
    pub(super) preset: Option<Preset>,
    pub(super) verbose: bool,
    pub(super) quiet: bool,
}

impl Raw {
    /// Fold the accumulated flags into a validated configuration.
    pub(super) fn resolve(self) -> Result<CliConfig> {
        let (p_colors, p_error, p_corner, p_epsilon) = self.preset.map_or(
            (
                DEFAULT_COLORS,
                DEFAULT_ERROR_THRESHOLD,
                DEFAULT_CORNER_THRESHOLD,
                DEFAULT_EPSILON,
            ),
            Preset::tuning,
        );
        let precision = self.precision.unwrap_or(DEFAULT_PRECISION);
        if !(MIN_PRECISION..=MAX_PRECISION).contains(&precision) {
            bail!("[ERROR] --precision must be 2 or 3, got {precision}");
        }
        if self.verbose && self.quiet {
            bail!("[ERROR] --verbose and --quiet are mutually exclusive");
        }
        let verbosity = match (self.quiet, self.verbose) {
            (true, _) => Verbosity::Quiet,
            (false, true) => Verbosity::Verbose,
            (false, false) => Verbosity::Normal,
        };
        let (Some(input), Some(output)) = (self.input, self.output) else {
            bail!("[ERROR] Both --input and --output are required\n\n{USAGE}");
        };
        Ok(CliConfig {
            input,
            output,
            colors: self.colors.unwrap_or(p_colors).clamp(1, MAX_COLORS),
            error_threshold: self.error_threshold.unwrap_or(p_error),
            corner_threshold: self.corner_threshold.unwrap_or(p_corner),
            epsilon: self.epsilon.unwrap_or(p_epsilon),
            precision,
            preset: self.preset,
            verbosity,
        })
    }
}
