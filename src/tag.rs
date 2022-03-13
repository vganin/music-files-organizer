use std::fs::File;
use std::path::Path;

use id3::Timestamp;
use mp4ameta::{Data, DataIdent};

pub enum Kind {
    ID3,
    MP4,
}

pub enum FrameId<'a> {
    Id3(&'a str),
    Mp4(&'a mp4ameta::DataIdent),
}

pub trait Tag {
    fn kind(&self) -> Kind;
    fn frame_ids(&self) -> Vec<FrameId>;
    fn frame_content_as_string(&self, frame_id: &FrameId) -> Option<String>;
    fn frame_human_readable_title(&self, frame_id: &FrameId) -> Option<String>;
    fn title(&self) -> Option<&str>;
    fn album(&self) -> Option<&str>;
    fn album_artist(&self) -> Option<&str>;
    fn artist(&self) -> Option<&str>;
    fn year(&self) -> Option<i32>;
    fn track(&self) -> Option<u32>;
    fn disc(&self) -> Option<u32>;
    fn set_title(&mut self, title: String);
    fn set_album(&mut self, album: String);
    fn set_album_artist(&mut self, album_artist: String);
    fn set_artist(&mut self, artist: String);
    fn set_year(&mut self, year: i32);
    fn set_track(&mut self, year: u32);
    fn set_total_tracks(&mut self, total_tracks: u32);
    fn set_genre(&mut self, genre: String);
    fn set_custom_tag(&mut self, key: String, value: String);
    fn write_to(&self, file: &mut File);
}

pub fn new(kind: Kind) -> Box<dyn Tag> {
    match kind {
        Kind::ID3 => Box::new(id3::Tag::new()),
        Kind::MP4 => Box::new(mp4ameta::Tag::default())
    }
}

pub fn read_from_path(path: impl AsRef<Path>) -> Option<Box<dyn Tag>> {
    match path.as_ref().extension().unwrap().to_str().unwrap() {
        "mp3" => Some(Box::new(id3::Tag::read_from_path(&path).unwrap())),
        "m4a" => Some(Box::new(mp4ameta::Tag::read_from_path(&path).unwrap())),
        _ => None
    }
}

impl Tag for id3::Tag {
    fn kind(&self) -> Kind {
        Kind::ID3
    }

    fn frame_ids(&self) -> Vec<FrameId> {
        id3::Tag::frames(self).map(|v| FrameId::Id3(v.id())).collect()
    }

    fn frame_content_as_string(&self, frame_id: &FrameId) -> Option<String> {
        if let FrameId::Id3(frame_id) = frame_id {
            id3::TagLike::get(self, frame_id).map(|v| v.content().to_string())
        } else {
            None
        }
    }

    fn frame_human_readable_title(&self, frame_id: &FrameId) -> Option<String> {
        if let FrameId::Id3(frame_id) = frame_id {
            id3::TagLike::get(self, frame_id).map(|v| v.name().to_owned())
        } else {
            None
        }
    }

    fn title(&self) -> Option<&str> {
        id3::TagLike::title(self)
    }

    fn album(&self) -> Option<&str> {
        id3::TagLike::album(self)
    }

    fn album_artist(&self) -> Option<&str> {
        id3::TagLike::album_artist(self)
    }

    fn artist(&self) -> Option<&str> {
        id3::TagLike::artist(self)
    }

    fn year(&self) -> Option<i32> {
        id3::TagLike::date_recorded(self).map(|date| date.year)
    }

    fn track(&self) -> Option<u32> {
        id3::TagLike::track(self)
    }

    fn disc(&self) -> Option<u32> {
        id3::TagLike::disc(self)
    }

    fn set_title(&mut self, title: String) {
        id3::TagLike::set_title(self, title)
    }

    fn set_album(&mut self, album: String) {
        id3::TagLike::set_album(self, album)
    }

    fn set_album_artist(&mut self, album_artist: String) {
        id3::TagLike::set_album_artist(self, album_artist)
    }

    fn set_artist(&mut self, artist: String) {
        id3::TagLike::set_artist(self, artist)
    }

    fn set_year(&mut self, year: i32) {
        id3::TagLike::set_date_recorded(self, Timestamp {
            year,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        });
    }

    fn set_track(&mut self, track: u32) {
        id3::TagLike::set_track(self, track)
    }

    fn set_total_tracks(&mut self, total_tracks: u32) {
        id3::TagLike::set_total_tracks(self, total_tracks)
    }

    fn set_genre(&mut self, genre: String) {
        id3::TagLike::set_genre(self, genre)
    }

    fn set_custom_tag(&mut self, key: String, value: String) {
        id3::TagLike::add_frame(self, id3::frame::ExtendedText {
            description: key,
            value,
        });
    }

    fn write_to(&self, file: &mut File) {
        id3::v1::Tag::remove(file).unwrap();
        id3::Encoder::new()
            .version(id3::Version::Id3v24)
            .encode_to_file(self, file)
            .unwrap()
    }
}

impl Tag for mp4ameta::Tag {
    fn kind(&self) -> Kind {
        Kind::MP4
    }

    fn frame_ids(&self) -> Vec<FrameId> {
        mp4ameta::Tag::data(self).map(|v| FrameId::Mp4(v.0)).collect()
    }

    fn frame_content_as_string(&self, frame_id: &FrameId) -> Option<String> {
        if let FrameId::Mp4(ident) = frame_id {
            mp4ameta::Tag::data_of(self, *ident).next()
                .and_then(|v| {
                    match v {
                        Data::Utf8(string) | Data::Utf16(string) => Some(string.to_owned()),
                        Data::Reserved(_) => None,
                        Data::Jpeg(_) => None,
                        Data::Png(_) => None,
                        Data::BeSigned(_) => None,
                        Data::Bmp(_) => None,
                    }
                })
        } else {
            None
        }
    }

    fn frame_human_readable_title(&self, frame_id: &FrameId) -> Option<String> {
        if let FrameId::Mp4(ident) = frame_id {
            match ident {
                DataIdent::Fourcc(mp4ameta::ident::TITLE) => Some("Title".to_owned()),
                DataIdent::Fourcc(mp4ameta::ident::ALBUM) => Some("Album".to_owned()),
                DataIdent::Fourcc(mp4ameta::ident::ALBUM_ARTIST) => Some("Album Artist".to_owned()),
                DataIdent::Fourcc(mp4ameta::ident::ARTIST) => Some("Artist".to_owned()),
                DataIdent::Fourcc(mp4ameta::ident::YEAR) => Some("Year".to_owned()),
                DataIdent::Fourcc(mp4ameta::ident::TRACK_NUMBER) => Some("Track number".to_owned()),
                DataIdent::Fourcc(mp4ameta::ident::DISC_NUMBER) => Some("Disc number".to_owned()),
                DataIdent::Fourcc(mp4ameta::ident::CUSTOM_GENRE) => Some("Genre".to_owned()),
                DataIdent::Freeform { mean: _mean, name } => Some(name.to_owned()),
                _ => None
            }
        } else {
            None
        }
    }

    fn title(&self) -> Option<&str> {
        mp4ameta::Tag::title(self)
    }

    fn album(&self) -> Option<&str> {
        mp4ameta::Tag::album(self)
    }

    fn album_artist(&self) -> Option<&str> {
        mp4ameta::Tag::album_artist(self)
    }

    fn artist(&self) -> Option<&str> {
        mp4ameta::Tag::artist(self)
    }

    fn year(&self) -> Option<i32> {
        mp4ameta::Tag::year(self).map(|v| v.parse::<i32>().unwrap())
    }

    fn track(&self) -> Option<u32> {
        mp4ameta::Tag::track_number(self).map(|v| v as u32)
    }

    fn disc(&self) -> Option<u32> {
        mp4ameta::Tag::disc_number(self).map(|v| v as u32)
    }

    fn set_title(&mut self, title: String) {
        mp4ameta::Tag::set_title(self, title)
    }

    fn set_album(&mut self, album: String) {
        mp4ameta::Tag::set_album(self, album)
    }

    fn set_album_artist(&mut self, album_artist: String) {
        mp4ameta::Tag::set_album_artist(self, album_artist)
    }

    fn set_artist(&mut self, artist: String) {
        mp4ameta::Tag::set_artist(self, artist)
    }

    fn set_year(&mut self, year: i32) {
        mp4ameta::Tag::set_year(self, year.to_string())
    }

    fn set_track(&mut self, track: u32) {
        mp4ameta::Tag::set_track_number(self, track as u16)
    }

    fn set_total_tracks(&mut self, total_tracks: u32) {
        mp4ameta::Tag::set_total_tracks(self, total_tracks as u16)
    }

    fn set_genre(&mut self, genre: String) {
        mp4ameta::Tag::set_genre(self, genre)
    }

    fn set_custom_tag(&mut self, key: String, value: String) {
        mp4ameta::Tag::set_data(
            self,
            mp4ameta::FreeformIdent::new("com.apple.iTunes", key.as_str()),
            Data::Utf8(value),
        )
    }

    fn write_to(&self, file: &mut File) {
        mp4ameta::Tag::write_to(self, file).unwrap()
    }
}
