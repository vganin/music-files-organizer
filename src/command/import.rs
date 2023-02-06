use std::{fs, io};
use std::collections::HashSet;
use std::fmt::Write;
use std::fs::File;
use std::io::Seek;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use dialoguer::{Confirm, Editor};
use dyn_clone::clone_box;
use itertools::Itertools;
use progress_streams::{ProgressReader, ProgressWriter};
use regex::Regex;
use reqwest::Url;
use tempfile::NamedTempFile;
use walkdir::WalkDir;

use DiscogsReleaseMatchResult::{Matched, Unmatched};

use crate::{Console, console_print, DiscogsMatcher, ImportArgs, pb_finish_with_message, pb_set_message, tag, util};
use crate::discogs::create_tag::create_tag_from_discogs_data;
use crate::discogs::matcher::DiscogsReleaseMatchResult;
use crate::discogs::model::{DiscogsRelease, DiscogsTrack};
use crate::music_file::MusicFile;
use crate::tag::frame::{FrameContent, FrameId};
use crate::util::console_styleable::ConsoleStyleable;
use crate::util::path_extensions::PathExtensions;
use crate::util::r#const::COVER_FILE_NAME_WITHOUT_EXTENSION;
use crate::util::transcode;

struct MusicFileChange<'a> {
    source: &'a MusicFile,
    target: MusicFile,
    transcode_to_mp4: bool,
    source_file_len: u64,
    discogs_release: Option<&'a DiscogsRelease>,
}

#[derive(Hash, PartialEq, Eq)]
struct CoverChange {
    path: PathBuf,
    uri: String,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct Cleanup {
    path: PathBuf,
}

struct ChangeList<'a> {
    music_files: Vec<MusicFileChange<'a>>,
    covers: Vec<CoverChange>,
    cleanups: Vec<Cleanup>,
}

pub fn import(args: ImportArgs, discogs_matcher: &DiscogsMatcher, console: &mut Console) -> Result<()> {
    if !fs::metadata(&args.to)?.is_dir() {
        bail!("Output path is not a directory")
    }

    let music_files_chunks = get_music_files_chunks(&args);

    for music_files in music_files_chunks {
        let music_files = music_files?;
        let discogs_releases = discogs_matcher.match_music_files(music_files.iter(), console)?;

        let mut changes = calculate_changes(&discogs_releases, &args)?;

        if changes.music_files.is_empty() && changes.covers.is_empty() {
            continue;
        }

        loop {
            if Confirm::new()
                .with_prompt("Do you want to review changes?")
                .default(false)
                .show_default(true)
                .wait_for_newline(true)
                .interact()?
            {
                print_changes_details(&changes, console);

                if Confirm::new()
                    .with_prompt("Do you want to edit changes?")
                    .default(false)
                    .show_default(true)
                    .wait_for_newline(true)
                    .interact()?
                {
                    changes = edit_changes(changes, &args)?;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if Confirm::new()
            .with_prompt("Do you want to make changes?")
            .default(true)
            .show_default(true)
            .wait_for_newline(true)
            .interact()?
        {
            write_music_files(&changes.music_files, console)?;
            download_covers(discogs_matcher, &changes.covers, console)?;
            cleanup(&changes.cleanups)?;
            if args.fsync {
                fsync(&changes, console)?;
            }
        }
    }

    Ok(())
}

fn get_music_files_chunks(args: &ImportArgs) -> impl Iterator<Item=Result<Vec<MusicFile>>> {
    args.from
        .iter()
        .map(|path| -> Result<_> {
            Ok(
                if fs::metadata(path)?.is_dir() {
                    WalkDir::new(path)
                        .into_iter()
                        .filter_ok(|e| e.file_type().is_dir())
                        .collect_vec()
                } else {
                    WalkDir::new(path)
                        .into_iter()
                        .collect_vec()
                }
            )
        })
        .flatten_ok()
        .flatten_ok()
        .filter_map(Result::ok)
        .chunks(args.chunk_size.unwrap_or(usize::MAX))
        .into_iter()
        .map(|chunk| chunk.collect_vec())
        .collect_vec()
        .into_iter()
        .map(move |chunk| {
            chunk
                .into_iter()
                .flat_map(|e| {
                    WalkDir::new(e.path())
                        .max_depth(1)
                        .into_iter()
                        .filter_map(Result::ok)
                })
                .filter(|e| !e.file_type().is_dir())
                .map(|file| MusicFile::from_path(file.path()))
                .flatten_ok()
                .try_collect::<MusicFile, Vec<MusicFile>, _>()
        })
}

fn calculate_changes<'a>(
    discogs_match_results: &'a Vec<DiscogsReleaseMatchResult>,
    args: &ImportArgs,
) -> Result<ChangeList<'a>> {
    let mut music_file_changes = Vec::new();

    for discogs_match_result in discogs_match_results {
        type Item<'a> = (&'a MusicFile, Option<(u32, &'a DiscogsTrack, &'a DiscogsRelease)>);
        let items: Vec<Item> = match discogs_match_result {
            Matched { tracks_matching, release } => {
                tracks_matching.iter().map(|v| (v.music_file, Some((v.position, &v.track, release)))).collect_vec()
            }
            Unmatched(music_files) => {
                music_files.iter().map(|v| (v.deref(), None)).collect_vec()
            }
        };

        for (music_file, discogs_info) in items {
            let source_tag = &music_file.tag;
            let target_tag = if let Some((position, discogs_track, discogs_release)) = discogs_info {
                create_tag_from_discogs_data(source_tag, position, discogs_track, discogs_release)?
            } else {
                clone_box(music_file.tag.deref())
            };
            let source_path = &music_file.file_path;
            let source_extension = source_path.extension_or_empty();
            let transcode_to_mp4 = source_extension == "flac";
            let bytes_to_transfer = fs::metadata(source_path)?.len();

            let music_file_change = MusicFileChange {
                source: music_file,
                target: MusicFile::from_tag(target_tag, &args.to, transcode_to_mp4, source_extension, music_file.duration)?,
                transcode_to_mp4,
                source_file_len: bytes_to_transfer,
                discogs_release: discogs_info.map(|v| v.2),
            };

            music_file_changes.push(music_file_change);
        }
    }

    music_file_changes.sort_by(|lhs, rhs| {
        let lhs = &lhs.target.tag;
        let rhs = &rhs.target.tag;
        let lhs_album = lhs.album().unwrap_or("");
        let rhs_album = rhs.album().unwrap_or("");
        let lhs_year = lhs.year().unwrap_or(i32::MIN);
        let rhs_year = rhs.year().unwrap_or(i32::MIN);
        if lhs_album == rhs_album && lhs_year == rhs_year {
            lhs.track_number().cmp(&rhs.track_number())
        } else if lhs_year == rhs_year {
            lhs_album.cmp(rhs_album)
        } else {
            lhs_year.cmp(&rhs_year)
        }
    });

    full_change_list(
        music_file_changes,
        args,
    )
}

