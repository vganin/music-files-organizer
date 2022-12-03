extern crate core;

use std::fs;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use crate::command::add_missing_covers::add_missing_covers;
use crate::command::import::import;
use crate::tag::Tag;
use crate::util::console::Console;
use crate::util::discogs::DiscogsClient;

mod tag;
mod command;
mod util;

const DISCOGS_TOKEN_FILE_NAME: &str = ".discogs_token";

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
    AddMissingCovers(AddMissingCoversArgs),
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
pub struct AddMissingCoversArgs {
    #[clap(long, parse(from_os_str))]
    to: PathBuf,

    #[clap(long)]
    force_update: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let discogs_token = match cli.discogs_token {
        Some(x) => x.to_owned(),
        None => {
            let discogs_token_file = get_discogs_token_file_path()
                .expect("Supply discogs token with commandline argument (refer to --help)");
            fs::read_to_string(&discogs_token_file).ok()
                .expect(&format!("Supply discogs token with commandline argument (refer to --help) or with the file {}", discogs_token_file.display()))
                .trim().to_owned()
        }
    };

    let discogs_client = DiscogsClient::new(&discogs_token);

    let mut console = Console::new();

    match cli.command {
        Command::Import(args) => import(args, &discogs_client, &mut console)?,
        Command::AddMissingCovers(args) => add_missing_covers(args, &discogs_client, &mut console)
    }

    Ok(())
}

fn get_discogs_token_file_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(DISCOGS_TOKEN_FILE_NAME))
}
