use anyhow::Result;

use crate::{FsyncArguments, util};
use crate::util::console::Console;

pub fn fsync(
    args: FsyncArguments,
    console: &mut Console,
) -> Result<()> {
    util::fsync::fsync(&args.path, console)?;
    Ok(())
}
