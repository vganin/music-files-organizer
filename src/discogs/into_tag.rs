use std::ops::Deref;

use anyhow::{Context, Result};
use dyn_clone::clone_box;
use itertools::Itertools;

use crate::discogs::model::DiscogsRelease;
use crate::tag::Tag;

impl DiscogsRelease {
    #[allow(clippy::borrowed_box)] // FIXME: Fix reference to Box
    pub fn to_tag(&self, original_tag: &Box<dyn Tag>) -> Result<Box<dyn Tag>> {
        let track_number = original_tag.track_number().context("No track number")?;
        let track_list = self.valid_track_list();
        let track_from_original_tag = track_list.get(track_number as usize - 1)
            .with_context(|| format!("Failed to find track {} from track list: {:?}", track_number, track_list))?;
        let album_artists: Vec<(&str, &str)> = self.artists
            .iter()
            .map(|artist| (
                artist.proper_name(),
                artist.join.as_deref().unwrap_or("&")
            ))
            .collect_vec();
        let track_artists: Option<Vec<(&str, &str)>> = track_from_original_tag.artists
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
        new_tag.set_title(Some(track_from_original_tag.proper_title().to_owned()));
        new_tag.set_album(Some(self.proper_title().to_owned()));
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
        new_tag.set_year(Some(self.year as i32));
        new_tag.set_track_number(Some(track_number));
        new_tag.set_total_tracks(Some(track_list.len() as u32));
        new_tag.set_genre(Some(self.styles.join("; ")));
        new_tag.set_custom_text(DISCOGS_RELEASE_TAG.to_owned(), Some(self.uri.to_owned()));

        Ok(new_tag)
    }
}

const DISCOGS_RELEASE_TAG: &str = "DISCOGS_RELEASE";
