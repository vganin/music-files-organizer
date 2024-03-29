use ::id3;
use anyhow::Result;
use itertools::Itertools;

use super::*;

impl Tag for id3::Tag {
    fn frame_ids(&self) -> Vec<FrameId> {
        id3::Tag::frames(self)
            .into_iter()
            .flat_map(|frame| match frame.id() {
                "TIT2" => vec![FrameId::Title],
                "TALB" => vec![FrameId::Album],
                "TPE2" => vec![FrameId::AlbumArtist],
                "TPE1" => vec![FrameId::Artist],
                "TYER" | "TDRC" => vec![FrameId::Year],
                "TRCK" => vec![FrameId::Track, FrameId::TotalTracks],
                "TPOS" => vec![FrameId::Disc, FrameId::TotalDiscs],
                "TCON" => vec![FrameId::Genre],
                "TXXX" => frame
                    .content()
                    .extended_text()
                    .into_iter()
                    .map(|extended_text| FrameId::CustomText {
                        key: extended_text.description.to_owned(),
                    })
                    .collect_vec(),
                _ => vec![],
            })
            .collect_vec()
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
        id3::TagLike::date_recorded(self)
            .map(|date| date.year)
            .or_else(|| id3::TagLike::year(self))
    }

    fn set_year(&mut self, year: Option<i32>) {
        if let Some(year) = year {
            id3::TagLike::set_date_recorded(
                self,
                id3::Timestamp {
                    year,
                    month: None,
                    day: None,
                    hour: None,
                    minute: None,
                    second: None,
                },
            );
        } else {
            id3::TagLike::remove_date_recorded(self)
        }
    }

    fn track_number(&self) -> Option<u32> {
        id3::TagLike::track(self)
    }

    fn set_track_number(&mut self, track: Option<u32>) {
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

    fn total_discs(&self) -> Option<u32> {
        id3::TagLike::total_discs(self)
    }

    fn set_total_discs(&mut self, total_discs: Option<u32>) {
        if let Some(total_discs) = total_discs {
            id3::TagLike::set_total_discs(self, total_discs)
        } else {
            id3::TagLike::remove_total_discs(self)
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
        id3::Tag::extended_texts(self)
            .find(|v| v.description == key)
            .map(|v| v.value.as_str())
    }

    fn set_custom_text(&mut self, key: String, value: Option<String>) {
        if let Some(value) = value {
            id3::TagLike::add_frame(
                self,
                id3::frame::ExtendedText {
                    description: key,
                    value,
                },
            );
        } else {
            id3::TagLike::remove_extended_text(self, Some(&key), None);
        }
    }

    fn clear(&mut self) {
        id3::TagLike::frames_vec_mut(self).clear();
    }

    fn write_to(&self, file: &mut File) -> Result<()> {
        file.rewind()?;
        id3::v1::Tag::remove(file)?;
        file.rewind()?;
        id3::Encoder::new()
            .version(id3::Version::Id3v24)
            .encode_to_file(self, file)?;
        Ok(())
    }
}
