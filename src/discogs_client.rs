use std::{cmp, thread, time};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dialoguer::Input;
use indicatif::ProgressBar;
use itertools::Itertools;
use progress_streams::ProgressWriter;
use reqwest::{blocking, IntoUrl, StatusCode, Url};
use reqwest::blocking::Response;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};

use crate::{Console, console_print, MusicFile};

pub struct DiscogsClient {
    client: blocking::Client,
}

pub struct DiscogsReleaseInfo {
    pub json: serde_json::Value,
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

            let discogs_info = if artists.is_empty() || albums.len() != 1 {
                self.fetch_by_release_id(
                    &ask_discogs_release_id(
                        &format!("Can't find release for \"{}\"", path.display()).as_str()),
                    console,
                )
            } else {
                let album = &albums[0];
                self.fetch_by_meta(&artists, &album, console)
                    .or_else(|| {
                        self.fetch_by_release_id(
                            &ask_discogs_release_id(
                                &format!("Can't find release for \"{} - {}\"", artists.join(", "), album)),
                            console,
                        )
                    })
            }.unwrap();

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
        console: &Console,
    ) -> Option<DiscogsReleaseInfo> {
        console_print!(console, "Searching Discogs for \"{} - {}\"", &artists.join(", "), album);

        let artist_param = artists.join(" ");
        let query_param = format!("{} - {}", &artists.join(", "), &album);

        let search_params_tries = vec![
            vec![
                ("type", "master"),
                ("artist", &artist_param),
                ("release_title", &album),
            ],
            vec![
                ("type", "release"),
                ("artist", &artist_param),
                ("release_title", &album),
            ],
            vec![
                ("type", "release"),
                ("query", &query_param),
            ],
        ];

        let release_url = search_params_tries.iter()
            .filter_map(|search_params| {
                let search_url = Url::parse_with_params(
                    "https://api.discogs.com/database/search", search_params).unwrap();

                console_print!(console, "Fetching {}", search_url);

                self
                    .safe_get(search_url)
                    .json::<serde_json::Value>()
                    .unwrap()
                    ["results"][0]["resource_url"]
                    .as_str()
                    .map(ToOwned::to_owned)
            })
            .find_map(Option::Some)?;

        self.fetch_by_url(&release_url, console)
    }

    pub fn fetch_by_release_id(&self, release_id: &str, console: &Console) -> Option<DiscogsReleaseInfo> {
        self.fetch_by_url(&format!("https://api.discogs.com/releases/{}", release_id), console)
    }

    pub fn download_cover(
        &self,
        uri: &str,
        path: &Path,
        pb: &ProgressBar,
    ) {
        let mut response = self.safe_get(uri);

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
        console_print!(console, "Fetching {}", release_url);

        let release_object = self
            .safe_get(release_url)
            .json::<serde_json::Value>()
            .unwrap()
            .clone();

        console_print!(console, "Will use {}", release_object["uri"].as_str().unwrap());

        Some(DiscogsReleaseInfo {
            json: release_object
        })
    }

    fn safe_get<T: IntoUrl + Clone>(&self, url: T) -> Response {
        loop {
            let response = self.client.get(url.clone()).send().unwrap();
            let status = response.status();
            if status.is_success() {
                break response;
            } else if status == StatusCode::TOO_MANY_REQUESTS {
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
