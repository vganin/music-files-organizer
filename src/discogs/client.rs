use std::{f64, thread, time};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::Display;
use std::path::Path;

use anyhow::{bail, Context, Result};
use dialoguer::Input;
use indicatif::ProgressBar;
use itertools::Itertools;
use progress_streams::ProgressWriter;
use reqwest::{blocking, IntoUrl, StatusCode, Url};
use reqwest::blocking::Response;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::de::DeserializeOwned;

use crate::command::import::MusicFile;
use crate::console_print;
use crate::discogs::model::{DiscogsMaster, DiscogsRelease, DiscogsSearchResultPage};
use crate::util::console::Console;
use crate::util::console_styleable::ConsoleStyleable;
use crate::util::path_extensions::PathExtensions;

pub struct DiscogsClient {
    client: blocking::Client,
}

impl DiscogsClient {
    pub fn new(discogs_token: &str) -> Result<Self> {
        Ok(DiscogsClient {
            client: blocking::ClientBuilder::new()
                .default_headers(Self::common_headers(discogs_token)?)
                .build()?
        })
    }

    fn common_headers(discogs_token: &str) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::try_from(
                format!("{name}/{version} +{site}",
                        name = env!("CARGO_PKG_NAME"),
                        version = env!("CARGO_PKG_VERSION"),
                        site = "https://github.com/vganin/music-files-organizer"
                )
            )?,
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::try_from(format!("Discogs token={}", discogs_token))?,
        );
        Ok(headers)
    }
}

pub struct MusicFilesToDiscogsRelease<'a> {
    pub music_files: Vec<&'a MusicFile>,
    pub discogs_release: DiscogsRelease,
}

