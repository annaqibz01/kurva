//! End-to-end smoke tests driving the compiled `kurva` binary.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn kurva(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kurva"))
        .args(args)
        .output()
        .expect("failed to spawn kurva binary")
}

/// Scratch directory private to one test invocation.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kurva-cli-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("failed to create scratch directory");
    dir
}

/// Write a deterministic two-block PNG: left half deep red, right half deep blue.
fn write_fixture(dir: &Path) -> PathBuf {
    let side = 16_u32;
    let mut image = image::RgbImage::new(side, side);
    for (x, _, pixel) in image.enumerate_pixels_mut() {
        *pixel = if x < side / 2 {
            image::Rgb([200, 20, 20])
        } else {
            image::Rgb([20, 20, 200])
        };
    }
    let path = dir.join("fixture.png");
    image.save(&path).expect("failed to write PNG fixture");
    path
}

#[test]
fn help_prints_usage_and_succeeds() {
    let output = kurva(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("USAGE"), "usage banner missing: {stdout}");
    assert!(stdout.contains("--error-threshold"));
    assert!(stdout.contains("--corner-threshold"));
    assert!(stdout.contains("--epsilon"));
}

#[test]
fn version_reports_the_crate_version() {
    let output = kurva(&["-V"]);
    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("kurva "), "stdout was: {stdout}");
    assert!(stdout.contains("0.1.0"));
}

#[test]
fn invalid_precision_is_rejected() {
    let output = kurva(&["-i", "a.png", "-o", "b.svg", "--precision", "4"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--precision"), "stderr was: {stderr}");
}

#[test]
fn missing_required_arguments_fail_with_usage() {
    let output = kurva(&["-o", "out.svg"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("[ERROR]"), "stderr was: {stderr}");
    assert!(stderr.contains("USAGE"), "usage banner missing: {stderr}");
}

#[test]
fn pipeline_renders_svg_from_a_png() {
    let dir = scratch("pipeline");
    let input = write_fixture(&dir);
    let output = dir.join("out.svg");

    let result = kurva(&[
        "-i",
        input.to_str().expect("input path is UTF-8"),
        "-o",
        output.to_str().expect("output path is UTF-8"),
        "-c",
        "2",
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let svg = std::fs::read_to_string(&output).expect("SVG output was not written");
    assert!(svg.starts_with("<svg"), "unexpected document head");
    assert!(svg.trim_end().ends_with("</svg>"), "document not closed");
    assert!(svg.contains("<path"), "no vector path emitted");
}

#[test]
fn unreadable_input_reports_a_contextual_error() {
    let dir = scratch("missing");
    let output = kurva(&[
        "-i",
        dir.join("absent.png").to_str().expect("path is UTF-8"),
        "-o",
        dir.join("out.svg").to_str().expect("path is UTF-8"),
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Failed to read input file"),
        "was: {stderr}"
    );
}

#[test]
fn preset_precision_and_quiet_control_the_run() {
    let dir = scratch("preset");
    let input = write_fixture(&dir);
    let output = dir.join("out.svg");
    let result = kurva(&[
        "--preset",
        "fast",
        "--precision",
        "3",
        "-q",
        "-i",
        input.to_str().expect("input path is UTF-8"),
        "-o",
        output.to_str().expect("output path is UTF-8"),
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        result.stdout.is_empty(),
        "quiet mode must not log: {}",
        String::from_utf8_lossy(&result.stdout)
    );
    let svg = std::fs::read_to_string(&output).expect("SVG output was not written");
    assert!(svg.starts_with("<svg"));
    assert!(svg.contains("<path"));
}

#[test]
fn verbose_run_reports_stage_timings_and_throughput() {
    let dir = scratch("verbose");
    let input = write_fixture(&dir);
    let output = dir.join("out.svg");
    let result = kurva(&[
        "-v",
        "-i",
        input.to_str().expect("input path is UTF-8"),
        "-o",
        output.to_str().expect("output path is UTF-8"),
    ]);
    assert_eq!(result.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&result.stdout);
    for stage in ["color", "contour", "poly", "fit", "svg"] {
        assert!(
            stdout.contains(&format!("stage {stage}:")),
            "missing {stage}"
        );
    }
    assert!(stdout.contains("MP/s"), "throughput metric missing");
    assert!(stdout.contains("bezier spans:"), "span metric missing");
}