fn full_change_list<'a>(
    music_file_changes: Vec<MusicFileChange<'a>>,
    args: &ImportArgs,
) -> Result<ChangeList<'a>> {
    let cover_changes = find_cover_changes(
        &music_file_changes
    )?;

    let cleanups = find_cleanups(
        &music_file_changes,
        &cover_changes,
        args,
    )?;

    Ok(ChangeList {
        music_files: music_file_changes,
        covers: cover_changes,
        cleanups,
    })
}

fn print_changes_details(changes: &ChangeList, console: &Console) {
    let mut step_number = 1u32;

    for change in &changes.music_files {
        let source = &change.source;
        let target = &change.target;

        let source_file_path = &source.file_path;
        let target_file_path = &target.file_path;
        if source_file_path == target_file_path {
            console_print!(
                console,
                "{:02}. {} {}",
                step_number,
                if change.transcode_to_mp4 { "Transcode" } else { "Update" }.styled().yellow(),
                source_file_path.file_name_or_empty().path_styled(),
            );
        } else {
            let common_file_prefix = common_path::common_path(source_file_path, target_file_path)
                .unwrap_or_default();
            console_print!(
                console,
                "{:02}. {} {} → {}",
                step_number,
                if change.transcode_to_mp4 { "Transcode" } else { "Copy" }.styled().green(),
                source_file_path.strip_prefix_or_same(&common_file_prefix).display().path_styled(),
                target_file_path.strip_prefix_or_same(&common_file_prefix).display().path_styled(),
            );
        }

        let source_tag = &source.tag;
        let target_tag = &target.tag;
        for frame_id in target_tag.frame_ids() {
            let source_frame_value = source_tag.frame_content(&frame_id).map(|v| v.to_string());
            let target_frame_value = target_tag.frame_content(&frame_id).map(|v| v.to_string());
            if target_frame_value != source_frame_value {
                console_print!(
                    console,
                    "    {}: {} → {}",
                    frame_id,
                    source_frame_value.unwrap_or_else(|| String::from("None")).styled().red(),
                    target_frame_value.unwrap_or_else(|| String::from("None")).styled().green(),
                );
            }
        }

        step_number += 1
    }

    for change in &changes.covers {
        console_print!(
            console,
            "{:02}. {} cover to {}",
            step_number,
            "Download".styled().green(),
            change.path.display().path_styled(),
        );
        step_number += 1;
    }

    for cleanup in &changes.cleanups {
        console_print!(
            console,
            "{:02}. {} {}",
            step_number,
            "Remove".styled().red().bold(),
            cleanup.path.display().path_styled(),
        );
        step_number += 1;
    }
}

