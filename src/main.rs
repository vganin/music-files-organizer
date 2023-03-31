#![warn(clippy::unwrap_used, clippy::panic, clippy::expect_used)]

extern crate core;

use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use crate::cli::{Cli, Command};
use crate::command::add_covers::add_covers;
use crate::command::fsync::fsync;
use crate::command::generate_completions::generate_completions;
use crate::command::import::import;
use crate::discogs::matcher::DiscogsMatcher;
use crate::util::console::Console;
use crate::util::console_styleable::ConsoleStyleable;

mod cli;
mod command;
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

    let discogs_token = match cli.discogs_token {
        Some(x) => x,
        None => {
            let discogs_token_file = get_discogs_token_file_path()
                .with_context(|| format!("Supply discogs token with commandline argument (refer to --help) or with the file ~/{}", DISCOGS_TOKEN_FILE_NAME))?;
            fs::read_to_string(discogs_token_file)?.trim().to_owned()
        }
    };

    let discogs_matcher = DiscogsMatcher::new(&discogs_token)?;

    let mut console = Console::new();

    match cli.command {
        Command::GenerateCompletions(args) => generate_completions(args),
        Command::Import(args) => import(args, &discogs_matcher, &mut console)?,
        Command::AddCovers(args) => add_covers(args, &discogs_matcher, &mut console)?,
        Command::Fsync(args) => fsync(args, &mut console)?,
    }

    Ok(())
}

fn get_discogs_token_file_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(DISCOGS_TOKEN_FILE_NAME))
}

const DISCOGS_TOKEN_FILE_NAME: &str = ".discogs_token";
