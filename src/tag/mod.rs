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
    fn set_title(&mut self, title: String);

    fn album(&self) -> Option<&str>;
    fn set_album(&mut self, album: String);

    fn album_artist(&self) -> Option<&str>;
    fn set_album_artist(&mut self, album_artist: String);

    fn artist(&self) -> Option<&str>;
    fn set_artist(&mut self, artist: String);

    fn year(&self) -> Option<i32>;
    fn set_year(&mut self, year: i32);

    fn track(&self) -> Option<u32>;
    fn set_track(&mut self, track: u32);

    fn total_tracks(&self) -> Option<u32>;
    fn set_total_tracks(&mut self, total_tracks: u32);

    fn disc(&self) -> Option<u32>;
    fn set_disc(&mut self, disc: u32);

    fn genre(&self) -> Option<&str>;
    fn set_genre(&mut self, genre: String);

    fn custom_text(&self, key: &str) -> Option<&str>;
    fn set_custom_text(&mut self, key: String, value: String);

    fn clear(&mut self);

    fn write_to(&self, file: &mut File);
}

impl dyn Tag {
    pub fn frame_content(&self, id: &FrameId) -> Option<FrameContent> {
        match id {
            FrameId::Title => self.title().map(|v| FrameContent::Str(v.to_owned())),
            FrameId::Album => self.album().map(|v| FrameContent::Str(v.to_owned())),
            FrameId::AlbumArtist => self.album_artist().map(|v| FrameContent::Str(v.to_owned())),
            FrameId::Artist => self.artist().map(|v| FrameContent::Str(v.to_owned())),
            FrameId::Year => self.year().map(|v| FrameContent::I32(v)),
            FrameId::Track => self.track().map(|v| FrameContent::U32(v)),
            FrameId::TotalTracks => self.total_tracks().map(|v| FrameContent::U32(v)),
            FrameId::Disc => self.disc().map(|v| FrameContent::U32(v)),
            FrameId::Genre => self.genre().map(|v| FrameContent::Str(v.to_owned())),
            FrameId::CustomText { key } => self.custom_text(&key).map(|v| FrameContent::Str(v.to_owned())),
        }
    }

    pub fn set_frame(&mut self, id: &FrameId, content: FrameContent) {
        match id {
            FrameId::Title => self.set_title(content.as_str().unwrap().to_owned()),
            FrameId::Album => self.set_album(content.as_str().unwrap().to_owned()),
            FrameId::AlbumArtist => self.set_album_artist(content.as_str().unwrap().to_owned()),
            FrameId::Artist => self.set_artist(content.as_str().unwrap().to_owned()),
            FrameId::Year => self.set_year(content.as_i32().unwrap()),
            FrameId::Track => self.set_track(content.as_u32().unwrap()),
            FrameId::TotalTracks => self.set_total_tracks(content.as_u32().unwrap()),
            FrameId::Disc => self.set_disc(content.as_u32().unwrap()),
            FrameId::Genre => self.set_genre(content.as_str().unwrap().to_owned()),
            FrameId::CustomText { key } => self.set_custom_text(
                key.to_owned(), content.as_str().unwrap().to_owned()),
        };
    }

    pub fn set_from(&mut self, other: &Box<dyn Tag>) {
        self.clear();
        for frame_id in other.frame_ids() {
            let frame_content = other.frame_content(&frame_id).unwrap();
            self.set_frame(&frame_id, frame_content);
        }
    }
}

pub fn read_from_path(path: impl AsRef<Path>, format: &str) -> Option<Result<Box<dyn Tag>>> {
    let context = || format!("Invalid tags in file {}", style_path(path.as_ref().display()));
    match format {
        "mp3" => Some(::id3::Tag::read_from_path(&path).map(|v| Box::new(v) as Box<dyn Tag>).with_context(context)),
        "m4a" => Some(::mp4ameta::Tag::read_from_path(&path).map(|v| Box::new(v) as Box<dyn Tag>).with_context(context)),
        "flac" => Some(::metaflac::Tag::read_from_path(&path).map(|v| Box::new(v) as Box<dyn Tag>).with_context(context)),
        _ => None
    }
}
