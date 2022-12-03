use std::{fs, io};
use std::collections::HashSet;
use std::fs::File;
use std::io::Seek;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dialoguer::Confirm;
use progress_streams::{ProgressReader, ProgressWriter};
use reqwest::Url;
use sanitize_filename::{Options, sanitize_with_options};
use tempfile::NamedTempFile;
use walkdir::WalkDir;

use crate::{Console, console_print, DiscogsClient, ImportArgs, pb_finish_with_message, pb_set_message, Tag, tag};
use crate::util::console::style_path;
use crate::util::discogs::{cover_uri_from_discogs_info, DiscogsRelease, tag_from_discogs_info};
use crate::util::r#const::COVER_FILE_NAME_WITHOUT_EXTENSION;
use crate::util::transcode;

pub struct MusicFile {
    pub file_path: PathBuf,
    pub tag: Box<dyn Tag>,
}

struct MusicFileChange {
    source: MusicFile,
    target: MusicFile,
    transcode_to_mp4: bool,
    source_file_len: u64,
}

#[derive(Hash, PartialEq, Eq)]
struct CoverChange {
    path: PathBuf,
    uri: String,
}

#[derive(Hash, PartialEq, Eq)]
struct Cleanup {
    path: PathBuf,
}

struct ChangeList {
    music_files: Vec<MusicFileChange>,
    covers: Vec<CoverChange>,
    cleanups: Vec<Cleanup>,
}

pub fn import(args: ImportArgs, discogs_client: &DiscogsClient, console: &mut Console) -> Result<()> {
    if !fs::metadata(&args.to)?.is_dir() {
        panic!("Output path is not directory")
    }

    let music_files = get_music_files(&args.from, console)?;
    let discogs_releases = discogs_client.fetch_by_music_files(music_files, console);
    let changes = calculate_changes(
        discogs_releases,
        &args.to,
        !args.dont_clean_target_folders,
        args.clean_source_folders,
    );

    if changes.music_files.is_empty() && changes.covers.is_empty() {
        console_print!(console, "{}", console::style("Nothing to do, all good").green());
        return Ok(());
    }

    if Confirm::new()
        .with_prompt("Do you want to print changes?")
        .default(false)
        .show_default(true)
        .wait_for_newline(true)
        .interact()?
    {
        print_changes_details(&changes, console);
    }

    if Confirm::new()
        .with_prompt("Do you want to make changes?")
        .default(true)
        .show_default(true)
        .wait_for_newline(true)
        .interact()?
    {
        write_music_files(&changes.music_files, console)?;
        download_covers(&discogs_client, &changes.covers, console);
        cleanup(&changes.cleanups);
    }

    Ok(())
}

fn get_music_files(path: impl AsRef<Path>, console: &mut Console) -> Result<Vec<MusicFile>> {
    let pb = console.new_default_spinner();

    let files: Vec<_> = WalkDir::new(path).into_iter()
        .filter_map(Result::ok)
        .filter(|e| !e.file_type().is_dir())
        .collect();

    let mut music_files = vec![];

    for file in files {
        let path = file.path();
        let invalid_path_context = || format!("Invalid path {}", style_path(path.display()));
        let file_name = path.file_name().with_context(invalid_path_context)?
            .to_str().with_context(invalid_path_context)?;
        let extension = path.extension().with_context(invalid_path_context)?
            .to_str().with_context(invalid_path_context)?;

        pb_set_message!(pb, "Analyzing {}", style_path(file_name));

        if let Some(tag) = tag::read_from_path(&path, extension) {
            let tag = tag?;
            music_files.push(
                MusicFile {
                    file_path: PathBuf::from(path),
                    tag,
                })
        }
    }

    pb.finish_and_clear();

    Ok(music_files)
}

