use std::ops::Deref;
use std::path::{Path, PathBuf};

use anyhow::Context;
use anyhow::Result;
use sanitize_filename::sanitize_with_options;

use crate::tag;
use crate::tag::frame::FrameId;
use crate::tag::Tag;
use crate::util::path_extensions::PathExtensions;

pub struct MusicFile {
    pub file_path: PathBuf,
    pub tag: Box<dyn Tag>,
}

impl MusicFile {
    pub fn from_path(path: &Path) -> Result<Option<Self>> {
        if let Some(tag) = tag::read_from_path(path, path.extension_or_empty())? {
            Ok(Some(MusicFile {
                file_path: PathBuf::from(path),
                tag,
            }))
        } else {
            Ok(None)
        }
    }

    pub fn from_tag(tag: Box<dyn Tag>, base_path: &Path, transcode_to_mp4: bool, source_extension: &str) -> Result<Self> {
        let target_folder_path = base_path.join(music_folder_path(tag.deref())?);
        let target_extension = if transcode_to_mp4 { "m4a" } else { source_extension };
        let target_path = target_folder_path.join(music_file_name(tag.deref(), target_extension)?);
        Ok(MusicFile {
            file_path: target_path,
            tag,
        })
    }
}

fn music_folder_path(tag: &dyn Tag) -> Result<PathBuf> {
    let context = |frame_id: FrameId| format!("No {} to form music folder name", frame_id);
    let album_artist = tag.album_artist().or_else(|| tag.artist()).with_context(|| context(FrameId::AlbumArtist))?;
    let year = tag.year().with_context(|| context(FrameId::Year))?;
    let album = tag.album().with_context(|| context(FrameId::Album))?;

    let mut path = PathBuf::new();
    path.push(sanitize_path(album_artist));
    path.push(sanitize_path(format!("({}) {}", year, album)));

    Ok(path)
}

fn music_file_name(tag: &dyn Tag, extension: &str) -> Result<String> {
    let context = |frame_id: FrameId| format!("No {} to form music file name", frame_id);
    let track = tag.track_number().with_context(|| context(FrameId::Track))?;
    let title = tag.title().with_context(|| context(FrameId::Title))?;

    Ok(sanitize_path(match tag.disc() {
        Some(disc) => format!(
            "{disc:02}.{track:02}. {title}.{ext}",
            disc = disc,
            track = track,
            title = title,
            ext = extension,
        ),
        None => format!(
            "{track:02}. {title}.{ext}",
            track = track,
            title = title,
            ext = extension,
        ),
    }))
}

fn sanitize_path<S: AsRef<str>>(name: S) -> String {
    sanitize_with_options(name, sanitize_filename::Options { replacement: "-", ..Default::default() })
}
