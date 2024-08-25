use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use lofty::file::AudioFile;
use lofty::probe::Probe;

pub fn from_path(path: impl AsRef<Path>) -> Result<Option<Duration>> {
    return Ok(Some(Probe::open(path)?.read()?.properties().duration()));
}
