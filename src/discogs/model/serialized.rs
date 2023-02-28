use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct DiscogsMaster {
    pub main_release_url: String,
}

#[derive(Serialize, Deserialize)]
pub struct DiscogsRelease {
    pub title: String,
    pub uri: String,
    pub images: Option<Vec<DiscogsImage>>,
    pub tracklist: Vec<DiscogsTrack>,
    pub artists: Vec<DiscogsArtist>,
    pub year: i32,
    pub styles: Option<Vec<String>>,
    pub format_quantity: Option<u32>,
}

#[derive(Serialize, Deserialize)]
pub struct DiscogsTrack {
    pub title: String,
    pub type_: String,
    pub artists: Option<Vec<DiscogsArtist>>,
    pub position: Option<String>,
    pub sub_tracks: Option<Vec<DiscogsTrack>>,
    pub duration: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct DiscogsArtist {
    pub name: String,
    pub join: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DiscogsImage {
    pub resource_url: String,
    #[serde(alias = "type")] pub type_: String,
}

#[derive(Serialize, Deserialize)]
pub struct DiscogsSearchResultPage {
    pub results: Vec<DiscogsSearchResult>,
}

#[derive(Serialize, Deserialize)]
pub struct DiscogsSearchResult {
    pub resource_url: String,
}
