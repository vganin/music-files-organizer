use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use lofty::{AudioFile, Probe};

use crate::util::path_extensions::PathExtensions;

pub fn from_path(path: impl AsRef<Path>) -> Result<Option<Duration>> {
    let extension = path.as_ref().extension_or_empty().to_lowercase();
    match extension.as_ref() {
        "mp3" => Ok(Some(mp3_duration::from_path(path)?)),
        "flac" => Ok(Some(Probe::open(path)?.read()?.properties().duration())),
        _ => Ok(None)
    }
}
