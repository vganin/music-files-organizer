#![warn(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

use std::ops::Deref;
use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, Command};
use crate::command::add_covers::add_covers;
use crate::command::generate_completions::generate_completions;
use crate::command::import::import;
use crate::util::console_styleable::ConsoleStyleable;

mod cli;
mod command;
mod core;
mod discogs;
mod music_file;
mod tag;
mod util;

fn main() -> ExitCode {
    match try_main() {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error.deref().error_styled());
            error
                .chain()
                .skip(1)
                .for_each(|cause| eprintln!("{} {}", "↳".error_styled(), cause.error_styled()));
            eprintln!("\n{}", error.backtrace().error_styled());
            ExitCode::FAILURE
        }
    }
}

fn try_main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::GenerateCompletions(args) => generate_completions(args),
        Command::Import(args) => import(args, cli.discogs_token)?,
        Command::AddCovers(args) => add_covers(args, cli.discogs_token)?,
    }

    Ok(())
}
