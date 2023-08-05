use std::{fs, io};
use std::fs::File;
use std::io::Seek;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use dialoguer::Confirm;
use itertools::Itertools;
use progress_streams::{ProgressReader, ProgressWriter};
use walkdir::WalkDir;

use crate::{pb_finish_with_message, pb_set_message, util};
use crate::core::changes::{
    calculate_changes, ChangeList, Cleanup, CoverChange, edit_changes, MusicFileChange,
    print_changes_details,
};
use crate::discogs::matcher::DiscogsMatcher;
use crate::music_file::MusicFile;
use crate::util::console;
use crate::util::console_styleable::ConsoleStyleable;
use crate::util::path_extensions::PathExtensions;

mod changes;

#[derive(PartialEq)]
pub enum AllowedChangeType {
    MusicFiles,
    Covers,
    SourceCleanup,
    TargetCleanup,
}

pub struct Args {
    pub input_paths: Vec<PathBuf>,
    pub output_path: Option<PathBuf>,
    pub allowed_change_types: Vec<AllowedChangeType>,
    pub allow_questions: bool,
    pub chunk_size: Option<usize>,
    pub discogs_token: Option<String>,
    pub discogs_release_id: Option<String>,
    pub force_fsync: bool,
}

pub fn work(args: Args) -> Result<()> {
    let discogs_matcher = DiscogsMatcher::with_optional_token(&args.discogs_token)?;

    match &args.output_path {
        Some(output_path) => {
            if !fs::metadata(output_path)?.is_dir() {
                bail!("Output path is not a directory")
            }
        }
        None => {}
    }

    let music_files_chunks = get_music_files_chunks(args.input_paths, args.chunk_size);

    for music_files in music_files_chunks {
        let music_files = music_files?;
        let discogs_releases =
            discogs_matcher.match_music_files(music_files.iter(), &args.discogs_release_id)?;

        let mut changes = calculate_changes(
            &discogs_releases,
            &args.output_path,
            &args.allowed_change_types,
        )?;

        if changes.music_files.is_empty() && changes.covers.is_empty() && changes.covers.is_empty()
        {
            continue;
        }

        if args.allow_questions {
            loop {
                if Confirm::new()
                    .with_prompt("Do you want to review changes?")
                    .default(false)
                    .show_default(true)
                    .wait_for_newline(true)
                    .interact()?
                {
                    print_changes_details(&changes);

                    if Confirm::new()
                        .with_prompt("Do you want to edit changes?")
                        .default(false)
                        .show_default(true)
                        .wait_for_newline(true)
                        .interact()?
                    {
                        changes = edit_changes(changes, &args.output_path)?;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }

        if !args.allow_questions
            || Confirm::new()
                .with_prompt("Do you want to make changes?")
                .default(true)
                .show_default(true)
                .wait_for_newline(true)
                .interact()?
        {
            write_music_files(&changes.music_files)?;
            download_covers(&discogs_matcher, &changes.covers)?;
            cleanup(&changes.cleanups)?;
            if args.force_fsync {
                fsync(&changes)?;
            }
        }
    }

    Ok(())
}

fn get_music_files_chunks(
    input_paths: Vec<PathBuf>,
    chunk_size: Option<usize>,
) -> impl Iterator<Item = Result<Vec<MusicFile>>> {
    input_paths
        .iter()
        .map(|path| -> Result<_> {
            Ok(if fs::metadata(path)?.is_dir() {
                WalkDir::new(path)
                    .into_iter()
                    .filter_ok(|e| e.file_type().is_dir())
                    .collect_vec()
            } else {
                WalkDir::new(path).into_iter().collect_vec()
            })
        })
        .flatten_ok()
        .flatten_ok()
        .filter_map(Result::ok)
        .chunks(chunk_size.unwrap_or(usize::MAX))
        .into_iter()
        .map(|chunk| chunk.collect_vec())
        .collect_vec()
        .into_iter()
        .map(|chunk| {
            let pb = console::get_mut().new_default_spinner();
            let result = chunk
                .into_iter()
                .flat_map(|e| {
                    WalkDir::new(e.path())
                        .max_depth(1)
                        .into_iter()
                        .filter_map(Result::ok)
                })
                .filter(|e| !e.file_type().is_dir())
                .map(|file| {
                    pb_set_message!(pb, "Analyzing {}", file.path().display().path_styled());
                    MusicFile::from_path(file.path())
                })
                .flatten_ok()
                .try_collect::<MusicFile, Vec<MusicFile>, _>();
            pb.finish_and_clear();
            result
        })
}

fn write_music_files(changes: &Vec<MusicFileChange>) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    };

    let total_bytes_to_transfer: u64 = changes.iter().map(|v| v.source_file_length).sum();

    let pb = console::get_mut().new_default_progress_bar(total_bytes_to_transfer);

    for change in changes {
        let source = &change.source;
        let target = &change.target;
        let source_path = &source.file_path;
        let target_path = &target.file_path;
        let target_tag = &target.tag;

        pb_set_message!(
            pb,
            "Writing {}",
            source_path.file_name_or_empty().path_styled()
        );

        fs::create_dir_all(target_path.parent_or_empty())?;

        let mut temp_file = {
            let mut source_file =
                ProgressReader::new(File::open(source_path)?, |bytes| pb.inc(bytes as u64 / 2));
            let mut temp_file = tempfile::tempfile()?;
            io::copy(&mut source_file, &mut temp_file)?;
            target_tag.write_to(&mut temp_file)?;
            temp_file
        };

        temp_file.rewind()?;

        let source_file_len = change.source_file_length;
        let temp_file_len = temp_file.metadata()?.len();
        let mut target_file = ProgressWriter::new(File::create(target_path)?, |bytes| {
            pb.inc(bytes as u64 * source_file_len / temp_file_len / 2)
        });

        io::copy(&mut temp_file, &mut target_file)?;
    }

    pb_finish_with_message!(
        pb,
        "{}",
        format!("Written {} file(s)", &changes.len())
            .styled()
            .green()
    );

    Ok(())
}

fn download_covers(discogs_matcher: &DiscogsMatcher, changes: &Vec<CoverChange>) -> Result<()> {
    if changes.is_empty() {
        return Ok(());
    };

    let count = changes.len();
    let pb = console::get_mut().new_default_progress_bar(!0);

    for (index, change) in changes.iter().enumerate() {
        pb_set_message!(pb, "Downloading cover {}/{}", index + 1, count);
        discogs_matcher.download_cover(&change.uri, &change.path, &pb)?;
    }

    pb_finish_with_message!(
        pb,
        "{}",
        format!("Downloaded {} cover(s)", count).styled().green()
    );

    Ok(())
}

fn cleanup(cleanups: &[Cleanup]) -> Result<()> {
    for cleanup in cleanups {
        let path = &cleanup.path;
        let metadata = fs::metadata(path)?;
        if metadata.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }

    // Clean all empty parent dirs
    for cleanup in cleanups {
        let mut path: &Path = &cleanup.path;
        while let Some(parent) = path.parent() {
            if Path::exists(parent)
                && parent.read_dir()?.next().is_none()
                && Confirm::new()
                    .with_prompt(format!(
                        "Directory {} is now empty. Do you wish to remove it?",
                        parent.display().path_styled()
                    ))
                    .default(true)
                    .show_default(true)
                    .wait_for_newline(true)
                    .interact()?
            {
                fs::remove_dir_all(parent)?;
                path = parent;
            } else {
                break;
            }
        }
    }

    Ok(())
}

fn fsync(changes: &ChangeList) -> Result<()> {
    let folders = changes
        .music_files
        .iter()
        .map(|v| &v.target.file_path)
        .chain(changes.covers.iter().map(|v| &v.path))
        .map(|v| v.parent_or_empty())
        .unique()
        .collect_vec();

    for folder in folders {
        util::fsync::fsync(folder)?;
    }

    Ok(())
}
