use super::*;

impl Tag for metaflac::Tag {
    fn frame_ids(&self) -> Vec<FrameId> {
        metaflac::Tag::vorbis_comments(self)
            .iter()
            .flat_map(|v| v.comments.keys())
            .map(|key| match key.as_str() {
                FLAC_TITLE => FrameId::Title,
                FLAC_ALBUM => FrameId::Album,
                FLAC_ALBUM_ARTIST => FrameId::AlbumArtist,
                FLAC_ARTIST => FrameId::Artist,
                FLAC_YEAR => FrameId::Year,
                FLAC_TRACK => FrameId::Track,
                FLAC_TOTAL_TRACKS => FrameId::TotalTracks,
                FLAC_DISC => FrameId::Disc,
                FLAC_GENRE => FrameId::Genre,
                key => FrameId::CustomText {
                    key: key.to_owned(),
                },
            })
            .collect()
    }

    fn title(&self) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .and_then(|v| v.title().and_then(|v| v.iter().next()))
            .map(|v| v.as_str())
    }

    fn set_title(&mut self, title: Option<String>) {
        let comments = metaflac::Tag::vorbis_comments_mut(self);
        if let Some(title) = title {
            comments.set_title(vec![title]);
        } else {
            comments.remove_title();
        }
    }

    fn album(&self) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .and_then(|v| v.album().and_then(|v| v.iter().next()))
            .map(|v| v.as_str())
    }

    fn set_album(&mut self, album: Option<String>) {
        let comments = metaflac::Tag::vorbis_comments_mut(self);
        if let Some(album) = album {
            comments.set_album(vec![album]);
        } else {
            comments.remove_album();
        }
    }

    fn album_artist(&self) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .and_then(|v| v.album_artist().and_then(|v| v.iter().next()))
            .map(|v| v.as_str())
    }

    fn set_album_artist(&mut self, album_artist: Option<String>) {
        let comments = metaflac::Tag::vorbis_comments_mut(self);
        if let Some(album_artist) = album_artist {
            comments.set_album_artist(vec![album_artist]);
        } else {
            comments.remove_album_artist();
        }
    }

    fn artist(&self) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .and_then(|v| v.artist().and_then(|v| v.iter().next()))
            .map(|v| v.as_str())
    }

    fn set_artist(&mut self, artist: Option<String>) {
        let comments = metaflac::Tag::vorbis_comments_mut(self);
        if let Some(artist) = artist {
            comments.set_artist(vec![artist]);
        } else {
            comments.remove_artist();
        }
    }

    fn year(&self) -> Option<i32> {
        metaflac::Tag::vorbis_comments(self).and_then(|v| {
            v.get(FLAC_YEAR).and_then(|s| {
                if !s.is_empty() {
                    s[0].parse::<i32>().ok()
                } else {
                    None
                }
            })
        })
    }

    fn set_year(&mut self, year: Option<i32>) {
        let comments = metaflac::Tag::vorbis_comments_mut(self);
        if let Some(year) = year {
            comments.set(FLAC_YEAR, vec![format!("{}", year)]);
        } else {
            comments.remove(FLAC_YEAR);
        }
    }

    fn track_number(&self) -> Option<u32> {
        metaflac::Tag::vorbis_comments(self).and_then(|v| {
            v.track()
                .or_else(|| Some(vorbis_comment_as_pair(v, FLAC_TRACK)?.0))
        })
    }

    fn set_track_number(&mut self, track: Option<u32>) {
        let comments = metaflac::Tag::vorbis_comments_mut(self);
        if let Some(track) = track {
            comments.set_track(track);
        } else {
            comments.remove_track();
        }
    }

    fn total_tracks(&self) -> Option<u32> {
        metaflac::Tag::vorbis_comments(self).and_then(|v| {
            v.total_tracks()
                .or_else(|| vorbis_comment_as_pair(v, FLAC_TRACK)?.1)
        })
    }

    fn set_total_tracks(&mut self, total_tracks: Option<u32>) {
        let comments = metaflac::Tag::vorbis_comments_mut(self);
        if let Some(total_tracks) = total_tracks {
            comments.set_total_tracks(total_tracks);
        } else {
            comments.remove_total_tracks()
        }
    }

    fn disc(&self) -> Option<u32> {
        metaflac::Tag::vorbis_comments(self).and_then(|v| {
            v.get(FLAC_DISC).and_then(|s| {
                if !s.is_empty() {
                    s[0].parse::<u32>().ok()
                } else {
                    None
                }
            })
        })
    }

    fn set_disc(&mut self, disc: Option<u32>) {
        let comments = metaflac::Tag::vorbis_comments_mut(self);
        if let Some(disc) = disc {
            comments.set(FLAC_DISC, vec![format!("{}", disc)]);
        } else {
            comments.remove(FLAC_DISC)
        }
    }

    fn total_discs(&self) -> Option<u32> {
        // no-op
        None
    }

    fn set_total_discs(&mut self, _total_discs: Option<u32>) {
        // no-op
    }

    fn genre(&self) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .and_then(|v| v.genre().and_then(|v| v.iter().next()))
            .map(|v| v.as_str())
    }

    fn set_genre(&mut self, genre: Option<String>) {
        let comments = metaflac::Tag::vorbis_comments_mut(self);
        if let Some(genre) = genre {
            comments.set_genre(vec![genre]);
        } else {
            comments.remove_genre()
        }
    }

    fn custom_text(&self, key: &str) -> Option<&str> {
        metaflac::Tag::vorbis_comments(self)
            .and_then(|v| v.get(key).and_then(|v| v.iter().next()))
            .map(|v| v.as_str())
    }

    fn set_custom_text(&mut self, key: String, value: Option<String>) {
        let comments = metaflac::Tag::vorbis_comments_mut(self);
        if let Some(value) = value {
            comments.set(key, vec![value]);
        } else {
            comments.remove(&key)
        }
    }

    fn clear(&mut self) {
        #![allow(clippy::unwrap_used)] // FIXME: Should deal with absence of media info
        let stream_info = metaflac::Tag::get_streaminfo(self).unwrap().to_owned();
        *self = metaflac::Tag::default();
        metaflac::Tag::set_streaminfo(self, stream_info);
    }

    fn write_to(&self, file: &mut File) -> Result<()> {
        file.rewind()?;
        let data = metaflac::Tag::skip_metadata(file);

        file.rewind()?;
        file.set_len(0)?;

        file.write_all(b"fLaC")?;

        let blocks: Vec<&metaflac::Block> = self.blocks().collect();
        let blocks_count = blocks.len();
        for (i, block) in blocks.iter().enumerate() {
            block.write_to(i == blocks_count - 1, file)?;
        }

        file.write_all(&data[..])?;

        Ok(())
    }
}

fn vorbis_comment_as_pair(
    tag: &metaflac::block::VorbisComment,
    id: &str,
) -> Option<(u32, Option<u32>)> {
    let text = tag.get(id)?.first()?;
    let mut split = text.splitn(2, &['\0', '/'][..]);
    let a = split.next()?.parse().ok()?;
    let b = split.next().and_then(|s| s.parse().ok());
    Some((a, b))
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
