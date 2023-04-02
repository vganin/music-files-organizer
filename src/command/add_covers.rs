use anyhow::Result;

use AllowedChangeType::Covers;

use crate::cli::AddCoversArguments;
use crate::core::{AllowedChangeType, work};

pub fn add_covers(args: AddCoversArguments, discogs_token: Option<String>) -> Result<()> {
    work(
        vec![args.to],
        None,
        vec![Covers],
        false,
        Some(1),
        discogs_token,
        None,
        false,
    )
}
