use anyhow::Result;

use crate::cli::FsyncArguments;
use crate::util;

pub fn fsync(args: FsyncArguments) -> Result<()> {
    util::fsync::fsync(&args.path)?;
    Ok(())
}
