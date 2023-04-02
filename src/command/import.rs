use anyhow::Result;

use AllowedChangeType::{Covers, MusicFiles, SourceCleanup, TargetCleanup};

use crate::cli::ImportArgs;
use crate::core::{AllowedChangeType, Args, work};

pub fn import(args: ImportArgs, discogs_token: Option<String>) -> Result<()> {
    work(Args {
        input_paths: args.from,
        output_path: args.to,
        allowed_change_types: vec![MusicFiles, Covers, SourceCleanup, TargetCleanup],
        allow_questions: true,
        chunk_size: args.chunk_size,
        discogs_token,
        discogs_release_id: args.discogs_release_id,
        force_fsync: args.fsync,
    })
}