fn calculate_changes(
    discogs_releases: Vec<DiscogsRelease>,
    import_path: &Path,
    clean_targets: bool,
    clean_sources: bool,
) -> ChangeList {
    let mut music_file_changes = Vec::new();
    let mut cover_changes = HashSet::new();

    for DiscogsRelease { music_files, discogs_info } in discogs_releases {
        for music_file in music_files {
            let source_tag = &music_file.tag;
            let target_tag = tag_from_discogs_info(source_tag, &discogs_info);
            let source_path = &music_file.file_path;
            let source_extension = source_path.extension().unwrap().to_str().unwrap();
            let transcode_to_mp4 = source_extension == "flac";
            let target_folder_path = import_path.join(music_folder_path(&*target_tag));
            let target_extension = if transcode_to_mp4 { "m4a" } else { source_extension };
            let target_path = target_folder_path.join(music_file_name(&*target_tag, target_extension));
            let bytes_to_transfer = fs::metadata(&source_path).unwrap().len();

            music_file_changes.push(MusicFileChange {
                source: music_file,
                target: MusicFile {
                    file_path: target_path,
                    tag: target_tag,
                },
                transcode_to_mp4,
                source_file_len: bytes_to_transfer,
            });

            if let Some(uri) = cover_uri_from_discogs_info(&discogs_info) {
                let uri_as_file_path = PathBuf::from(Url::parse(&uri).unwrap().path());
                let extension = uri_as_file_path.extension().unwrap();
                let file_name = PathBuf::from(COVER_FILE_NAME_WITHOUT_EXTENSION).with_extension(extension);
                cover_changes.insert(CoverChange {
                    path: target_folder_path.join(file_name),
                    uri: uri.to_owned(),
                });
            }
        }
    }

    music_file_changes.sort_by(|lhs, rhs| {
        let lhs = &lhs.target.tag;
        let rhs = &rhs.target.tag;
        let lhs_album = lhs.album().unwrap();
        let rhs_album = rhs.album().unwrap();
        let lhs_year = lhs.year().unwrap();
        let rhs_year = rhs.year().unwrap();
        if lhs_album == rhs_album && lhs_year == rhs_year {
            lhs.track().cmp(&rhs.track())
        } else if lhs_year == rhs_year {
            lhs_album.cmp(rhs_album)
        } else {
            lhs_year.cmp(&rhs_year)
        }
    });

    let cover_changes = cover_changes.into_iter().collect();

    let cleanups = find_cleanups(
        &music_file_changes,
        &cover_changes,
        clean_targets,
        clean_sources,
    );

    ChangeList {
        music_files: music_file_changes,
        covers: cover_changes,
        cleanups,
    }
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
                console::style(if change.transcode_to_mp4 { "Transcode" } else { "Update" }).yellow(),
                console::style(source_file_path.file_name().unwrap().to_str().unwrap()).bold(),
            );
        } else {
            let common_file_prefix = common_path::common_path(source_file_path, target_file_path)
                .unwrap_or(PathBuf::new());
            console_print!(
                console,
                "{:02}. {} {} → {}",
                step_number,
                console::style(if change.transcode_to_mp4 { "Transcode" } else { "Copy" }).green(),
                console::style(source_file_path.strip_prefix(&common_file_prefix).unwrap().display()).bold(),
                console::style(target_file_path.strip_prefix(&common_file_prefix).unwrap().display()).bold(),
            );
        }

        let source_tag = &source.tag;
        let target_tag = &target.tag;
        for frame_id in target_tag.frame_ids() {
            let source_frame_value = source_tag.frame_content(&frame_id).map(|v| v.stringify_content());
            let target_frame_value = target_tag.frame_content(&frame_id).map(|v| v.stringify_content());
            if target_frame_value != source_frame_value {
                console_print!(
                    console,
                    "    {}: {} → {}",
                    frame_id.description(),
                    console::style(source_frame_value.unwrap_or(String::from("None"))).red(),
                    console::style(target_frame_value.unwrap_or(String::from("None"))).green(),
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
            console::style("Download").green(),
            console::style(change.path.display()).bold(),
        );
        step_number += 1;
    }

    for cleanup in &changes.cleanups {
        console_print!(
            console,
            "{:02}. {} {}",
            step_number,
            console::style("Remove").red().bold(),
            console::style(cleanup.path.display()).bold(),
        );
        step_number += 1;
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

        pb_set_message!(pb, "Writing {}",
            console::style(source_path.file_name().unwrap().to_str().unwrap()).bold());

        fs::create_dir_all(target_path.parent().unwrap()).unwrap();

        let mut temp_file = if change.transcode_to_mp4 {
            let mut named_temp_file = NamedTempFile::new().unwrap();
            transcode::to_mp4(
                &source_path,
                named_temp_file.path(),
                |bytes| pb.inc(bytes as u64 / 2),
            );
            let mut tag = tag::read_from_path(named_temp_file.path(), "m4a")
                .with_context(|| format!("Failed to read from temp file {}", style_path(named_temp_file.path().display())))??;
            tag.set_from(target_tag);
            tag.write_to(named_temp_file.as_file_mut());
            named_temp_file.into_file()
        } else {
            let mut source_file = ProgressReader::new(
                File::open(&source_path).unwrap(),
                |bytes| pb.inc(bytes as u64 / 2),
            );
            let mut temp_file = tempfile::tempfile().unwrap();
            io::copy(&mut source_file, &mut temp_file).unwrap();
            target_tag.write_to(&mut temp_file);
            temp_file
        };

        temp_file.seek(io::SeekFrom::Start(0)).unwrap();

        let source_file_len = change.source_file_len;
        let temp_file_len = temp_file.metadata().unwrap().len();
        let mut target_file = ProgressWriter::new(
            File::create(&target_path).unwrap(),
            |bytes| pb.inc(bytes as u64 * source_file_len / temp_file_len / 2),
        );

        io::copy(&mut temp_file, &mut target_file).unwrap();
    }

    pb_finish_with_message!(pb, "{}", console::style(format!("Written {} file(s)", &changes.len())).green());

    Ok(())
}

