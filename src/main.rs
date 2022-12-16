#![warn(
clippy::unwrap_used,
clippy::panic,
clippy::expect_used,
)]

extern crate core;

use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use crate::command::add_covers::add_covers;
use crate::command::fsync::fsync;
use crate::command::import::import;
use crate::discogs::client::DiscogsClient;
use crate::tag::Tag;
use crate::util::console::Console;
use crate::util::console_styleable::ConsoleStyleable;

mod tag;
mod command;
mod util;
mod discogs;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(long)]
    discogs_token: Option<String>,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Import(ImportArgs),
    AddCovers(AddCoversArguments),
    Fsync(FsyncArguments),
}

#[derive(Args)]
pub struct ImportArgs {
    #[clap(long, parse(from_os_str))]
    from: PathBuf,

    #[clap(long, parse(from_os_str))]
    to: PathBuf,

    #[clap(long)]
    dont_clean_target_folders: bool,

    #[clap(long)]
    clean_source_folders: bool,
}

#[derive(Args)]
pub struct AddCoversArguments {
    #[clap(long, parse(from_os_str))]
    to: PathBuf,

    #[clap(long)]
    skip_if_present: bool,
}

#[derive(Args)]
pub struct FsyncArguments {
    #[clap(parse(from_os_str))]
    path: PathBuf,
}

fn main() -> ExitCode {
    match main_with_result() {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", error.deref().error_styled());
            error.chain().skip(1).for_each(|cause| eprintln!("{} {}", "↳".error_styled(), cause.error_styled()));
            eprintln!("\n{}", error.backtrace().error_styled());
            ExitCode::FAILURE
        }
    }
}

fn main_with_result() -> Result<()> {
    let cli = Cli::parse();

    let discogs_token = match cli.discogs_token {
        Some(x) => x,
        None => {
            let discogs_token_file = get_discogs_token_file_path()
                .with_context(|| format!("Supply discogs token with commandline argument (refer to --help) or with the file ~/{}", DISCOGS_TOKEN_FILE_NAME))?;
            fs::read_to_string(discogs_token_file)?.trim().to_owned()
        }
    };

    let discogs_client = DiscogsClient::new(&discogs_token)?;

    let mut console = Console::new();

    match cli.command {
        Command::Import(args) => import(args, &discogs_client, &mut console)?,
        Command::AddCovers(args) => add_covers(args, &discogs_client, &mut console)?,
        Command::Fsync(args) => fsync(args, &mut console)?,
    }

    Ok(())
}

const DISCOGS_TOKEN_FILE_NAME: &str = ".discogs_token";

fn get_discogs_token_file_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(DISCOGS_TOKEN_FILE_NAME))
}
