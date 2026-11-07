//! Wall-clock measurement and technical run reporting.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::args::Verbosity;

/// Wall-clock durations for each pipeline stage plus the whole run.
#[derive(Debug, Default)]
pub struct StageTimes {
    pub color: Duration,
    pub contour: Duration,
    pub poly: Duration,
    pub fit: Duration,
    pub svg: Duration,
    pub total: Duration,
}

impl StageTimes {
    /// Per-stage milliseconds in execution order.
    pub fn stages(&self) -> [(&'static str, f64); 5] {
        [
            ("color", millis(self.color)),
            ("contour", millis(self.contour)),
            ("poly", millis(self.poly)),
            ("fit", millis(self.fit)),
            ("svg", millis(self.svg)),
        ]
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Monotonic stopwatch; every `lap` returns the time since the previous one.
#[derive(Debug)]
pub struct Stopwatch {
    origin: Instant,
    last: Instant,
}

impl Stopwatch {
    pub fn start() -> Self {
        let now = Instant::now();
        Self {
            origin: now,
            last: now,
        }
    }

    pub fn lap(&mut self) -> Duration {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last);
        self.last = now;
        elapsed
    }

    pub fn total(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// Counters describing the artefacts produced by one run.
#[derive(Debug, Default)]
pub struct RunStats {
    pub width: usize,
    pub height: usize,
    pub palette: usize,
    pub contours: usize,
    pub vertices: usize,
    pub spans: usize,
    pub paths: usize,
    pub svg_bytes: usize,
    pub times: StageTimes,
}

impl RunStats {
    /// Megapixels processed per second over the whole run.
    pub fn throughput(&self) -> f64 {
        let megapixels = (self.width.saturating_mul(self.height)) as f64 / 1_000_000.0;
        let seconds = self.times.total.as_secs_f64();
        if seconds > 0.0 {
            megapixels / seconds
        } else {
            0.0
        }
    }

    /// Emit the report for the selected verbosity; quiet prints nothing.
    pub fn report(&self, verbosity: Verbosity, input: &Path, output: &Path) {
        if self.paths == 0 {
            println!("[WARN] no vector paths produced from {}", input.display());
        }
        let summary = format!(
            "[INFO] kurva: {} -> {} ({} paths, {} colors, {:.2} ms)",
            input.display(),
            output.display(),
            self.paths,
            self.palette,
            millis(self.times.total)
        );
        match verbosity {
            Verbosity::Quiet => {}
            Verbosity::Normal => println!("{summary}"),
            Verbosity::Verbose => {
                println!(
                    "[INFO] canvas: {}x{} ({} px), palette: {} colors",
                    self.width,
                    self.height,
                    self.width.saturating_mul(self.height),
                    self.palette
                );
                for (stage, elapsed) in self.times.stages() {
                    println!("[INFO] stage {stage}: {elapsed:.2} ms");
                }
                println!(
                    "[INFO] contours: {} ({} simplified vertices)",
                    self.contours, self.vertices
                );
                println!(
                    "[INFO] bezier spans: {}, output: {} bytes",
                    self.spans, self.svg_bytes
                );
                println!(
                    "[INFO] total: {:.2} ms ({:.2} MP/s)",
                    millis(self.times.total),
                    self.throughput()
                );
                println!("{summary}");
            }
        }
    }
}
