use std::iter;

use itertools::Itertools;
use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscogsRelease {
    pub title: String,
    pub uri: String,
    pub images: Option<Vec<DiscogsImage>>,
    #[serde(alias = "tracklist")] pub track_list: Vec<DiscogsTrack>,
    pub artists: Vec<DiscogsArtist>,
    pub year: i64,
    pub styles: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscogsMaster {
    pub main_release_url: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscogsTrack {
    pub title: String,
    // It's literally "type_" in format
    pub type_: String,
    pub artists: Option<Vec<DiscogsArtist>>,
    pub position: Option<String>,
    pub sub_tracks: Option<Vec<DiscogsTrack>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscogsArtist {
    pub name: String,
    pub join: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscogsImage {
    pub resource_url: String,
    #[serde(alias = "type")] pub type_: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscogsSearchResultPage {
    pub results: Vec<DiscogsSearchResult>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DiscogsSearchResult {
    pub resource_url: String,
}

impl DiscogsRelease {
    pub fn proper_title(&self) -> &str {
        self.title.trim()
    }

    pub fn valid_track_list(&self) -> Vec<&DiscogsTrack> {
        Self::_valid_track_list(&self.track_list).collect_vec()
    }

    fn _valid_track_list<'a, It>(track_iterator: It) -> Box<dyn Iterator<Item=&'a DiscogsTrack> + 'a>
        where It: IntoIterator<Item=&'a DiscogsTrack> + 'a,
    {
        Box::new(
            track_iterator
                .into_iter()
                .flat_map(|v| iter::once(v).chain(Self::_valid_track_list(v.sub_tracks.iter().flatten())))
                .filter(|v| v.type_ == "track")
        )
    }

    pub fn best_image(&self) -> Option<&DiscogsImage> {
        let images = self.images.iter().flatten();
        images.clone().find(|v| v.type_ == "primary")
            .or_else(|| images.clone().find(|v| v.type_ == "secondary"))
    }
}

impl DiscogsTrack {
    pub fn proper_title(&self) -> &str {
        self.title.trim()
    }
}

impl DiscogsArtist {
    pub fn proper_name(&self) -> &str {
        let name = &self.name;
        #[allow(clippy::unwrap_used)] let regex = Regex::new(r".*( \(\d+\))").unwrap();
        match regex.captures(name) {
            Some(captures) => {
                #[allow(clippy::unwrap_used)] let range = captures.get(1).unwrap().range();
                &name[..range.start]
            }
            None => name
        }.trim()
    }
}
