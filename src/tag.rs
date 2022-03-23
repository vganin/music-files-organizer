use std::fs::File;
use std::path::Path;

use id3::Timestamp;
use mp4ameta::{Data, DataIdent};

pub enum FrameId {
    Title,
    Album,
    AlbumArtist,
    Artist,
    Year,
    Track,
    TotalTracks,
    Disc,
    Genre,
    CustomText { key: String },
}

#[derive(PartialEq)]
pub enum FrameContent {
    Str(String),
    I32(i32),
    U32(u32),
}

impl FrameContent {
    fn as_str(&self) -> Option<&str> {
        match self {
            FrameContent::Str(v) => Some(v),
            _ => None
        }
    }

    fn as_i32(&self) -> Option<i32> {
        match self {
            FrameContent::I32(v) => Some(*v),
            _ => None
        }
    }

    fn as_u32(&self) -> Option<u32> {
        match self {
            FrameContent::U32(v) => Some(*v),
            _ => None
        }
    }
}

impl FrameId {
    pub fn description(&self) -> &str {
        match self {
            FrameId::Title => "Title",
            FrameId::Album => "Album",
            FrameId::AlbumArtist => "Album Artist",
            FrameId::Artist => "Artist",
            FrameId::Year => "Year",
            FrameId::Track => "Track",
            FrameId::TotalTracks => "Total Tracks",
            FrameId::Disc => "Disc",
            FrameId::Genre => "Genre",
            FrameId::CustomText { key } => key,
        }
    }
}

impl FrameContent {
    pub fn stringify_content(&self) -> String {
        match self {
            FrameContent::Str(v) => v.to_owned(),
            FrameContent::I32(v) => v.to_string(),
            FrameContent::U32(v) => v.to_string(),
        }
    }
}

pub trait Tag {
    fn kind(&self) -> Kind;

