use std::collections::HashSet;
use std::fmt::Write;
use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result};
use dialoguer::Editor;
use itertools::Itertools;
use regex::Regex;
use reqwest::Url;

use crate::console_print;
use crate::core::AllowedChangeType;
use crate::discogs::create_tag::{create_tag_from_discogs_data, strip_redundant_fields};
use crate::discogs::matcher::DiscogsReleaseMatchResult;
use crate::discogs::matcher::DiscogsReleaseMatchResult::{Matched, Unmatched};
use crate::discogs::model::refined::DiscogsRelease;
use crate::music_file::{music_file_name_for, MusicFile, relative_path_for};
use crate::tag::frame::{FrameContent, FrameId};
use crate::util::console_styleable::ConsoleStyleable;
use crate::util::path_extensions::PathExtensions;

pub struct ChangeList<'a> {
    pub music_files: Vec<MusicFileChange<'a>>,
    pub covers: Vec<CoverChange>,
    pub cleanups: Vec<Cleanup>,
}

pub struct MusicFileChange<'a> {
    pub source: &'a MusicFile,
    pub target: MusicFile,
    pub is_transcode: bool,
    pub source_file_length: u64,
    discogs_release: Option<&'a DiscogsRelease>,
}

#[derive(Hash, PartialEq, Eq)]
pub struct CoverChange {
    pub path: PathBuf,
    pub uri: String,
}

#[derive(Clone, Hash, PartialEq, Eq)]
pub struct Cleanup {
    pub path: PathBuf,
}

pub fn calculate_changes<'a>(
    discogs_match_results: &'a [DiscogsReleaseMatchResult],
    output_path: &Option<PathBuf>,
    allowed_change_types: &[AllowedChangeType],
) -> Result<ChangeList<'a>> {
    let file_changes = get_file_changes(discogs_match_results, output_path)?;
    let cover_changes = get_cover_changes(&file_changes)?;
    let cleanup_changes = get_cleanup_changes(
        &file_changes,
        &cover_changes,
        allowed_change_types.contains(&AllowedChangeType::SourceCleanup),
        allowed_change_types.contains(&AllowedChangeType::TargetCleanup),
    )?;
    Ok(ChangeList {
        music_files: if allowed_change_types.contains(&AllowedChangeType::MusicFiles) {
            file_changes
        } else {
            vec![]
        },
        covers: if allowed_change_types.contains(&AllowedChangeType::Covers) {
            cover_changes
        } else {
            vec![]
        },
        cleanups: cleanup_changes,
    })
}

pub fn edit_changes<'a>(
    changes: ChangeList<'a>,
    output_path: &Option<PathBuf>,
) -> Result<ChangeList<'a>> {
    const TRACK_DELIMITER: &str = "--------------------------";
    let line_pattern: Regex = Regex::new(r"^(.+?): (.*)$")?;
    let mut editor_prompt = String::new();

    for music_file in &changes.music_files {
        let tag = &music_file.target.tag;
        for frame_id in &tag.frame_ids() {
            let frame_content = tag.frame_content(frame_id);
            writeln!(
                &mut editor_prompt,
                "{}: {}",
                frame_id,
                frame_content.map(|v| v.to_string()).unwrap_or_default()
            )?;
        }

        editor_prompt.push_str(TRACK_DELIMITER);
        editor_prompt.push('\n');
    }

    if let Some(edited) = Editor::new().edit(&editor_prompt)? {
        let mut edited_lines = edited.lines();
        let mut new_music_file_changes: Vec<MusicFileChange> = Vec::new();

        for music_file in changes.music_files {
            let old_tag = &music_file.target.tag;
            let mut new_tag = old_tag.clone();
            new_tag.clear();

            loop {
                let line = edited_lines
                    .next()
                    .context("Failed to find meta for track")?;

                if line == TRACK_DELIMITER {
                    break;
                }

                let invalid_line_context = || format!("Invalid line: {}", line);
                let captures = line_pattern
                    .captures(line)
                    .with_context(invalid_line_context)?;
                let frame_id_as_string =
                    captures.get(1).with_context(invalid_line_context)?.as_str();
                let frame_content_as_string =
                    captures.get(2).with_context(invalid_line_context)?.as_str();
                let frame_id = FrameId::from_str(frame_id_as_string)?;

                let frame_content = match frame_id {
                    FrameId::Title
                    | FrameId::Album
                    | FrameId::AlbumArtist
                    | FrameId::Artist
                    | FrameId::Genre
                    | FrameId::CustomText { .. } => {
                        FrameContent::Str(frame_content_as_string.to_owned())
                    }
                    FrameId::Year => FrameContent::I32(frame_content_as_string.parse::<i32>()?),
                    FrameId::Track | FrameId::TotalTracks | FrameId::Disc | FrameId::TotalDiscs => {
                        FrameContent::U32(frame_content_as_string.parse::<u32>()?)
                    }
                };
                new_tag.set_frame(&frame_id, Some(frame_content))?;
            }
            let file_path = if let Some(output_path) = &output_path {
                output_path.join(relative_path_for(
                    new_tag.deref(),
                    music_file.target.file_path.extension_or_empty(),
                )?)
            } else {
                music_file
                    .source
                    .file_path
                    .parent_or_empty()
                    .join(music_file_name_for(
                        new_tag.deref(),
                        music_file.target.file_path.extension_or_empty(),
                    )?)
            };

            new_music_file_changes.push(MusicFileChange {
                target: MusicFile {
                    file_path,
                    tag: new_tag,
                    ..music_file.target
                },
                ..music_file
            })
        }

        Ok(ChangeList {
            music_files: new_music_file_changes,
            ..changes
        })
    } else {
        Ok(changes)
    }
}

