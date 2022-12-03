use std::fs::File;
use std::io;
use std::io::{Seek, Write};
use std::path::Path;

use anyhow::{Context, Result};
use dyn_clone::DynClone;

use frame::*;

use crate::util::console::style_path;

pub mod frame;
mod id3;
mod m4a;
mod flac;

pub trait Tag: DynClone {
    fn frame_ids(&self) -> Vec<FrameId>;

    fn title(&self) -> Option<&str>;
    fn set_title(&mut self, title: Option<String>);

    fn album(&self) -> Option<&str>;
    fn set_album(&mut self, album: Option<String>);

    fn album_artist(&self) -> Option<&str>;
    fn set_album_artist(&mut self, album_artist: Option<String>);

    fn artist(&self) -> Option<&str>;
    fn set_artist(&mut self, artist: Option<String>);

    fn year(&self) -> Option<i32>;
    fn set_year(&mut self, year: Option<i32>);

    fn track(&self) -> Option<u32>;
    fn set_track(&mut self, track: Option<u32>);

    fn total_tracks(&self) -> Option<u32>;
    fn set_total_tracks(&mut self, total_tracks: Option<u32>);

    fn disc(&self) -> Option<u32>;
    fn set_disc(&mut self, disc: Option<u32>);

    fn genre(&self) -> Option<&str>;
    fn set_genre(&mut self, genre: Option<String>);

    fn custom_text(&self, key: &str) -> Option<&str>;
    fn set_custom_text(&mut self, key: String, value: Option<String>);

    fn clear(&mut self);

    fn write_to(&self, file: &mut File);
}

impl dyn Tag + '_ {
    pub fn frame_content(&self, id: &FrameId) -> Option<FrameContent> {
        match id {
            FrameId::Title => self.title().map(|v| FrameContent::Str(v.to_owned())),
            FrameId::Album => self.album().map(|v| FrameContent::Str(v.to_owned())),
            FrameId::AlbumArtist => self.album_artist().map(|v| FrameContent::Str(v.to_owned())),
            FrameId::Artist => self.artist().map(|v| FrameContent::Str(v.to_owned())),
            FrameId::Year => self.year().map(FrameContent::I32),
            FrameId::Track => self.track().map(FrameContent::U32),
            FrameId::TotalTracks => self.total_tracks().map(FrameContent::U32),
            FrameId::Disc => self.disc().map(FrameContent::U32),
            FrameId::Genre => self.genre().map(|v| FrameContent::Str(v.to_owned())),
            FrameId::CustomText { key } => self.custom_text(key).map(|v| FrameContent::Str(v.to_owned())),
        }
    }

    pub fn set_frame(&mut self, id: &FrameId, content: Option<FrameContent>) -> Result<()> {
        if let Some(content) = content {
            self.set_some_frame(id, content)?
        } else {
            self.remove_frame(id)
        };

        Ok(())
    }

    fn set_some_frame(&mut self, id: &FrameId, content: FrameContent) -> Result<()> {
        match id {
            FrameId::Title => self.set_title(Some(content.as_str()?.to_owned())),
            FrameId::Album => self.set_album(Some(content.as_str()?.to_owned())),
            FrameId::AlbumArtist => self.set_album_artist(Some(content.as_str()?.to_owned())),
            FrameId::Artist => self.set_artist(Some(content.as_str()?.to_owned())),
            FrameId::Year => self.set_year(Some(content.as_i32()?)),
            FrameId::Track => self.set_track(Some(content.as_u32()?)),
            FrameId::TotalTracks => self.set_total_tracks(Some(content.as_u32()?)),
            FrameId::Disc => self.set_disc(Some(content.as_u32()?)),
            FrameId::Genre => self.set_genre(Some(content.as_str()?.to_owned())),
            FrameId::CustomText { key } => self.set_custom_text(
                key.to_owned(), Some(content.as_str()?.to_owned())),
        };

        Ok(())
    }

    fn remove_frame(&mut self, id: &FrameId) {
        match id {
            FrameId::Title => self.set_title(None),
            FrameId::Album => self.set_album(None),
            FrameId::AlbumArtist => self.set_album_artist(None),
            FrameId::Artist => self.set_artist(None),
            FrameId::Year => self.set_year(None),
            FrameId::Track => self.set_track(None),
            FrameId::TotalTracks => self.set_total_tracks(None),
            FrameId::Disc => self.set_disc(None),
            FrameId::Genre => self.set_genre(None),
            FrameId::CustomText { key } => self.set_custom_text(key.to_owned(), None),
        };
    }

    pub fn set_from(&mut self, other: &dyn Tag) -> Result<()> {
        self.clear();

        for frame_id in other.frame_ids() {
            let frame_content = other.frame_content(&frame_id);
            self.set_frame(&frame_id, frame_content)?;
        }

        Ok(())
    }
}

pub fn read_from_path(path: impl AsRef<Path>, format: &str) -> Option<Result<Box<dyn Tag>>> {
    let context = || format!("Invalid tags in file {}", style_path(path.as_ref().display()));
    match format {
        "mp3" => Some(::id3::Tag::read_from_path(&path).map(|v| Box::new(v) as Box<dyn Tag>).with_context(context)),
        "m4a" => Some(mp4ameta::Tag::read_from_path(&path).map(|v| Box::new(v) as Box<dyn Tag>).with_context(context)),
        "flac" => Some(metaflac::Tag::read_from_path(&path).map(|v| Box::new(v) as Box<dyn Tag>).with_context(context)),
        _ => None
    }
}
