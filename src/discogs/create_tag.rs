use std::ops::Deref;

use anyhow::Result;
use dyn_clone::clone_box;
use itertools::Itertools;

use crate::discogs::model::{DiscogsRelease, DiscogsTrack};
use crate::tag::Tag;

#[allow(clippy::borrowed_box)] // FIXME: Fix reference to Box
pub fn create_tag_from_discogs_data(
    original_tag: &Box<dyn Tag>, // FIXME: Can't create new tag without "template" for now
    position: u32,
    discogs_track: &DiscogsTrack,
    discogs_release: &DiscogsRelease,
) -> Result<Box<dyn Tag>> {
    let album_artists: Vec<(&str, &str)> = discogs_release.artists
        .iter()
        .map(|artist| (
            artist.proper_name(),
            artist.join.as_deref().unwrap_or("&")
        ))
        .collect_vec();
    let track_artists: Option<Vec<(&str, &str)>> = discogs_track.artists
        .as_ref()
        .map(|artists| {
            artists
                .iter()
                .map(|artist| (
                    artist.proper_name(),
                    artist.join.as_deref().unwrap_or("&")
                ))
                .collect_vec()
        });

    let mut new_tag = clone_box(original_tag.deref());

    new_tag.clear();
    new_tag.set_title(Some(discogs_track.proper_title().to_owned()));
    new_tag.set_album(Some(discogs_release.proper_title().to_owned()));
    new_tag.set_album_artist(Some(
        if track_artists.is_some() {
            "Various Artists".to_owned()
        } else {
            album_artists
                .iter()
                .flat_map(|v| [v.0, (v.1)])
                .collect::<Vec<&str>>()
                .join(" ")
                .trim()
                .to_owned()
        }
    ));
    new_tag.set_artist(Some(
        track_artists
            .unwrap_or(album_artists)
            .iter()
            .flat_map(|v| [v.0, (v.1)])
            .collect_vec()
            .join(" ")
            .trim()
            .to_owned()
    ));
    new_tag.set_year(Some(discogs_release.year as i32));
    new_tag.set_track_number(Some(position));
    new_tag.set_total_tracks(Some(discogs_release.valid_track_list().len() as u32));
    new_tag.set_genre(Some(discogs_release.styles.join("; ")));
    new_tag.set_custom_text(DISCOGS_RELEASE_TAG.to_owned(), Some(discogs_release.uri.to_owned()));

    Ok(new_tag)
}

const DISCOGS_RELEASE_TAG: &str = "DISCOGS_RELEASE";