fn edit_changes<'a>(
    changes: ChangeList<'a>,
    args: &ImportArgs,
) -> Result<ChangeList<'a>> {
    const TRACK_DELIMITER: &str = "--------------------------";
    let line_pattern: Regex = Regex::new(r"^(.+?): (.*)$")?;
    let mut editor_prompt = String::new();

    for music_file in &changes.music_files {
        let tag = &music_file.target.tag;
        for frame_id in &tag.frame_ids() {
            let frame_content = tag.frame_content(frame_id);
            writeln!(&mut editor_prompt, "{}: {}", frame_id, frame_content.map(|v| v.to_string()).unwrap_or_default())?;
        }

        editor_prompt.push_str(TRACK_DELIMITER);
        editor_prompt.push('\n');
    }

    if let Some(edited) = Editor::new().edit(&editor_prompt)? {
        let mut edited_lines = edited.lines();
        let mut new_music_file_changes: Vec<MusicFileChange> = Vec::new();

        for music_file in changes.music_files {
            let old_tag = &music_file.target.tag;
            let mut new_tag = clone_box(old_tag.deref());
            new_tag.clear();

            loop {
                let line = edited_lines.next().context("Failed to find meta for track")?;

                if line == TRACK_DELIMITER {
                    break;
                }

                let invalid_line_context = || format!("Invalid line: {}", line);
                let captures = line_pattern.captures(line).with_context(invalid_line_context)?;
                let frame_id_as_string = captures.get(1).with_context(invalid_line_context)?.as_str();
                let frame_content_as_string = captures.get(2).with_context(invalid_line_context)?.as_str();
                let frame_id = FrameId::from_str(frame_id_as_string)?;

                let frame_content = match frame_id {
                    FrameId::Title |
                    FrameId::Album |
                    FrameId::AlbumArtist |
                    FrameId::Artist |
                    FrameId::Genre |
                    FrameId::CustomText { .. } => FrameContent::Str(frame_content_as_string.to_owned()),
                    FrameId::Year => FrameContent::I32(frame_content_as_string.parse::<i32>()?),
                    FrameId::Track |
                    FrameId::TotalTracks |
                    FrameId::Disc => FrameContent::U32(frame_content_as_string.parse::<u32>()?),
                };
                new_tag.set_frame(&frame_id, Some(frame_content))?;
            }

            new_music_file_changes.push(MusicFileChange {
                target: MusicFile::from_tag(
                    new_tag,
                    &args.to,
                    music_file.transcode_to_mp4,
                    music_file.source.file_path.extension_or_empty(),
                    music_file.source.duration,
                )?,
                ..music_file
            })
        }


        Ok(full_change_list(
            new_music_file_changes,
            args,
        )?)
    } else {
        Ok(changes)
    }
}

fn write_music_files(changes: &Vec<MusicFileChange>, console: &mut Console) -> Result<()> {
    if changes.is_empty() { return Ok(()); };

    let total_bytes_to_transfer: u64 = changes.iter()
        .map(|v| v.source_file_len)
        .sum();

    let pb = console.new_default_progress_bar(total_bytes_to_transfer);

    for change in changes {
        let source = &change.source;
        let target = &change.target;
        let source_path = &source.file_path;
        let target_path = &target.file_path;
        let target_tag = &target.tag;

        pb_set_message!(pb, "Writing {}", source_path.file_name_or_empty().path_styled());

        fs::create_dir_all(target_path.parent_or_empty())?;

        let mut temp_file = if change.transcode_to_mp4 {
            let mut named_temp_file = NamedTempFile::new()?;
            let named_temp_file_path = named_temp_file.path();
            transcode::to_mp4(
                source_path,
                named_temp_file_path,
                |bytes| pb.inc(bytes as u64 / 2),
            );
            let mut tag = tag::read_from_path(named_temp_file_path, "m4a")?
                .with_context(|| format!("Failed to read from temp file {}", named_temp_file_path.display().path_styled()))?;
            tag.set_from(target_tag.as_ref())?;
            tag.write_to(named_temp_file.as_file_mut())?;
            named_temp_file.into_file()
        } else {
            let mut source_file = ProgressReader::new(
                File::open(source_path)?,
                |bytes| pb.inc(bytes as u64 / 2),
            );
            let mut temp_file = tempfile::tempfile()?;
            io::copy(&mut source_file, &mut temp_file)?;
            target_tag.write_to(&mut temp_file)?;
            temp_file
        };

        temp_file.seek(io::SeekFrom::Start(0))?;

        let source_file_len = change.source_file_len;
        let temp_file_len = temp_file.metadata()?.len();
        let mut target_file = ProgressWriter::new(
            File::create(target_path)?,
            |bytes| pb.inc(bytes as u64 * source_file_len / temp_file_len / 2),
        );

        io::copy(&mut temp_file, &mut target_file)?;
    }

    pb_finish_with_message!(pb, "{}", format!("Written {} file(s)", &changes.len()).styled().green());

    Ok(())
}

