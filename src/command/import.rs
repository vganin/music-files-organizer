use anyhow::Result;

use AllowedChangeType::{Covers, MusicFiles, SourceCleanup, TargetCleanup};

use crate::cli::ImportArgs;
use crate::core::{AllowedChangeType, work};

pub fn import(args: ImportArgs, discogs_token: Option<String>) -> Result<()> {
    work(
        args.from,
        args.to,
        vec![MusicFiles, Covers, SourceCleanup, TargetCleanup],
        true,
        args.chunk_size,
        discogs_token,
        args.discogs_release_id,
        args.fsync,
    )
}
