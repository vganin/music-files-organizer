use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use crate::util::path_extensions::PathExtensions;

pub fn from_path(path: impl AsRef<Path>) -> Result<Option<Duration>> {
    let extension = path.as_ref().extension_or_empty().to_lowercase();
    match extension.as_ref() {
        "mp3" => Ok(Some(mp3_duration::from_path(path)?)),
        // TODO: Support m4a and flac
        _ => Ok(None)
    }
}
