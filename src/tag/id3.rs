use ::id3 as id3;

use super::*;

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
                    "TYER" |
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

    fn set_title(&mut self, title: Option<String>) {
        if let Some(title) = title {
            id3::TagLike::set_title(self, title)
        } else {
            id3::TagLike::remove_title(self)
        }
    }

    fn album(&self) -> Option<&str> {
        id3::TagLike::album(self)
    }

    fn set_album(&mut self, album: Option<String>) {
        if let Some(album) = album {
            id3::TagLike::set_album(self, album)
        } else {
            id3::TagLike::remove_album(self)
        }
    }

    fn album_artist(&self) -> Option<&str> {
        id3::TagLike::album_artist(self)
    }

    fn set_album_artist(&mut self, album_artist: Option<String>) {
        if let Some(album_artist) = album_artist {
            id3::TagLike::set_album_artist(self, album_artist)
        } else {
            id3::TagLike::remove_album_artist(self)
        }
    }

    fn artist(&self) -> Option<&str> {
        id3::TagLike::artist(self)
    }

    fn set_artist(&mut self, artist: Option<String>) {
        if let Some(artist) = artist {
            id3::TagLike::set_artist(self, artist)
        } else {
            id3::TagLike::remove_artist(self)
        }
    }

    fn year(&self) -> Option<i32> {
        id3::TagLike::date_recorded(self).map(|date| date.year)
            .or_else(|| {
                id3::TagLike::year(self)
            })
    }

    fn set_year(&mut self, year: Option<i32>) {
        if let Some(year) = year {
            id3::TagLike::set_date_recorded(self, id3::Timestamp {
                year,
                month: None,
                day: None,
                hour: None,
                minute: None,
                second: None,
            });
        } else {
            id3::TagLike::remove_date_recorded(self)
        }
    }

    fn track(&self) -> Option<u32> {
        id3::TagLike::track(self)
    }

    fn set_track(&mut self, track: Option<u32>) {
        if let Some(track) = track {
            id3::TagLike::set_track(self, track)
        } else {
            id3::TagLike::remove_track(self)
        }
    }

    fn total_tracks(&self) -> Option<u32> {
        id3::TagLike::total_tracks(self)
    }

    fn set_total_tracks(&mut self, total_tracks: Option<u32>) {
        if let Some(total_tracks) = total_tracks {
            id3::TagLike::set_total_tracks(self, total_tracks)
        } else {
            id3::TagLike::remove_total_tracks(self)
        }
    }

    fn disc(&self) -> Option<u32> {
        id3::TagLike::disc(self)
    }

    fn set_disc(&mut self, disc: Option<u32>) {
        if let Some(disc) = disc {
            id3::TagLike::set_disc(self, disc)
        } else {
            id3::TagLike::remove_disc(self)
        }
    }

    fn genre(&self) -> Option<&str> {
        id3::TagLike::genre(self)
    }

    fn set_genre(&mut self, genre: Option<String>) {
        if let Some(genre) = genre {
            id3::TagLike::set_genre(self, genre)
        } else {
            id3::TagLike::remove_genre(self)
        }
    }

    fn custom_text(&self, key: &str) -> Option<&str> {
        id3::TagLike::get(self, key)
            .and_then(|f| f.content().extended_text()).map(|v| v.value.as_str())
    }

    fn set_custom_text(&mut self, key: String, value: Option<String>) {
        if let Some(value) = value {
            id3::TagLike::add_frame(self, id3::frame::ExtendedText {
                description: key,
                value,
            });
        } else {
            id3::TagLike::remove(self, &key);
        }
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
