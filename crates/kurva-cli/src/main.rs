//! `kurva` entry point: configuration dispatch and diagnostic formatting.

mod args;
mod pipeline;
mod stats;

use std::process::ExitCode;

use anyhow::Result;

use crate::args::{CliConfig, Command};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<()> {
    match CliConfig::parse(std::env::args().skip(1))? {
        Command::Help => {
            println!("{}", args::USAGE);
            Ok(())
        }
        Command::Version => {
            println!("kurva {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Run(config) => pipeline::run(&config),
    }
}
