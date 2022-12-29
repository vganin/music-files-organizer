use std::ops::Deref;

use anyhow::{bail, Context, Result};
use dyn_clone::clone_box;
use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use itertools::Itertools;
use unidecode::unidecode;

use crate::discogs::model::{DiscogsRelease, DiscogsTrack};
use crate::tag::Tag;

impl DiscogsRelease {
    #[allow(clippy::borrowed_box)] // FIXME: Fix reference to Box
    pub fn to_tag(&self, original_tag: &Box<dyn Tag>) -> Result<Box<dyn Tag>> {
        let track_list = self.valid_track_list();
        let (track_number, track) = self.find_track_from(original_tag)?;
        let album_artists: Vec<(&str, &str)> = self.artists
            .iter()
            .map(|artist| (
                artist.proper_name(),
                artist.join.as_deref().unwrap_or("&")
            ))
            .collect_vec();
        let track_artists: Option<Vec<(&str, &str)>> = track.artists
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
        new_tag.set_title(Some(track.proper_title().to_owned()));
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

    #[allow(clippy::borrowed_box)] // FIXME: Fix reference to Box
    fn find_track_from(&self, original_tag: &Box<dyn Tag>) -> Result<(u32, &DiscogsTrack)> {
        let track_list = self.valid_track_list();

        if let Some(track_number) = original_tag.track_number() {
            if let Some(track) = track_list.get(track_number as usize - 1) {
                return Ok((track_number, track));
            }
        }

        let matcher = SkimMatcherV2::default();
        let pattern = original_tag.title()
            .with_context(|| format!("Track contains no title: {:?}", original_tag))?
            .simplify();

        for (track_index, track) in track_list.iter().enumerate() {
            if matcher.fuzzy_match(&track.title.simplify(), &pattern).is_some() {
                let position = &track.position;
                let position = position.parse::<u32>().unwrap_or(track_index as u32 + 1);
                return Ok((position, track));
            }
        }

        bail!(
            "Failed to find track with '{}' and with {} from track list: {:?}",
            original_tag.title().unwrap_or("no title"),
            original_tag.track_number().map(|v| format!("position {}", v)).as_deref().unwrap_or("with no track number"),
            track_list
        )
    }
}

trait StringExtension {
    fn simplify(&self) -> String;
}

impl StringExtension for str {
    fn simplify(&self) -> String {
        unidecode(self).to_lowercase()
    }
}

const DISCOGS_RELEASE_TAG: &str = "DISCOGS_RELEASE";
