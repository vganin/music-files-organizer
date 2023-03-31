use std::fs::File;
use std::path::Path;

use anyhow::Result;

use crate::pb_set_message;
use crate::util::console::Console;
use crate::util::console_styleable::ConsoleStyleable;

pub(crate) fn fsync(path: &Path, console: &mut Console) -> Result<()> {
    let pb = console.new_default_spinner();
    pb_set_message!(pb, "Syncing {}", path.display().path_styled());
    File::open(path)?.sync_all()?;
    Ok(())
}
