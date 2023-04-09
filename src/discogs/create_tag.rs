use std::borrow::ToOwned;
use std::string::ToString;

use anyhow::Result;
use itertools::Itertools;
use once_cell::sync::Lazy;

use crate::discogs::model::refined::{DiscogsRelease, DiscogsTrack};
use crate::tag::frame::FrameId;
use crate::tag::Tag;

#[allow(clippy::borrowed_box)]
pub fn create_tag_from_discogs_data(
    original_tag: &Box<dyn Tag>, // FIXME: Can't create new tag without "template" for now
    discogs_track: &DiscogsTrack,
    discogs_release: &DiscogsRelease,
) -> Result<Box<dyn Tag>> {
    let mut new_tag = original_tag.clone();
    new_tag.clear();

    new_tag.set_title(Some(discogs_track.title.to_owned()));
    new_tag.set_album(Some(discogs_release.title.to_owned()));
    let album_artists: Vec<(&str, &str)> = discogs_release
        .artists
        .iter()
        .map(|artist| (artist.name.as_str(), artist.join.as_deref().unwrap_or("&")))
        .collect_vec();
    let track_artists: Option<Vec<(&str, &str)>> = discogs_track.artists.as_ref().map(|artists| {
        artists
            .iter()
            .map(|artist| (artist.name.as_str(), artist.join.as_deref().unwrap_or("&")))
            .collect_vec()
    });
    new_tag.set_album_artist(Some(if track_artists.is_some() {
        "Various Artists".to_owned()
    } else {
        album_artists
            .iter()
            .flat_map(|v| [v.0, (v.1)])
            .collect::<Vec<&str>>()
            .join(" ")
            .trim()
            .to_owned()
    }));
    new_tag.set_artist(Some(
        track_artists
            .unwrap_or(album_artists)
            .iter()
            .flat_map(|v| [v.0, (v.1)])
            .collect_vec()
            .join(" ")
            .trim()
            .to_owned(),
    ));
    new_tag.set_year(Some(discogs_release.year));
    new_tag.set_track_number(Some(discogs_track.position));
    new_tag.set_total_tracks(Some(
        discogs_release.disc_to_total_tracks[&discogs_track.disc],
    ));
    let total_discs = discogs_release.disc_to_total_tracks.keys().len() as u32;
    if total_discs > 1 {
        new_tag.set_disc(Some(discogs_track.disc));
        new_tag.set_total_discs(Some(total_discs));
    }
    new_tag.set_genre(Some(
        discogs_release
            .styles
            .as_deref()
            .unwrap_or_default()
            .join("; "),
    ));
    new_tag.set_custom_text(
        DISCOGS_RELEASE_TAG.to_owned(),
        Some(discogs_release.uri.to_owned()),
    );

    Ok(new_tag)
}

#[allow(clippy::borrowed_box)]
pub fn strip_redundant_fields(tag: &Box<dyn Tag>) -> Result<Box<dyn Tag>> {
    let mut new_tag = tag.clone();
    new_tag.clear();

    for frame_id in ALLOWED_FRAMES.iter() {
        new_tag.set_frame(frame_id, tag.frame_content(frame_id))?;
    }

    Ok(new_tag)
}

const DISCOGS_RELEASE_TAG: &str = "DISCOGS_RELEASE";
static ALLOWED_FRAMES: Lazy<Vec<FrameId>> = Lazy::new(|| {
    vec![
        FrameId::Title,
        FrameId::Album,
        FrameId::AlbumArtist,
        FrameId::Artist,
        FrameId::Year,
        FrameId::Track,
        FrameId::TotalTracks,
        FrameId::Disc,
        FrameId::TotalDiscs,
        FrameId::Genre,
        FrameId::CustomText {
            key: DISCOGS_RELEASE_TAG.to_string(),
        },
    ]
});
