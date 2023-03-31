use super::*;

impl Tag for mp4ameta::Tag {
    fn frame_ids(&self) -> Vec<FrameId> {
        mp4ameta::Tag::data(self)
            .filter_map(|(ident, data)| match ident {
                mp4ameta::DataIdent::Fourcc(mp4ameta::ident::TITLE) => Some(vec![FrameId::Title]),
                mp4ameta::DataIdent::Fourcc(mp4ameta::ident::ALBUM) => Some(vec![FrameId::Album]),
                mp4ameta::DataIdent::Fourcc(mp4ameta::ident::ALBUM_ARTIST) => {
                    Some(vec![FrameId::AlbumArtist])
                }
                mp4ameta::DataIdent::Fourcc(mp4ameta::ident::ARTIST) => Some(vec![FrameId::Artist]),
                mp4ameta::DataIdent::Fourcc(mp4ameta::ident::YEAR) => Some(vec![FrameId::Year]),
                mp4ameta::DataIdent::Fourcc(mp4ameta::ident::TRACK_NUMBER) => {
                    Some(vec![FrameId::Track, FrameId::TotalTracks])
                }
                mp4ameta::DataIdent::Fourcc(mp4ameta::ident::DISC_NUMBER) => {
                    Some(vec![FrameId::Disc, FrameId::TotalDiscs])
                }
                mp4ameta::DataIdent::Fourcc(mp4ameta::ident::CUSTOM_GENRE) => {
                    Some(vec![FrameId::Genre])
                }
                mp4ameta::DataIdent::Freeform { name, .. } => {
                    if data.is_string() {
                        Some(vec![FrameId::CustomText {
                            key: name.to_owned(),
                        }])
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .flatten()
            .collect()
    }

    fn title(&self) -> Option<&str> {
        mp4ameta::Tag::title(self)
    }

    fn set_title(&mut self, title: Option<String>) {
        if let Some(title) = title {
            mp4ameta::Tag::set_title(self, title)
        } else {
            mp4ameta::Tag::remove_title(self)
        }
    }

    fn album(&self) -> Option<&str> {
        mp4ameta::Tag::album(self)
    }

    fn set_album(&mut self, album: Option<String>) {
        if let Some(album) = album {
            mp4ameta::Tag::set_album(self, album)
        } else {
            mp4ameta::Tag::remove_album(self)
        }
    }

    fn album_artist(&self) -> Option<&str> {
        mp4ameta::Tag::album_artist(self)
    }

    fn set_album_artist(&mut self, album_artist: Option<String>) {
        if let Some(album_artist) = album_artist {
            mp4ameta::Tag::set_album_artist(self, album_artist)
        } else {
            mp4ameta::Tag::remove_data_of(self, &mp4ameta::ident::ALBUM_ARTIST)
        }
    }

    fn artist(&self) -> Option<&str> {
        mp4ameta::Tag::artist(self)
    }

    fn set_artist(&mut self, artist: Option<String>) {
        if let Some(artist) = artist {
            mp4ameta::Tag::set_artist(self, artist)
        } else {
            mp4ameta::Tag::remove_data_of(self, &mp4ameta::ident::ARTIST)
        }
    }

    fn year(&self) -> Option<i32> {
        mp4ameta::Tag::year(self).and_then(|v| v.parse::<i32>().ok())
    }

    fn set_year(&mut self, year: Option<i32>) {
        if let Some(year) = year {
            mp4ameta::Tag::set_year(self, year.to_string())
        } else {
            mp4ameta::Tag::remove_year(self)
        }
    }

    fn track_number(&self) -> Option<u32> {
        mp4ameta::Tag::track_number(self).map(|v| v as u32)
    }

    fn set_track_number(&mut self, track: Option<u32>) {
        if let Some(track) = track {
            mp4ameta::Tag::set_track_number(self, track as u16)
        } else {
            mp4ameta::Tag::remove_track_number(self)
        }
    }

    fn total_tracks(&self) -> Option<u32> {
        mp4ameta::Tag::total_tracks(self).map(|v| v as u32)
    }

    fn set_total_tracks(&mut self, total_tracks: Option<u32>) {
        if let Some(total_tracks) = total_tracks {
            mp4ameta::Tag::set_total_tracks(self, total_tracks as u16)
        } else {
            mp4ameta::Tag::remove_total_tracks(self)
        }
    }

    fn disc(&self) -> Option<u32> {
        mp4ameta::Tag::disc_number(self).map(|v| v as u32)
    }

    fn set_disc(&mut self, disc: Option<u32>) {
        if let Some(disc) = disc {
            mp4ameta::Tag::set_disc_number(self, disc as u16)
        } else {
            mp4ameta::Tag::remove_disc_number(self)
        }
    }

    fn total_discs(&self) -> Option<u32> {
        mp4ameta::Tag::total_discs(self).map(|v| v as u32)
    }

    fn set_total_discs(&mut self, total_discs: Option<u32>) {
        if let Some(total_discs) = total_discs {
            mp4ameta::Tag::set_total_discs(self, total_discs as u16)
        } else {
            mp4ameta::Tag::remove_total_discs(self)
        }
    }

    fn genre(&self) -> Option<&str> {
        mp4ameta::Tag::genre(self)
    }

    fn set_genre(&mut self, genre: Option<String>) {
        if let Some(genre) = genre {
            mp4ameta::Tag::set_genre(self, genre)
        } else {
            mp4ameta::Tag::remove_data_of(self, &mp4ameta::ident::CUSTOM_GENRE)
        }
    }

    fn custom_text(&self, key: &str) -> Option<&str> {
        let ident =
            mp4ameta::DataIdent::from(mp4ameta::FreeformIdent::new("com.apple.iTunes", key));
        mp4ameta::Tag::strings(self)
            .find(|v| v.0 == &ident)
            .map(|v| v.1)
    }

    fn set_custom_text(&mut self, key: String, value: Option<String>) {
        let ident = mp4ameta::FreeformIdent::new("com.apple.iTunes", key.as_str());
        if let Some(value) = value {
            mp4ameta::Tag::set_data(self, ident, mp4ameta::Data::Utf8(value))
        } else {
            mp4ameta::Tag::remove_data_of(self, &ident)
        }
    }

    fn clear(&mut self) {
        mp4ameta::Tag::clear(self);
    }

    fn write_to(&self, file: &mut File) -> Result<()> {
        file.rewind()?;
        mp4ameta::Tag::write_to(self, file)?;
        Ok(())
    }
}
