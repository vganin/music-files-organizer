use std::{cmp, thread, time};
use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};

use dialoguer::Input;
use dyn_clone::clone_box;
use indicatif::ProgressBar;
use itertools::Itertools;
use progress_streams::ProgressWriter;
use regex::Regex;
use reqwest::{blocking, IntoUrl, StatusCode, Url};
use reqwest::blocking::Response;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};

use crate::{Console, console_print, Tag};
use crate::command::import::MusicFile;
use crate::util::console_styleable::ConsoleStyleable;

pub struct DiscogsClient {
    client: blocking::Client,
}

pub struct DiscogsReleaseInfo {
    json: serde_json::Value,
}

pub struct DiscogsRelease {
    pub music_files: Vec<MusicFile>,
    pub discogs_info: DiscogsReleaseInfo,
}

impl DiscogsClient {
    pub fn new(discogs_token: &str) -> Self {
        DiscogsClient {
            client: blocking::ClientBuilder::new()
                .default_headers(common_headers(&discogs_token))
                .build()
                .unwrap(),
        }
    }

    pub fn fetch_by_music_files(&self, music_files: Vec<MusicFile>, console: &Console) -> Vec<DiscogsRelease> {
        let mut files_grouped_by_parent_path: HashMap<PathBuf, Vec<MusicFile>> = HashMap::new();

        for music_file in music_files {
            let parent_path = music_file.file_path.parent().unwrap().to_owned();
            files_grouped_by_parent_path.entry(parent_path).or_default().push(music_file);
        }

        let mut result = Vec::new();

        for (path, music_files) in files_grouped_by_parent_path {
            let artists: Vec<String> = music_files.iter()
                .filter_map(|v| v.tag.artist().map(ToString::to_string))
                .unique()
                .collect();
            let albums: Vec<String> = music_files.iter()
                .filter_map(|v| v.tag.album().map(ToString::to_string))
                .unique()
                .collect();
            let tracks: Vec<String> = music_files.iter()
                .filter_map(|v| v.tag.title().map(ToString::to_string))
                .unique()
                .collect();

            let discogs_info = if artists.is_empty() || albums.len() != 1 || tracks.is_empty() {
                self.fetch_by_release_id(
                    &ask_discogs_release_id(
                        &format!("Can't find release for {}", path.display().path_styled())),
                    console,
                )
            } else {
                let album = &albums[0];
                let track = &tracks[0];
                self.fetch_by_meta(&artists, &album, track, tracks.len(), console)
                    .or_else(|| {
                        self.fetch_by_release_id(
                            &ask_discogs_release_id(
                                &format!(
                                    "Can't find release for {} - {}",
                                    artists.join(", ").tag_styled(),
                                    album.tag_styled()
                                )
                            ),
                            console,
                        )
                    })
            }.unwrap();

            console_print!(console, "Will use {}", discogs_info.json["uri"].as_str().unwrap().path_styled());

            result.push(DiscogsRelease {
                music_files,
                discogs_info,
            });
        }

        result
    }