fn download_covers(
    discogs_matcher: &DiscogsMatcher,
    changes: &Vec<CoverChange>,
    console: &mut Console,
) -> Result<()> {
    if changes.is_empty() { return Ok(()); };

    let count = changes.len();
    let pb = console.new_default_progress_bar(!0);

    for (index, change) in changes.iter().enumerate() {
        pb_set_message!(pb, "Downloading cover {}/{}", index + 1, count);
        discogs_matcher.download_cover(&change.uri, &change.path, &pb, console)?;
    }

    pb_finish_with_message!(pb, "{}", format!("Downloaded {} cover(s)", count).styled().green());

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
            if Path::exists(parent) &&
                parent.read_dir()?.next().is_none() &&
                Confirm::new()
                    .with_prompt(format!("Directory {} is now empty. Do you wish to remove it?", parent.display().path_styled()))
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

fn find_cover_changes(
    music_files_changes: &Vec<MusicFileChange>
) -> Result<Vec<CoverChange>> {
    let mut cover_changes = HashSet::new();

    for music_file_change in music_files_changes {
        let discogs_release = music_file_change.discogs_release;
        if let Some(best_image) = discogs_release.as_ref().and_then(|v| v.best_image()) {
            let uri = &best_image.resource_url;
            let uri_as_file_path = PathBuf::from(Url::parse(uri)?.path());
            let extension = uri_as_file_path.extension_or_empty();
            let file_name = PathBuf::from(COVER_FILE_NAME_WITHOUT_EXTENSION).with_extension(extension);
            cover_changes.insert(CoverChange {
                path: music_file_change.target.file_path.parent_or_empty().join(file_name),
                uri: uri.to_owned(),
            });
        }
    }

    Ok(cover_changes.into_iter().collect_vec())
}

fn find_cleanups(
    music_files: &Vec<MusicFileChange>,
    covers: &Vec<CoverChange>,
    args: &ImportArgs,
) -> Result<Vec<Cleanup>> {
    let mut result = Vec::new();

    let mut source_folder_paths = HashSet::new();
    let mut target_folder_paths = HashSet::new();
    let mut target_paths = HashSet::new();

    for change in music_files {
        source_folder_paths.insert(change.source.file_path.parent_or_empty());
        target_folder_paths.insert(change.target.file_path.parent_or_empty());
        target_paths.insert(&change.target.file_path);
    }

    for change in covers {
        target_folder_paths.insert(change.path.parent_or_empty());
        target_paths.insert(&change.path);
    }

    if args.clean_target_folders {
        for target_folder_path in target_folder_paths {
            target_folder_path.read_dir()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .for_each(|entry| {
                    let path = entry.path();
                    if !target_paths.contains(&path) {
                        result.push(Cleanup { path });
                    }
                });
        }
    }

    if args.clean_source_folders {
        for source_folder_path in source_folder_paths {
            source_folder_path.read_dir()
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .for_each(|entry| {
                    let path = entry.path();
                    if !target_paths.contains(&path) {
                        result.push(Cleanup { path });
                    }
                });
        }
    }

    Ok(result.into_iter().unique().collect_vec())
}

fn fsync(changes: &ChangeList, console: &mut Console) -> Result<()> {
    let folders = changes.music_files.iter().map(|v| &v.target.file_path)
        .chain(changes.covers.iter().map(|v| &v.path))
        .map(|v| v.parent_or_empty())
        .unique()
        .collect_vec();

    for folder in folders {
        util::fsync::fsync(folder, console)?;
    }

    Ok(())
}
