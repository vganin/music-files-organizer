use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use sanitize_filename::sanitize_with_options;

use crate::tag;
use crate::tag::frame::FrameId;
use crate::tag::Tag;
use crate::util::audio_file_duration::from_path;
use crate::util::path_extensions::PathExtensions;

pub struct MusicFile {
    pub file_path: PathBuf,
    pub tag: Box<dyn Tag>,
    pub duration: Option<Duration>,
}

impl MusicFile {
    pub fn from_path(path: &Path) -> Result<Option<Self>> {
        if let Some(tag) = tag::read_from_path(path, path.extension_or_empty())? {
            Ok(Some(MusicFile {
                file_path: PathBuf::from(path),
                tag,
                duration: from_path(path)?,
            }))
        } else {
            Ok(None)
        }
    }
}

pub fn relative_path_for(tag: &dyn Tag, with_extension: &str) -> Result<PathBuf> {
    Ok(music_folder_path_for(tag.deref())?.join(music_file_name_for(tag.deref(), with_extension)?))
}

pub fn music_folder_path_for(tag: &dyn Tag) -> Result<PathBuf> {
    let context = |frame_id: FrameId| format!("No {} to form music folder name", frame_id);
    let album_artist = tag
        .album_artist()
        .or_else(|| tag.artist())
        .with_context(|| context(FrameId::AlbumArtist))?;
    let year = tag.year().with_context(|| context(FrameId::Year))?;
    let album = tag.album().with_context(|| context(FrameId::Album))?;

    let mut path = PathBuf::new();
    path.push(sanitize_path(album_artist));
    path.push(sanitize_path(format!("({}) {}", year, album)));

    Ok(path)
}

pub fn music_file_name_for(tag: &dyn Tag, with_extension: &str) -> Result<String> {
    let context = |frame_id: FrameId| format!("No {} to form music file name", frame_id);
    let track = tag
        .track_number()
        .with_context(|| context(FrameId::Track))?;
    let title = tag.title().with_context(|| context(FrameId::Title))?;

    Ok(sanitize_path(match tag.disc() {
        Some(disc) => format!(
            "{disc:02}.{track:02}. {title}.{extension}",
            disc = disc,
            track = track,
            title = title,
            extension = with_extension,
        ),
        None => format!(
            "{track:02}. {title}.{extension}",
            track = track,
            title = title,
            extension = with_extension,
        ),
    }))
}

fn sanitize_path<S: AsRef<str>>(name: S) -> String {
    sanitize_with_options(
        name,
        sanitize_filename::Options {
            replacement: "-",
            ..Default::default()
        },
    )
}