pub fn print_changes_details(changes: &ChangeList) {
    let mut step_number = 1u32;

    for change in &changes.music_files {
        let source = &change.source;
        let target = &change.target;

        let source_file_path = &source.file_path;
        let target_file_path = &target.file_path;
        if source_file_path == target_file_path {
            console_print!(
                "{:02}. {} {}",
                step_number,
                if change.is_transcode {
                    "Transcode"
                } else {
                    "Update"
                }
                .styled()
                .yellow(),
                source_file_path.file_name_or_empty().path_styled(),
            );
        } else {
            let common_file_prefix =
                common_path::common_path(source_file_path, target_file_path).unwrap_or_default();
            console_print!(
                "{:02}. {} {} → {}",
                step_number,
                if change.is_transcode {
                    "Transcode"
                } else {
                    "Copy"
                }
                .styled()
                .green(),
                source_file_path
                    .strip_prefix_or_same(&common_file_prefix)
                    .display()
                    .path_styled(),
                target_file_path
                    .strip_prefix_or_same(&common_file_prefix)
                    .display()
                    .path_styled(),
            );
        }

        let source_tag = &source.tag;
        let target_tag = &target.tag;
        for frame_id in target_tag.frame_ids() {
            let source_frame_value = source_tag.frame_content(&frame_id).map(|v| v.to_string());
            let target_frame_value = target_tag.frame_content(&frame_id).map(|v| v.to_string());
            if target_frame_value != source_frame_value {
                console_print!(
                    "    {}: {} → {}",
                    frame_id,
                    source_frame_value
                        .unwrap_or_else(|| String::from("None"))
                        .styled()
                        .red(),
                    target_frame_value
                        .unwrap_or_else(|| String::from("None"))
                        .styled()
                        .green(),
                );
            }
        }

        step_number += 1
    }

    for change in &changes.covers {
        console_print!(
            "{:02}. {} cover to {}",
            step_number,
            "Download".styled().green(),
            change.path.display().path_styled(),
        );
        step_number += 1;
    }

    for cleanup in &changes.cleanups {
        console_print!(
            "{:02}. {} {}",
            step_number,
            "Remove".styled().red().bold(),
            cleanup.path.display().path_styled(),
        );
        step_number += 1;
    }
}

fn get_file_changes<'a>(
    discogs_match_results: &'a [DiscogsReleaseMatchResult],
    output_path: &Option<PathBuf>,
) -> Result<Vec<MusicFileChange<'a>>> {
    let mut result = Vec::new();

    let match_items = discogs_match_results
        .iter()
        .flat_map(|discogs_match_result| match discogs_match_result {
            Matched {
                tracks_matching,
                release,
            } => tracks_matching
                .iter()
                .map(|v| (v.music_file, Some((&v.track, release))))
                .collect_vec(),
            Unmatched(music_files) => music_files.iter().map(|v| (v.deref(), None)).collect_vec(),
        })
        .collect_vec();

    for (music_file, discogs_info) in match_items {
        let source_tag = &music_file.tag;
        let target_tag = if let Some((discogs_track, discogs_release)) = discogs_info {
            create_tag_from_discogs_data(source_tag, discogs_track, discogs_release)?
        } else {
            strip_redundant_fields(source_tag)?
        };
        let source_path = &music_file.file_path;
        let source_extension = source_path.extension_or_empty();
        let target_extension = if source_extension == "flac" {
            "m4a"
        } else {
            source_extension
        };
        let source_file_length = fs::metadata(source_path)?.len();
        let file_path = if let Some(output_path) = output_path {
            output_path.join(relative_path_for(target_tag.deref(), target_extension)?)
        } else {
            source_path
                .parent_or_empty()
                .join(music_file_name_for(target_tag.deref(), target_extension)?)
        };
        let duration = music_file.duration;
        let discogs_release = discogs_info.map(|v| v.1);
        let is_transcode = source_extension != target_extension;
        let music_file_change = MusicFileChange {
            source: music_file,
            target: MusicFile {
                file_path,
                tag: target_tag,
                duration,
            },
            is_transcode,
            source_file_length,
            discogs_release,
        };

        result.push(music_file_change);
    }

    result.sort_by(|lhs, rhs| {
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

    Ok(result)
}

fn get_cover_changes(music_files: &Vec<MusicFileChange>) -> Result<Vec<CoverChange>> {
    let mut cover_changes = HashSet::new();

    for music_file in music_files {
        let Some(discogs_release) = music_file.discogs_release else { continue };
        let Some(best_image) = &discogs_release.image else { continue };
        let uri = best_image.url.to_owned();
        let uri_as_file_path = PathBuf::from(Url::parse(&uri)?.path());
        let extension = uri_as_file_path.extension_or_empty();
        let path = music_file
            .target
            .file_path
            .parent_or_empty()
            .join(PathBuf::from(COVER_FILE_NAME_WITHOUT_EXTENSION).with_extension(extension));

        cover_changes.insert(CoverChange { path, uri });
    }

    Ok(cover_changes.into_iter().collect_vec())
}

fn get_cleanup_changes(
    music_files: &Vec<MusicFileChange>,
    covers: &Vec<CoverChange>,
    clean_source_folders: bool,
    clean_target_folders: bool,
) -> Result<Vec<Cleanup>> {
    if !(clean_source_folders || clean_target_folders) {
        return Ok(vec![]);
    }

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

    if clean_target_folders {
        for target_folder_path in target_folder_paths {
            target_folder_path
                .read_dir()
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

    if clean_source_folders {
        for source_folder_path in source_folder_paths {
            source_folder_path
                .read_dir()
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

const COVER_FILE_NAME_WITHOUT_EXTENSION: &str = "cover";
