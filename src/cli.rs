use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    #[clap(long)]
    pub discogs_token: Option<String>,

    #[clap(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    GenerateCompletions(GenerateCompletionsArgs),
    Import(ImportArgs),
    AddCovers(AddCoversArguments),
    Fsync(FsyncArguments),
}

#[derive(Args)]
pub struct GenerateCompletionsArgs {
    #[clap()]
    pub shell: Option<Shell>,
}

#[derive(Args)]
pub struct ImportArgs {
    #[clap(long, num_args = 1..)]
    pub from: Vec<PathBuf>,

    #[clap(long)]
    pub to: Option<PathBuf>,

    #[clap(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub clean_target_folders: bool,

    #[clap(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub clean_source_folders: bool,

    #[clap(long, default_value_t = true, action = clap::ArgAction::Set)]
    pub fsync: bool,

    #[clap(long)]
    pub chunk_size: Option<usize>,

    #[clap(long)]
    pub discogs_release_id: Option<String>,
}

#[derive(Args)]
pub struct AddCoversArguments {
    #[clap()]
    pub to: PathBuf,
}

#[derive(Args)]
pub struct FsyncArguments {
    #[clap()]
    pub path: PathBuf,
}
