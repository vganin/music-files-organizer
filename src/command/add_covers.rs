use anyhow::Result;

use AllowedChangeType::Covers;

use crate::cli::AddCoversArguments;
use crate::core::{AllowedChangeType, Args, work};

pub fn add_covers(args: AddCoversArguments, discogs_token: Option<String>) -> Result<()> {
    work(Args {
        input_paths: vec![args.to],
        output_path: None,
        allowed_change_types: vec![Covers],
        allow_questions: false,
        chunk_size: Some(1),
        discogs_token,
        discogs_release_id: None,
        force_fsync: false,
    })
}