fn download_covers(
    discogs_client: &DiscogsClient,
    changes: &Vec<CoverChange>,
    console: &mut Console,
) {
    if changes.is_empty() { return; };

    let count = changes.len();
    let pb = console.new_default_progress_bar(!0);

    for (index, change) in changes.iter().enumerate() {
        pb_set_message!(pb, "Downloading cover {}/{}", index + 1, count);
        discogs_client.download_cover(&change.uri, &change.path, &pb, console);
    }

    pb_finish_with_message!(pb, "{}", console::style(format!("Downloaded {} cover(s)", count)).green());
}

fn cleanup(cleanups: &[Cleanup]) {
    let mut parent_dirs = HashSet::new();

    for cleanup in cleanups {
        let path = &cleanup.path;
        parent_dirs.insert(path.parent().unwrap().to_owned());
        let metadata = fs::metadata(path).unwrap();
        if metadata.is_dir() {
            fs::remove_dir_all(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }

    for parent_dir in parent_dirs {
        if Path::exists(&parent_dir) && parent_dir.read_dir().unwrap().next().is_none() {
            if Confirm::new()
                .with_prompt(format!(
                    "Directory {} is now empty. Do you wish to remove it?",
                    console::style(parent_dir.display()).bold()
                ))
                .default(true)
                .show_default(true)
                .wait_for_newline(true)
                .interact()
                .unwrap()
            {
                fs::remove_dir_all(&parent_dir).unwrap();
            }
        }
    }
}

fn find_cleanups(
    music_files: &Vec<MusicFileChange>,
    covers: &Vec<CoverChange>,
    clean_targets: bool,
    clean_sources: bool,
) -> Vec<Cleanup> {
    let mut result = HashSet::new();

    let mut source_folder_paths = HashSet::new();
    let mut target_folder_paths = HashSet::new();
    let mut target_paths = HashSet::new();

    for change in music_files {
        source_folder_paths.insert(PathBuf::from(change.source.file_path.parent().unwrap()));
        target_folder_paths.insert(PathBuf::from(change.target.file_path.parent().unwrap()));
        target_paths.insert(change.target.file_path.to_owned());
    }

    for change in covers {
        target_folder_paths.insert(PathBuf::from(change.path.parent().unwrap()));
        target_paths.insert(change.path.to_owned());
    }

    if clean_targets {
        for target_folder_path in target_folder_paths {
            target_folder_path.read_dir().into_iter().flat_map(|v| v.into_iter()).for_each(|entry| {
                let entry = entry.unwrap();
                let path = entry.path();
                if !target_paths.contains(&path) {
                    result.insert(Cleanup { path });
                }
            });
        }
    }

    if clean_sources {
        for source_folder_path in source_folder_paths {
            source_folder_path.read_dir().into_iter().flat_map(|v| v.into_iter()).for_each(|entry| {
                let entry = entry.unwrap();
                let path = entry.path();
                if !target_paths.contains(&path) {
                    result.insert(Cleanup { path });
                }
            });
        }
    }

    result.into_iter().collect()
}

fn music_folder_path(tag: &dyn Tag) -> PathBuf {
    let mut path = PathBuf::new();
    path.push(sanitize_path(tag.album_artist().unwrap()));
    path.push(sanitize_path(format!("({}) {}", tag.year().unwrap(), tag.album().unwrap())));
    path
}

fn music_file_name(tag: &dyn Tag, ext: &str) -> String {
    sanitize_path(match tag.disc() {
        Some(disc) => format!(
            "{disc:02}.{track:02}. {title}.{ext}",
            disc = disc,
            track = tag.track().unwrap(),
            title = tag.title().unwrap(),
            ext = ext,
        ),
        None => format!(
            "{track:02}. {title}.{ext}",
            track = tag.track().unwrap(),
            title = tag.title().unwrap(),
            ext = ext,
        ),
    })
}

fn sanitize_path<S: AsRef<str>>(name: S) -> String {
    let mut options = Options::default();
    options.replacement = "-";
    sanitize_with_options(name, options)
}
