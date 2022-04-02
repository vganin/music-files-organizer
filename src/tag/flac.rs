use super::*;

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

const FLAC_TITLE: &str = "TITLE";
const FLAC_ALBUM: &str = "ALBUM";
const FLAC_ALBUM_ARTIST: &str = "ALBUMARTIST";
const FLAC_ARTIST: &str = "ARTIST";
const FLAC_YEAR: &str = "DATE";
const FLAC_TRACK: &str = "TRACKNUMBER";
const FLAC_TOTAL_TRACKS: &str = "TOTALTRACKS";
const FLAC_DISC: &str = "DISCNUMBER";
const FLAC_GENRE: &str = "GENRE";
