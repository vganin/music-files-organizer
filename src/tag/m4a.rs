use super::*;

impl Tag for mp4ameta::Tag {
    fn frame_ids(&self) -> Vec<FrameId> {
        mp4ameta::Tag::data(self)
            .filter_map(|(ident, data)| {
                match ident {
                    mp4ameta::DataIdent::Fourcc(mp4ameta::ident::TITLE) => Some(vec![FrameId::Title]),
                    mp4ameta::DataIdent::Fourcc(mp4ameta::ident::ALBUM) => Some(vec![FrameId::Album]),
                    mp4ameta::DataIdent::Fourcc(mp4ameta::ident::ALBUM_ARTIST) => Some(vec![FrameId::AlbumArtist]),
                    mp4ameta::DataIdent::Fourcc(mp4ameta::ident::ARTIST) => Some(vec![FrameId::Artist]),
                    mp4ameta::DataIdent::Fourcc(mp4ameta::ident::YEAR) => Some(vec![FrameId::Year]),
                    mp4ameta::DataIdent::Fourcc(mp4ameta::ident::TRACK_NUMBER) => Some(vec![FrameId::Track, FrameId::TotalTracks]),
                    mp4ameta::DataIdent::Fourcc(mp4ameta::ident::DISC_NUMBER) => Some(vec![FrameId::Disc]),
                    mp4ameta::DataIdent::Fourcc(mp4ameta::ident::CUSTOM_GENRE) => Some(vec![FrameId::Genre]),
                    mp4ameta::DataIdent::Freeform { name, .. } => if data.is_string() {
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
        let ident = mp4ameta::DataIdent::from(mp4ameta::FreeformIdent::new("com.apple.iTunes", key));
        mp4ameta::Tag::strings(self).find(|v| v.0 == &ident)
            .map(|v| v.1)
    }

    fn set_custom_text(&mut self, key: String, value: String) {
        mp4ameta::Tag::set_data(
            self,
            mp4ameta::FreeformIdent::new("com.apple.iTunes", key.as_str()),
            mp4ameta::Data::Utf8(value),
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