impl DiscogsClient {
    pub fn group_music_files_with_discogs_data<'a>(
        &self,
        music_files: &'a Vec<MusicFile>,
        console: &Console,
    ) -> Result<Vec<MusicFilesToDiscogsRelease<'a>>> {
        let mut files_grouped_by_parent_path: HashMap<&Path, Vec<&MusicFile>> = HashMap::new();

        for music_file in music_files {
            let parent_path = music_file.file_path.parent_or_empty();
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

            let discogs_release: DiscogsRelease = if let (Some(first_album), Some(first_track)) = (albums.first(), tracks.first()) {
                if let Some(discogs_release) = self.fetch_release_by_meta(&artists, first_album, first_track, tracks.len(), console)? {
                    discogs_release
                } else {
                    let discogs_release_id = ask_discogs_release_id(
                        &format!(
                            "Can't find release for {} - {}",
                            artists.join(", ").tag_styled(),
                            first_album.tag_styled()
                        )
                    )?;
                    self.fetch_release_by_id(&discogs_release_id, console)?
                }
            } else {
                let discogs_release_id = ask_discogs_release_id(
                    &format!("Can't find release for {}", path.display().path_styled())
                )?;
                self.fetch_release_by_id(&discogs_release_id, console)?
            };

            console_print!(console, "Will use {}", discogs_release.uri.as_str().path_styled());

            result.push(MusicFilesToDiscogsRelease {
                music_files,
                discogs_release,
            });
        }

        Ok(result)
    }

    pub fn download_cover(
        &self,
        url: &str,
        path: &Path,
        pb: &ProgressBar,
        console: &Console,
    ) -> Result<()> {
        let mut response = self.get_ok(url, console)?;

        let mut file = &mut ProgressWriter::new(
            std::fs::File::create(path)?,
            |bytes| pb.inc(bytes as u64),
        );

        pb.set_length(response.content_length().context("Failed to get content length")?);
        pb.set_position(0);

        response.copy_to(&mut file)?;

        Ok(())
    }

    pub fn fetch_release_by_meta(
        &self,
        artists: &[String],
        album: &str,
        track: &str,
        track_count: usize,
        console: &Console,
    ) -> Result<Option<DiscogsRelease>> {
        console_print!(
            console,
            "Searching Discogs for {} – {}",
            &artists.join(", ").tag_styled(),
            album.tag_styled(),
        );

        let urls_from_master_search = || self.fetch_release_urls_from_master_search(artists, album, track, console);
        let urls_from_release_search = || self.fetch_release_urls_from_release_search(artists, album, track, console);

        let fetch_funcs: [&dyn Fn() -> Result<Vec<String>>; 2] = [
            &urls_from_master_search,
            &urls_from_release_search
        ];

        let release_urls = fetch_funcs
            .iter()
            .map(|fetch| fetch())
            .map_ok(|v| v)
            .flatten_ok();

        for release_url in release_urls {
            let release: DiscogsRelease = self.fetch_by_url(release_url?, console)?;
            if release.valid_track_list().len() == track_count {
                return Ok(Some(release));
            }
        }

        Ok(None)
    }

    fn fetch_release_urls_from_master_search(
        &self,
        artists: &[String],
        album: &str,
        track: &str,
        console: &Console,
    ) -> Result<Vec<String>> {
        let search_master_results = self.fetch_search_results(
            [
                ("type", "master"),
                ("artist", &artists.join(" ")),
                ("release_title", album),
                ("track", track),
            ],
            console,
        )?;

        let mut result = vec![];

        for search_master_result in search_master_results.results {
            let master: DiscogsMaster = self.fetch_by_url(&search_master_result.resource_url, console)?;
            result.push(master.main_release_url);
        }

        Ok(result)
    }

    fn fetch_release_urls_from_release_search(
        &self,
        artists: &[String],
        album: &str,
        track: &str,
        console: &Console,
    ) -> Result<Vec<String>> {
        let search_variants = [
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
        ];

        let mut result = vec![];

        for search_variant in search_variants {
            let search_release_results = self.fetch_search_results(search_variant, console)?;

            for search_release_result in search_release_results.results {
                result.push(search_release_result.resource_url)
            }
        }

        Ok(result)
    }

    fn fetch_release_by_id(&self, release_id: &str, console: &Console) -> Result<DiscogsRelease> {
        let url = &format!("https://api.discogs.com/releases/{}", release_id);
        self.fetch_by_url(url, console)
    }

    fn fetch_search_results<I, K, V>(&self, params: I, console: &Console) -> Result<DiscogsSearchResultPage>
        where
            I: IntoIterator,
            I::Item: Borrow<(K, V)>,
            K: AsRef<str>,
            V: AsRef<str>,
    {
        let url = Url::parse_with_params("https://api.discogs.com/database/search", params)?;
        self.fetch_by_url(url, console)
    }

    fn fetch_by_url<U, T>(
        &self,
        url: U,
        console: &Console,
    ) -> Result<T>
        where U: IntoUrl + Clone + Display,
              T: DeserializeOwned
    {
        Ok(
            serde_json::from_value(
                self
                    .get_ok(url, console)?
                    .json::<serde_json::Value>()?
            )?
        )
    }

    fn get_ok<T: IntoUrl + Clone + Display>(&self, url: T, console: &Console) -> Result<Response> {
        console_print!(console, "Fetching {}", (&url).path_styled());
        loop {
            let response = self.client.get(url.clone()).send()?;
            let status = response.status();
            if status.is_success() {
                break Ok(response);
            } else if status == StatusCode::TOO_MANY_REQUESTS {
                console_print!(console, "{}", "Reached requests limit! Slowing down...".styled().bold().yellow());
                let header_as_number = |header| -> Result<f64> {
                    response.headers()
                        .get(header)
                        .map(|v| -> Result<f64> {
                            Ok(v.to_str()?.parse::<f64>()?)
                        })
                        .with_context(|| format!("No required header: {}", header))?
                };
                let rate_limit = header_as_number("X-Discogs-Ratelimit")?;
                let rate_limit_used = header_as_number("X-Discogs-Ratelimit-Used")?;
                let skip = f64::min(rate_limit_used - rate_limit, 0f64) + 1f64;
                thread::sleep(time::Duration::from_secs_f64(skip * 60f64 / rate_limit));
            } else {
                bail!("Expected successful status code but got {}", status)
            }
        }
    }
}

fn ask_discogs_release_id(reason: &str) -> Result<String> {
    Input::new()
        .with_prompt(format!("{}. Please enter Discogs release ID", reason))
        .interact_text()
        .context("Failed to interact")
}