    fn frame_ids(&self) -> Vec<FrameId>;
    fn frame_content(&self, id: &FrameId) -> Option<FrameContent> {
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
    fn set_frame(&mut self, id: &FrameId, content: FrameContent) {
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
    fn set_track(&mut self, year: u32);

    fn total_tracks(&self) -> Option<u32>;
    fn set_total_tracks(&mut self, total_tracks: u32);

    fn disc(&self) -> Option<u32>;
    fn set_disc(&mut self, year: u32);

    fn genre(&self) -> Option<&str>;
    fn set_genre(&mut self, genre: String);

    fn custom_text(&self, key: &str) -> Option<&str>;
    fn set_custom_text(&mut self, key: String, value: String);

    fn write_to(&self, file: &mut File);
}

pub enum Kind {
    Id3,
    Mp4,
}

pub fn new(kind: Kind) -> Box<dyn Tag> {
    match kind {
        Kind::Id3 => Box::new(id3::Tag::new()),
        Kind::Mp4 => Box::new(mp4ameta::Tag::default()),
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
        Kind::Id3
    }

    fn frame_ids(&self) -> Vec<FrameId> {
        id3::Tag::frames(self)
            .into_iter()
            .filter_map(|frame| {
                match frame.id() {
                    "TIT2" => Some(vec![FrameId::Title]),
                    "TALB" => Some(vec![FrameId::Album]),
                    "TPE2" => Some(vec![FrameId::AlbumArtist]),
                    "TPE1" => Some(vec![FrameId::Artist]),
                    "TDRC" => Some(vec![FrameId::Year]),
                    "TRCK" => Some(vec![FrameId::Track, FrameId::TotalTracks]),
                    "TPOS" => Some(vec![FrameId::Disc]),
                    "TCON" => Some(vec![FrameId::Genre]),
                    "TXXX" => Some(vec![FrameId::CustomText {
                        key: frame.content().extended_text().unwrap().description.to_owned()
                    }]),
                    _ => None
                }
            })
            .flatten()
            .collect()
    }

    fn title(&self) -> Option<&str> {
        id3::TagLike::title(self)
    }

    fn set_title(&mut self, title: String) {
        id3::TagLike::set_title(self, title)
    }

    fn album(&self) -> Option<&str> {
        id3::TagLike::album(self)
    }

    fn set_album(&mut self, album: String) {
        id3::TagLike::set_album(self, album)
    }

    fn album_artist(&self) -> Option<&str> {
        id3::TagLike::album_artist(self)
    }

    fn set_album_artist(&mut self, album_artist: String) {
        id3::TagLike::set_album_artist(self, album_artist)
    }

    fn artist(&self) -> Option<&str> {
        id3::TagLike::artist(self)
    }

    fn set_artist(&mut self, artist: String) {
        id3::TagLike::set_artist(self, artist)
    }

    fn year(&self) -> Option<i32> {
        id3::TagLike::date_recorded(self).map(|date| date.year)
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

    fn track(&self) -> Option<u32> {
        id3::TagLike::track(self)
    }

    fn set_track(&mut self, track: u32) {
        id3::TagLike::set_track(self, track)
    }

    fn total_tracks(&self) -> Option<u32> {
        id3::TagLike::total_tracks(self)
    }

    fn set_total_tracks(&mut self, total_tracks: u32) {
        id3::TagLike::set_total_tracks(self, total_tracks)
    }

    fn disc(&self) -> Option<u32> {
        id3::TagLike::disc(self)
    }

    fn set_disc(&mut self, total_tracks: u32) {
        id3::TagLike::set_disc(self, total_tracks)
    }

    fn genre(&self) -> Option<&str> {
        id3::TagLike::genre(self)
    }

    fn set_genre(&mut self, genre: String) {
        id3::TagLike::set_genre(self, genre)
    }

    fn custom_text(&self, key: &str) -> Option<&str> {
        id3::TagLike::get(self, key)
            .and_then(|f| f.content().extended_text()).map(|v| v.value.as_str())
    }

    fn set_custom_text(&mut self, key: String, value: String) {
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
        Kind::Mp4
    }

    fn frame_ids(&self) -> Vec<FrameId> {
        mp4ameta::Tag::data(self)
            .filter_map(|(ident, data)| {
                match ident {
                    DataIdent::Fourcc(mp4ameta::ident::TITLE) => Some(vec![FrameId::Title]),
                    DataIdent::Fourcc(mp4ameta::ident::ALBUM) => Some(vec![FrameId::Album]),
                    DataIdent::Fourcc(mp4ameta::ident::ALBUM_ARTIST) => Some(vec![FrameId::AlbumArtist]),
                    DataIdent::Fourcc(mp4ameta::ident::ARTIST) => Some(vec![FrameId::Artist]),
                    DataIdent::Fourcc(mp4ameta::ident::YEAR) => Some(vec![FrameId::Year]),
                    DataIdent::Fourcc(mp4ameta::ident::TRACK_NUMBER) => Some(vec![FrameId::Track, FrameId::TotalTracks]),
                    DataIdent::Fourcc(mp4ameta::ident::DISC_NUMBER) => Some(vec![FrameId::Disc]),
                    DataIdent::Fourcc(mp4ameta::ident::CUSTOM_GENRE) => Some(vec![FrameId::Genre]),
                    DataIdent::Freeform { name, .. } => if data.is_string() {
                        Some(vec![FrameId::CustomText { key: name.to_owned() }])
                    } else {
                        None
                    }
                    _ => None
                }
            })
            .flatten()
            .collect()
    }

    fn title(&self) -> Option<&str> {
        mp4ameta::Tag::title(self)
    }

    fn set_title(&mut self, title: String) {
        mp4ameta::Tag::set_title(self, title)
    }

    fn album(&self) -> Option<&str> {
        mp4ameta::Tag::album(self)
    }

    fn set_album(&mut self, album: String) {
        mp4ameta::Tag::set_album(self, album)
    }

    fn album_artist(&self) -> Option<&str> {
        mp4ameta::Tag::album_artist(self)
    }

    fn set_album_artist(&mut self, album_artist: String) {
        mp4ameta::Tag::set_album_artist(self, album_artist)
    }

    fn artist(&self) -> Option<&str> {
        mp4ameta::Tag::artist(self)
    }

    fn set_artist(&mut self, artist: String) {
        mp4ameta::Tag::set_artist(self, artist)
    }

    fn year(&self) -> Option<i32> {
        mp4ameta::Tag::year(self).map(|v| v.parse::<i32>().unwrap())
    }

    fn set_year(&mut self, year: i32) {
        mp4ameta::Tag::set_year(self, year.to_string())
    }

    fn track(&self) -> Option<u32> {
        mp4ameta::Tag::track_number(self).map(|v| v as u32)
    }

    fn set_track(&mut self, track: u32) {
        mp4ameta::Tag::set_track_number(self, track as u16)
    }

    fn total_tracks(&self) -> Option<u32> {
        mp4ameta::Tag::total_tracks(self).map(|v| v as u32)
    }

    fn set_total_tracks(&mut self, total_tracks: u32) {
        mp4ameta::Tag::set_total_tracks(self, total_tracks as u16)
    }

    fn disc(&self) -> Option<u32> {
        mp4ameta::Tag::disc_number(self).map(|v| v as u32)
    }

    fn set_disc(&mut self, disc: u32) {
        mp4ameta::Tag::set_disc_number(self, disc as u16)
    }

    fn genre(&self) -> Option<&str> {
        mp4ameta::Tag::genre(self)
    }

    fn set_genre(&mut self, genre: String) {
        mp4ameta::Tag::set_genre(self, genre)
    }

    fn custom_text(&self, key: &str) -> Option<&str> {
        let ident = DataIdent::from(mp4ameta::FreeformIdent::new("com.apple.iTunes", key));
        mp4ameta::Tag::strings(self).find(|v| v.0 == &ident)
            .map(|v| v.1)
    }

    fn set_custom_text(&mut self, key: String, value: String) {
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