    pub fn fetch_by_meta(
        &self,
        artists: &[String],
        album: &str,
        track: &str,
        track_count: usize,
        console: &Console,
    ) -> Option<DiscogsReleaseInfo> {
        console_print!(
            console,
            "Searching Discogs for {} :: {} :: {}",
            &artists.join(", ").tag_styled(),
            album.tag_styled(),
            track.tag_styled(),
        );

        let artist_param = artists.join(" ");

        let release_urls_from_master_search = self.search_with_params(&[
            ("type", "master".to_owned()),
            ("artist", artist_param.to_owned()),
            ("release_title", album.to_owned()),
            ("track", track.to_owned()),
        ], console)
            .into_iter()
            .flat_map(|json| {
                json["results"]
                    .as_array()
                    .unwrap()
                    .to_owned()
            })
            .map(|json| {
                json["resource_url"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .filter_map(|master_url| {
                Some(self.get_ok(Url::parse(&master_url).unwrap(), console)?
                    .json::<serde_json::Value>()
                    .unwrap()
                    ["main_release_url"]
                    .as_str()
                    .unwrap()
                    .to_owned())
            });

        let release_urls_from_release_search = [
            vec![
                ("type", "release".to_owned()),
                ("artist", artists.join(" ")),
                ("release_title", album.to_owned()),
                ("track", track.to_owned()),
            ],
            vec![
                ("type", "release".to_owned()),
                ("query", format!("{} - {}", &artists.join(", "), &album)),
            ],
        ].to_owned()
            .into_iter()
            .flat_map(|search_params| {
                self.search_with_params(&search_params, console).into_iter()
            })
            .flat_map(|json| {
                json["results"]
                    .as_array()
                    .unwrap()
                    .to_owned()
            })
            .map(|json| {
                json["resource_url"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            });

        let release_object = release_urls_from_master_search
            .chain(release_urls_from_release_search)
            .filter_map(|release_url| {
                self.fetch_by_url(&release_url, console)
                    .filter(|info| track_list(&info.json).len() == track_count)
            })
            .find_map(Some);

        release_object
    }

    fn search_with_params(&self, params: &[(&str, String)], console: &Console) -> Option<serde_json::Value> {
        let search_url = Url::parse_with_params("https://api.discogs.com/database/search", params).unwrap();

        self
            .get_ok(search_url, console)?
            .json::<serde_json::Value>()
            .ok()
    }

    pub fn fetch_by_release_id(&self, release_id: &str, console: &Console) -> Option<DiscogsReleaseInfo> {
        self.fetch_by_url(&format!("https://api.discogs.com/releases/{}", release_id), console)
    }

    pub fn download_cover(
        &self,
        uri: &str,
        path: &Path,
        pb: &ProgressBar,
        console: &Console,
    ) {
        let mut response = self.get_ok(uri, console).unwrap();

        let mut file = &mut ProgressWriter::new(
            std::fs::File::create(&path).unwrap(),
            |bytes| pb.inc(bytes as u64),
        );

        pb.set_length(response.content_length().unwrap());
        pb.set_position(0);

        response
            .copy_to(&mut file)
            .unwrap();
    }

    fn fetch_by_url(
        &self,
        release_url: &str,
        console: &Console,
    ) -> Option<DiscogsReleaseInfo> {
        let release_object = self
            .get_ok(release_url, console)?
            .json::<serde_json::Value>()
            .unwrap();

        Some(DiscogsReleaseInfo {
            json: release_object
        })
    }

    fn get_ok<T: IntoUrl + Clone + Display>(&self, url: T, console: &Console) -> Option<Response> {
        console_print!(console, "Fetching {}", (&url).path_styled());
        loop {
            let response = self.client.get(url.clone()).send().unwrap();
            let status = response.status();
            if status.is_success() {
                break Some(response);
            } else if status == StatusCode::NOT_FOUND {
                break None;
            } else if status == StatusCode::TOO_MANY_REQUESTS {
                console_print!(console, "{}", "Reached requests limit! Slowing down...".styled().bold().yellow());
                let header_as_number = |str| response.headers().get(str).unwrap().to_str().unwrap().parse::<f64>().unwrap();
                let rate_limit = header_as_number("X-Discogs-Ratelimit");
                let rate_limit_used = header_as_number("X-Discogs-Ratelimit-Used");
                let skip = cmp::min_by(
                    rate_limit_used - rate_limit,
                    0f64,
                    |lhs, rhs| lhs.partial_cmp(rhs).unwrap(),
                ) + 1f64;
                thread::sleep(time::Duration::from_secs_f64(skip * 60f64 / rate_limit));
            } else {
                panic!("Expected successful status code but got {}", status)
            }
        }
    }
}

pub fn tag_from_discogs_info(original_tag: &Box<dyn Tag>, info: &DiscogsReleaseInfo) -> Box<dyn Tag> {
    let release = &info.json;
    let track_number = original_tag.track().unwrap();
    let track_list = track_list(&info.json);
    let track = track_list[track_number as usize - 1];
    let album_artists = release["artists"].as_array().unwrap().iter()
        .map(|v| (
            fix_discogs_artist_name(v["name"].as_str().unwrap().trim()),
            v["join"].as_str().unwrap_or("&")
        ))
        .collect::<Vec<(&str, &str)>>();
    let track_artists = track["artists"].as_array()
        .map(|array| {
            array.iter()
                .map(|v| (
                    fix_discogs_artist_name(v["name"].as_str().unwrap().trim()),
                    v["join"].as_str().unwrap_or("&")
                ))
                .collect::<Vec<(&str, &str)>>()
        });

    let mut new_tag = clone_box(&**original_tag);

    new_tag.clear();
    new_tag.set_title(Some(track["title"].as_str().unwrap().trim().to_owned()));
    new_tag.set_album(Some(release["title"].as_str().unwrap().trim().to_owned()));
    new_tag.set_album_artist(Some(
        if track_artists.is_some() {
            "Various Artists".to_owned()
        } else {
            album_artists
                .iter()
                .flat_map(|v| [v.0, v.1])
                .collect::<Vec<&str>>()
                .join(" ")
                .trim()
                .to_owned()
        }
    ));
    new_tag.set_artist(Some(
        track_artists
            .or(Some(album_artists))
            .unwrap()
            .iter()
            .flat_map(|v| [v.0, v.1])
            .collect::<Vec<&str>>()
            .join(" ")
            .trim()
            .to_owned()
    ));
    new_tag.set_year(Some(release["year"].as_i64().unwrap() as i32));
    new_tag.set_track(Some(track_number));
    new_tag.set_total_tracks(Some(track_list.len() as u32));
    new_tag.set_genre(Some(
        release["styles"].as_array().unwrap_or(&vec![]).iter()
            .map(|v| v.as_str().unwrap().trim())
            .collect::<Vec<&str>>()
            .join("; ")
    ));
    new_tag.set_custom_text(DISCOGS_RELEASE_TAG.to_owned(), Some(release["uri"].as_str().unwrap().to_owned()));
    new_tag
}

pub fn cover_uri_from_discogs_info(info: &DiscogsReleaseInfo) -> Option<&str> {
    let images_array = info.json["images"].as_array()?;
    images_array.iter()
        .find(|v| v["type"].as_str().unwrap() == "primary")
        .map(|v| v["uri"].as_str().unwrap())
        .or_else(|| {
            images_array.iter()
                .find(|v| v["type"].as_str().unwrap() == "secondary")
                .map(|v| v["uri"].as_str().unwrap())
        })
}

fn fix_discogs_artist_name(name: &str) -> &str {
    let regex = Regex::new(r".*( \(\d+\))").unwrap();
    match regex.captures(name) {
        Some(captures) => {
            let range = captures.get(1).unwrap().range();
            &name[..range.start]
        }
        None => name
    }
}

fn ask_discogs_release_id(reason: &str) -> String {
    Input::new()
        .with_prompt(format!("{}. Please enter Discogs release ID", reason))
        .interact_text()
        .unwrap()
}

fn common_headers(discogs_token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::try_from(
        format!("{name}/{version} +{site}",
                name = env!("CARGO_PKG_NAME"),
                version = env!("CARGO_PKG_VERSION"),
                site = "https://github.com/vganin/orgtag"
        )
    ).unwrap());
    headers.insert(AUTHORIZATION, HeaderValue::try_from(format!("Discogs token={}", discogs_token)).unwrap());
    headers
}

fn track_list(release_object: &serde_json::Value) -> Vec<&serde_json::Value> {
    release_object["tracklist"].as_array().unwrap().iter()
        .filter(|v| v["type_"].as_str().unwrap() == "track")
        .collect::<Vec<&serde_json::Value>>()
}

const DISCOGS_RELEASE_TAG: &str = "DISCOGS_RELEASE";
