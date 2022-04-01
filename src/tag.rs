use std::fs::File;
use std::io;
use std::io::{Seek, Write};
use std::path::Path;

use dyn_clone::DynClone;
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

pub trait Tag: DynClone {
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

pub fn read_from_path(path: impl AsRef<Path>) -> Option<Box<dyn Tag>> {
    match path.as_ref().extension().unwrap().to_str().unwrap() {
        "mp3" => Some(Box::new(id3::Tag::read_from_path(&path).unwrap())),
        "m4a" => Some(Box::new(mp4ameta::Tag::read_from_path(&path).unwrap())),
        "flac" => Some(Box::new(metaflac::Tag::read_from_path(&path).unwrap())),
        _ => None
    }
}

impl Tag for id3::Tag {
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

    fn set_disc(&mut self, disc: u32) {
        id3::TagLike::set_disc(self, disc)
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

    fn clear(&mut self) {
        id3::TagLike::frames_vec_mut(self).clear();
    }

    fn write_to(&self, file: &mut File) {
        file.seek(io::SeekFrom::Start(0)).unwrap();
        id3::v1::Tag::remove(file).unwrap();
        file.seek(io::SeekFrom::Start(0)).unwrap();
        id3::Encoder::new()
            .version(id3::Version::Id3v24)
            .encode_to_file(self, file)
            .unwrap()
    }
}

impl Tag for mp4ameta::Tag {
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

    fn clear(&mut self) {
        mp4ameta::Tag::clear(self);
    }

    fn write_to(&self, file: &mut File) {
        file.seek(io::SeekFrom::Start(0)).unwrap();
        mp4ameta::Tag::write_to(self, file).unwrap()
    }
}

const FLAC_TITLE: &str = "TITLE";
const FLAC_ALBUM: &str = "ALBUM";
const FLAC_ALBUM_ARTIST: &str = "ALBUMARTIST";
const FLAC_ARTIST: &str = "ARTIST";
const FLAC_YEAR: &str = "DATE";
const FLAC_TRACK: &str = "TRACKNUMBER";
const FLAC_TOTAL_TRACKS: &str = "TOTALTRACKS";
const FLAC_DISC: &str = "DISCNUMBER";
const FLAC_GENRE: &str = "GENRE";

impl Tag for metaflac::Tag {
    fn frame_ids(&self) -> Vec<FrameId> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| v.comments.keys())
            .unwrap()
            .map(|key| {
                match key.as_str() {
                    FLAC_TITLE => FrameId::Title,
                    FLAC_ALBUM => FrameId::Album,
                    FLAC_ALBUM_ARTIST => FrameId::AlbumArtist,
                    FLAC_ARTIST => FrameId::Artist,
                    FLAC_YEAR => FrameId::Year,
                    FLAC_TRACK => FrameId::Track,
                    FLAC_TOTAL_TRACKS => FrameId::TotalTracks,
                    FLAC_DISC => FrameId::Disc,
                    FLAC_GENRE => FrameId::Genre,
                    key => FrameId::CustomText { key: key.to_owned() }
                }
            })
            .collect()
    }

    fn title(&self) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| v.title().map(|v| v.iter().next()).flatten())
            .flatten()
            .map(|v| v.as_str())
    }

    fn set_title(&mut self, title: String) {
        metaflac::Tag::vorbis_comments_mut(self).set_title(vec![title]);
    }

    fn album(&self) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| v.album().map(|v| v.iter().next()).flatten())
            .flatten()
            .map(|v| v.as_str())
    }

    fn set_album(&mut self, album: String) {
        metaflac::Tag::vorbis_comments_mut(self).set_album(vec![album]);
    }

    fn album_artist(&self) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| v.album_artist().map(|v| v.iter().next()).flatten())
            .flatten()
            .map(|v| v.as_str())
    }

    fn set_album_artist(&mut self, album_artist: String) {
        metaflac::Tag::vorbis_comments_mut(self).set_album_artist(vec![album_artist]);
    }

    fn artist(&self) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| v.artist().map(|v| v.iter().next()).flatten())
            .flatten()
            .map(|v| v.as_str())
    }

    fn set_artist(&mut self, artist: String) {
        metaflac::Tag::vorbis_comments_mut(self).set_artist(vec![artist]);
    }

    fn year(&self) -> Option<i32> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| {
                v.get(FLAC_YEAR).and_then(|s| {
                    if !s.is_empty() {
                        s[0].parse::<i32>().ok()
                    } else {
                        None
                    }
                })
            })
            .flatten()
    }

    fn set_year(&mut self, year: i32) {
        metaflac::Tag::vorbis_comments_mut(self).set(FLAC_YEAR, vec![format!("{}", year)]);
    }

    fn track(&self) -> Option<u32> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| v.track())
            .flatten()
    }

    fn set_track(&mut self, track: u32) {
        metaflac::Tag::vorbis_comments_mut(self).set_track(track);
    }

    fn total_tracks(&self) -> Option<u32> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| v.total_tracks())
            .flatten()
    }

    fn set_total_tracks(&mut self, total_tracks: u32) {
        metaflac::Tag::vorbis_comments_mut(self).set_total_tracks(total_tracks);
    }

    fn disc(&self) -> Option<u32> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| {
                v.get(FLAC_DISC).and_then(|s| {
                    if !s.is_empty() {
                        s[0].parse::<u32>().ok()
                    } else {
                        None
                    }
                })
            })
            .flatten()
    }

    fn set_disc(&mut self, disc: u32) {
        metaflac::Tag::vorbis_comments_mut(self).set(FLAC_DISC, vec![format!("{}", disc)]);
    }

    fn genre(&self) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| v.genre().map(|v| v.iter().next()).flatten())
            .flatten()
            .map(|v| v.as_str())
    }

    fn set_genre(&mut self, genre: String) {
        metaflac::Tag::vorbis_comments_mut(self).set_genre(vec![genre]);
    }

    fn custom_text(&self, key: &str) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .map(|v| v.get(key).map(|v| v.iter().next()).flatten())
            .flatten()
            .map(|v| v.as_str())
    }

    fn set_custom_text(&mut self, key: String, value: String) {
        metaflac::Tag::vorbis_comments_mut(self).set(key, vec![value]);
    }

    fn clear(&mut self) {
        let stream_info = metaflac::Tag::get_streaminfo(self).unwrap().to_owned();
        *self = metaflac::Tag::default();
        metaflac::Tag::set_streaminfo(self, stream_info);
    }

    fn write_to(&self, file: &mut File) {
        file.seek(io::SeekFrom::Start(0)).unwrap();
        let data = metaflac::Tag::skip_metadata(file);

        file.seek(io::SeekFrom::Start(0)).unwrap();
        file.set_len(0).unwrap();

        file.write_all(b"fLaC").unwrap();

        let blocks: Vec<&metaflac::Block> = self.blocks().collect();
        let blocks_count = blocks.len();
        for i in 0..blocks_count {
            let block = blocks[i];
            block.write_to(i == blocks_count - 1, file).unwrap();
        }

        file.write_all(&data[..]).unwrap();
    }
}
