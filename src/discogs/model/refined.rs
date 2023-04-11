use std::collections::HashMap;
use std::iter;
use std::time::Duration;

use anyhow::Result;
use itertools::Itertools;
use regex::Regex;

use crate::console_print;
use crate::discogs::model::serialized;
use crate::util::console_styleable::ConsoleStyleable;

#[derive(Clone)]
pub struct DiscogsRelease {
    pub uri: String,
    pub title: String,
    pub year: i32,
    pub styles: Option<Vec<String>>,
    pub image: Option<DiscogsImage>,
    pub tracks: Vec<DiscogsTrack>,
    pub disc_to_total_tracks: HashMap<u32, u32>,
    pub artists: Vec<DiscogsArtist>,
}

#[derive(Clone)]
pub struct DiscogsImage {
    pub url: String,
}

#[derive(Clone)]
pub struct DiscogsTrack {
    pub title: String,
    pub position: u32,
    pub disc: u32,
    pub duration: Option<Duration>,
    pub artists: Option<Vec<DiscogsArtist>>,
}

#[derive(Clone)]
pub struct DiscogsArtist {
    pub name: String,
    pub join: Option<String>,
}

impl DiscogsRelease {
    pub fn from(serialized: &serialized::DiscogsRelease) -> Result<DiscogsRelease> {
        let tracks = Self::tracks(serialized)?;
        let disc_to_total_tracks = Self::disc_to_total_tracks(&tracks);
        Ok(DiscogsRelease {
            uri: serialized.uri.clone(),
            title: Self::title(serialized),
            year: serialized.year,
            styles: serialized.styles.clone(),
            image: Self::image(serialized),
            tracks,
            disc_to_total_tracks,
            artists: serialized
                .artists
                .iter()
                .map(DiscogsArtist::from)
                .collect_vec(),
        })
    }

    fn title(serialized: &serialized::DiscogsRelease) -> String {
        return serialized.title.trim().to_owned();
    }

    fn image(serialized: &serialized::DiscogsRelease) -> Option<DiscogsImage> {
        let images = serialized.images.iter().flatten();
        images
            .clone()
            .find(|v| v.type_ == "primary")
            .or_else(|| images.clone().find(|v| v.type_ == "secondary"))
            .map(DiscogsImage::from)
    }

    fn tracks(serialized: &serialized::DiscogsRelease) -> Result<Vec<DiscogsTrack>> {
        const DEFAULT_DISC: u32 = 1;

        let serialized_tracks = Self::extract_track_list(&serialized.tracklist).collect_vec();

        let mut refined_tracks = Vec::new();

        let mut track_index_position = 0u32;
        let mut used_indexing = false;
        let mut used_parsed_position = false;
        for serialized_track in serialized_tracks {
            let (disc, position) = if let Some((disc, position)) =
                DiscogsTrack::disc_position(serialized_track).ok().flatten()
            {
                if used_indexing {
                    console_print!(
                        "{}",
                        "Tried to use parsed position while used indexing already".warning_styled()
                    )
                } else {
                    used_parsed_position = true;
                }
                if let Some(disc) = disc {
                    (disc, position)
                } else {
                    (DEFAULT_DISC, position)
                }
            } else {
                if used_parsed_position {
                    console_print!(
                        "{}",
                        "Tried to use indexing while used parsed position already".warning_styled()
                    )
                } else {
                    used_indexing = true;
                }
                track_index_position += 1;
                (DEFAULT_DISC, track_index_position)
            };
            refined_tracks.push(DiscogsTrack::from(serialized_track, position, disc)?)
        }

        Ok(refined_tracks)
    }

    fn extract_track_list<'a, It>(
        track_iterator: It,
    ) -> Box<dyn Iterator<Item = &'a serialized::DiscogsTrack> + 'a>
    where
        It: IntoIterator<Item = &'a serialized::DiscogsTrack> + 'a,
    {
        Box::new(
            track_iterator
                .into_iter()
                .flat_map(|v| {
                    iter::once(v).chain(Self::extract_track_list(v.sub_tracks.iter().flatten()))
                })
                .filter(|v| v.type_ == "track"),
        )
    }

    fn disc_to_total_tracks(tracks: &Vec<DiscogsTrack>) -> HashMap<u32, u32> {
        let mut result = HashMap::new();
        for track in tracks {
            *result.entry(track.disc).or_default() += 1;
        }
        result
    }
}

impl DiscogsImage {
    fn from(serialized: &serialized::DiscogsImage) -> DiscogsImage {
        DiscogsImage {
            url: serialized.resource_url.clone(),
        }
    }
}

impl DiscogsTrack {
    fn from(
        serialized: &serialized::DiscogsTrack,
        position: u32,
        disc: u32,
    ) -> Result<DiscogsTrack> {
        Ok(DiscogsTrack {
            title: Self::title(serialized),
            position,
            disc,
            duration: Self::duration(serialized)?,
            artists: serialized
                .artists
                .as_ref()
                .map(|v| v.iter().map(DiscogsArtist::from).collect_vec()),
        })
    }

    fn title(serialized: &serialized::DiscogsTrack) -> String {
        return serialized.title.trim().to_owned();
    }

    fn duration(serialized: &serialized::DiscogsTrack) -> Result<Option<Duration>> {
        serialized
            .duration
            .as_ref()
            .filter(|v| !v.is_empty())
            .map(|v| -> Result<_> {
                let parts = v.split(':').rev().collect::<Vec<_>>();
                let mut seconds = 0u64;
                let mut multiplier = 1u64;
                for part in parts {
                    seconds += part.parse::<u64>()? * multiplier;
                    multiplier *= 60
                }
                Ok(Duration::from_secs(seconds))
            })
            .transpose()
    }

    fn disc_position(serialized: &serialized::DiscogsTrack) -> Result<Option<(Option<u32>, u32)>> {
        serialized
            .position
            .as_ref()
            .map(|position| {
                position
                    .split('-')
                    .next_tuple::<(&str, &str)>()
                    .map(|(a, b)| (Some(a.to_string()), b.to_string()))
                    .unwrap_or_else(|| (None, position.to_string()))
            })
            .map(|(disc, position)| -> Result<_> {
                Ok((
                    disc.map(|v| v.parse::<u32>()).transpose()?,
                    position.parse::<u32>()?,
                ))
            })
            .transpose()
    }
}

impl DiscogsArtist {
    fn from(serialized: &serialized::DiscogsArtist) -> DiscogsArtist {
        DiscogsArtist {
            name: Self::name(serialized),
            join: serialized.join.clone(),
        }
    }

    fn name(serialized: &serialized::DiscogsArtist) -> String {
        let name = &serialized.name;
        #[allow(clippy::unwrap_used)]
        let regex = Regex::new(r".*( \(\d+\))").unwrap();
        match regex.captures(name) {
            Some(captures) => {
                #[allow(clippy::unwrap_used)]
                let range = captures.get(1).unwrap().range();
                &name[..range.start]
            }
            None => name,
        }
        .trim()
        .to_owned()
    }
}
