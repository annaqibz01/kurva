//! Parser unit tests: flag handling, presets and rejection paths.

use super::{CliConfig, Command, Preset, Verbosity};

fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn config(items: &[&str]) -> CliConfig {
    match CliConfig::parse(argv(items)) {
        Ok(Command::Run(config)) => *config,
        Ok(other) => panic!("expected a run command, got {other:?}"),
        Err(error) => panic!("expected valid arguments, got {error}"),
    }
}

fn parse_error(items: &[&str]) -> String {
    match CliConfig::parse(argv(items)) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("expected a parse error for {items:?}"),
    }
}

#[test]
fn defaults_follow_the_prd() {
    let args = config(&["-i", "in.png", "-o", "out.svg"]);
    assert_eq!(args.colors, 4);
    assert!((args.error_threshold - 2.0).abs() < f64::EPSILON);
    assert!((args.corner_threshold - 120.0).abs() < f64::EPSILON);
    assert!((args.epsilon - 1.0).abs() < f64::EPSILON);
    assert_eq!(args.precision, 2);
    assert_eq!(args.preset, None);
    assert_eq!(args.verbosity, Verbosity::Normal);
}

#[test]
fn presets_tune_every_parameter() {
    let logo = config(&["-i", "a.png", "-o", "b.svg", "--preset", "logo"]);
    assert_eq!(logo.preset, Some(Preset::Logo));
    assert_eq!(logo.colors, 8);
    assert!((logo.error_threshold - 1.5).abs() < f64::EPSILON);
    assert!((logo.corner_threshold - 110.0).abs() < f64::EPSILON);
    assert!((logo.epsilon - 0.8).abs() < f64::EPSILON);

    let illustration = config(&["-i", "a.png", "-o", "b.svg", "--preset", "illustration"]);
    assert_eq!(illustration.colors, 16);
    assert!((illustration.error_threshold - 2.5).abs() < f64::EPSILON);
    assert!((illustration.corner_threshold - 130.0).abs() < f64::EPSILON);
    assert!((illustration.epsilon - 1.2).abs() < f64::EPSILON);

    let fast = config(&["-i", "a.png", "-o", "b.svg", "--preset", "fast"]);
    assert_eq!(fast.colors, 4);
    assert!((fast.error_threshold - 3.0).abs() < f64::EPSILON);
    assert!((fast.epsilon - 1.5).abs() < f64::EPSILON);

    let message = parse_error(&["-i", "a.png", "-o", "b.svg", "--preset", "poster"]);
    assert!(message.contains("Unknown preset"), "{message}");
}

#[test]
fn explicit_flags_override_the_preset_in_any_order() {
    let before = config(&[
        "--epsilon",
        "0.1",
        "--preset",
        "logo",
        "-i",
        "a.png",
        "-o",
        "b.svg",
    ]);
    let after = config(&[
        "--preset",
        "logo",
        "--epsilon",
        "0.1",
        "-i",
        "a.png",
        "-o",
        "b.svg",
    ]);
    for args in [before, after] {
        assert!((args.epsilon - 0.1).abs() < f64::EPSILON);
        assert_eq!(args.colors, 8);
    }
}

#[test]
fn precision_accepts_two_and_three_only() {
    let strict = config(&["-i", "a.png", "-o", "b.svg", "--precision", "3"]);
    assert_eq!(strict.precision, 3);
    let message = parse_error(&["-i", "a.png", "-o", "b.svg", "--precision", "4"]);
    assert!(message.contains("--precision"), "{message}");
    let message = parse_error(&["-i", "a.png", "-o", "b.svg", "--precision", "1"]);
    assert!(message.contains("--precision"), "{message}");
}

#[test]
fn verbosity_flags_are_parsed_and_conflicts_rejected() {
    let quiet = config(&["-i", "a.png", "-o", "b.svg", "-q"]);
    assert_eq!(quiet.verbosity, Verbosity::Quiet);
    let verbose = config(&["-i", "a.png", "-o", "b.svg", "--verbose"]);
    assert_eq!(verbose.verbosity, Verbosity::Verbose);
    let message = parse_error(&["-i", "a.png", "-o", "b.svg", "-q", "-v"]);
    assert!(message.contains("mutually exclusive"), "{message}");
}

#[test]
fn colors_are_clamped_into_the_palette_range() {
    let low = config(&["-i", "a.png", "-o", "b.svg", "-c", "0"]);
    assert_eq!(low.colors, 1);
    let high = config(&["-i", "a.png", "-o", "b.svg", "-c", "4096"]);
    assert_eq!(high.colors, 256);
    let exact = config(&["--input", "a.png", "--output", "b.svg", "--colors", "16"]);
    assert_eq!(exact.colors, 16);
}

#[test]
fn required_arguments_are_enforced() {
    assert!(parse_error(&["-i", "a.png"]).contains("--output"));
    assert!(parse_error(&["-o", "b.svg"]).contains("--input"));
    assert!(parse_error(&[]).contains("required"));
}

#[test]
fn malformed_input_is_rejected() {
    let missing = parse_error(&["-i", "a.png", "-o", "b.svg", "-c"]);
    assert!(missing.contains("Missing value"), "{missing}");
    let invalid = parse_error(&["-i", "a.png", "-o", "b.svg", "--epsilon", "wide"]);
    assert!(invalid.contains("Invalid value"), "{invalid}");
    let unknown = parse_error(&["-i", "a.png", "-o", "b.svg", "--bogus", "1"]);
    assert!(unknown.contains("Unknown argument"), "{unknown}");
}

#[test]
fn help_and_version_short_circuit() {
    assert!(matches!(CliConfig::parse(argv(&["-h"])), Ok(Command::Help)));
    assert!(matches!(
        CliConfig::parse(argv(&["--help"])),
        Ok(Command::Help)
    ));
    assert!(matches!(
        CliConfig::parse(argv(&["-V"])),
        Ok(Command::Version)
    ));
    assert!(matches!(
        CliConfig::parse(argv(&["--version"])),
        Ok(Command::Version)
    ));
}
