use std::{f64, thread};
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::mem::swap;
use std::ops::Deref;
use std::path::Path;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use dialoguer::{Input, Select};
use indicatif::ProgressBar;
use itertools::Itertools;
use progress_streams::ProgressWriter;
use regex::Regex;
use reqwest::{blocking, IntoUrl, StatusCode, Url};
use reqwest::blocking::Response;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde::de::DeserializeOwned;

use DiscogsReleaseMatchResult::Matched;

use crate::console_print;
use crate::discogs::model::{DiscogsMaster, DiscogsRelease, DiscogsSearchResultPage, DiscogsTrack};
use crate::music_file::MusicFile;
use crate::util::console::Console;
use crate::util::console_styleable::ConsoleStyleable;
use crate::util::path_extensions::PathExtensions;
use crate::util::string_extensions::StringExtensions;

pub struct DiscogsMatcher {
    http_client: blocking::Client,
}

impl DiscogsMatcher {
    pub fn new(discogs_token: &str) -> Result<Self> {
        Ok(DiscogsMatcher {
            http_client: blocking::ClientBuilder::new()
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

pub struct DiscogsTrackMatch<'a> {
    pub music_file: &'a MusicFile,
    pub track: DiscogsTrack,
    pub position: u32,
}

pub enum DiscogsReleaseMatchResult<'a> {
    Matched {
        tracks_matching: Vec<DiscogsTrackMatch<'a>>,
        release: DiscogsRelease,
    },
    Unmatched(Vec<&'a MusicFile>),
}

impl DiscogsMatcher {
    pub fn match_music_files<'a>(
        &self,
        music_files: impl Iterator<Item=&'a MusicFile>,
        console: &Console,
    ) -> Result<Vec<DiscogsReleaseMatchResult<'a>>> {
        let mut files_grouped_by_parent_path: HashMap<&Path, Vec<&MusicFile>> = HashMap::new();
        for music_file in music_files {
            let parent_path = music_file.file_path.parent_or_empty();
            files_grouped_by_parent_path.entry(parent_path).or_default().push(music_file);
        }

        let mut result = Vec::new();

        for (path, music_files) in files_grouped_by_parent_path {
            console_print!(
                console,
                "Matching Discogs for {} – {}",
                music_files
                    .iter()
                    .filter_map(|v| v.tag.artist().map(ToString::to_string))
                    .unique()
                    .join(" & ")
                    .tag_styled(),
                music_files
                    .iter()
                    .filter_map(|v| v.tag.album().map(ToString::to_string))
                    .unique()
                    .join(", ")
                    .tag_styled(),
            );

            let common_search_params = Self::common_search_params_from_music_files(&music_files);
            let release_urls = common_search_params
                .iter()
                .flat_map(|params| {
                    self.search_master_release(params, console)
                        .chain(self.search_release(params, console))
                        .take(5) // No more than 5 release fetches per params combinations to give other combinations realistic chances
                });

            let mut match_result: DiscogsReleaseMatchResult = DiscogsReleaseMatchResult::Unmatched(music_files.clone());

            let mut checked_release_urls = HashSet::new();
            for release_url in release_urls {
                let release_url = release_url?;
                if checked_release_urls.contains(&release_url) {
                    continue;
                } else {
                    checked_release_urls.insert(release_url.clone());
                }

                let release: DiscogsRelease = self.fetch_by_url(release_url, console)?;
                let release_clone = release.clone();
                match Self::match_release_with_music_files_complex(release, &music_files) {
                    None => continue,
                    Some(tracks_matching) => {
                        match_result = Matched { tracks_matching, release: release_clone };
                        break;
                    }
                }
            }

            if let DiscogsReleaseMatchResult::Unmatched(_) = match_result {
                if let Some(release_id) = Self::ask_for_release_id(
                    &format!("Can't find release for {}", path.display().path_styled()))?
                {
                    let mut release_id = release_id;
                    loop {
                        let release = self.fetch_release_by_id(&release_id, console)?;
                        let release_clone = release.clone();
                        match Self::match_release_with_music_files_simple(release, &music_files) {
                            None => {
                                match Self::ask_for_release_id(
                                    &format!("Failed to match with ID {}", release_id).error_styled().to_string())?
                                {
                                    None => break,
                                    Some(new_release_id) => release_id = new_release_id
                                }
                            }
                            Some(tracks_matching) => {
                                match_result = Matched { tracks_matching, release: release_clone };
                                break;
                            }
                        }
                    }
                }
            }

            if let Matched { release, .. } = &match_result {
                console_print!(console, "Will use {}", release.uri.as_str().path_styled());
            } else {
                console_print!(console, "Will use file tags as is");
            }

            result.push(match_result);
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

    fn match_release_with_music_files_complex<'a>(
        release: DiscogsRelease,
        music_files: &Vec<&'a MusicFile>,
    ) -> Option<Vec<DiscogsTrackMatch<'a>>> {
        let track_list = release.valid_track_list();

        if track_list.is_empty() {
            return None;
        }

        let mut tracks_matching: Vec<DiscogsTrackMatch> = vec![];

        for music_file in music_files {
            let tag = &music_file.tag;
            let track_title = tag.title().or_else(|| music_file.file_path.file_stem().and_then(|v| v.to_str())).unwrap_or_default();
            let Some((index, track)) = track_list.iter().enumerate().find(|(index, track)| {
                let title_matched = || track_title.is_similar(&track.title);
                let duration_matched = || {
                    const DURATION_DIFF_THRESHOLD: Duration = Duration::from_secs(90);
                    let Some(mut duration1) = music_file.duration else { return false; };
                    let Ok(Some(mut duration2)) = track.parsed_duration() else { return false; };
                    if duration2 < duration1 { swap(&mut duration1, &mut duration2); };
                    duration2 - duration1 < DURATION_DIFF_THRESHOLD
                };
                let position_matched = || tag.track_number().map(|v| &((v - 1) as usize) == index).unwrap_or(true);
                (title_matched() || duration_matched()) && position_matched()
            }) else { return None; };

            tracks_matching.push(DiscogsTrackMatch {
                music_file,
                track: track.deref().clone(),
                position: (index + 1) as u32,
            })
        }

        Some(tracks_matching)
    }

    fn match_release_with_music_files_simple<'a>(
        release: DiscogsRelease,
        music_files: &Vec<&'a MusicFile>,
    ) -> Option<Vec<DiscogsTrackMatch<'a>>> {
        let track_list = release.valid_track_list();

        let mut tracks_matching: Vec<DiscogsTrackMatch> = vec![];

        for music_file in music_files {
            let tag = &music_file.tag;
            let Some((index, track)) = track_list.iter().enumerate().find(|(index, _)| {
                tag.track_number().map(|v| &((v - 1) as usize) == index).unwrap_or(true)
            }) else { return None; };

            tracks_matching.push(DiscogsTrackMatch {
                music_file,
                track: track.deref().clone(),
                position: (index + 1) as u32,
            })
        }

        Some(tracks_matching)
    }

    fn search_master_release<'a>(&'a self, params: &'a [(&str, String)], console: &'a Console) -> impl Iterator<Item=Result<String>> + '_ {
        std::iter::once_with(move || {
            let mut search_params: Vec<(&str, String)> = vec![("type", "master".to_owned())];
            search_params.extend_from_slice(params);
            self.fetch_search_results(search_params, console)
        })
            .map_ok(|v| v.results)
            .flatten_ok()
            .map_ok(|v| -> Result<String> {
                let master: DiscogsMaster = self.fetch_by_url(&v.resource_url, console)?;
                Ok(master.main_release_url)
            })
            .flatten()
    }

    fn search_release<'a>(&'a self, params: &'a [(&str, String)], console: &'a Console) -> impl Iterator<Item=Result<String>> + '_ {
        std::iter::once_with(move || {
            let mut search_params: Vec<(&str, String)> = vec![("type", "release".to_owned())];
            search_params.extend_from_slice(params);
            self.fetch_search_results(search_params, console)
        })
            .map_ok(|v| v.results)
            .flatten_ok()
            .map_ok(|v| v.resource_url)
    }

    fn common_search_params_from_music_files(music_files: &[&MusicFile]) -> Vec<Vec<(&'static str, String)>> {
        let artist = (
            "artist",
            music_files
                .iter()
                .filter_map(|v| v.tag.artist().map(StringExtensions::simplify))
                .unique()
                .join(" ")
        );
        let album = (
            "release_title",
            music_files
                .iter()
                .filter_map(|v| v.tag.album().map(StringExtensions::simplify))
                .unique()
                .join(" ")
        );
        let year = (
            "year",
            music_files
                .iter()
                .filter_map(|v| v.tag.year().map(|v| v.to_string()))
                .unique()
                .join(" ")
        );
        vec![
            vec![
                album.clone(),
                year.clone(),
            ],
            vec![
                album.clone(),
            ],
            vec![
                artist.clone(),
                album.clone(),
                year.clone(),
            ],
            vec![
                artist.clone(),
                album.clone(),
            ],
            vec![
                artist.clone(),
            ],
        ]
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

    fn fetch_by_url<U, T>(&self, url: U, console: &Console) -> Result<T>
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
            let response = self.http_client.get(url.clone()).send()?;
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
                thread::sleep(Duration::from_secs_f64(skip * 60f64 / rate_limit));
            } else {
                bail!("Expected successful status code but got {}", status)
            }
        }
    }

    fn ask_for_release_id(reason: &str) -> Result<Option<String>> {
        let selected = Select::new()
            .with_prompt(reason.styled().yellow().to_string())
            .default(0)
            .item("Enter Discogs ID")
            .item("Take as is")
            .interact()?;

        match selected {
            0 => Input::new()
                .with_prompt("Please enter Discogs release ID".styled().bold().to_string())
                .interact_text()
                .context("Failed to interact")
                .and_then(|v: String| {
                    #[allow(clippy::unwrap_used)]
                        let regex1 = Regex::new(r"^\[r([0-9]+)\]$").unwrap();
                    #[allow(clippy::unwrap_used)]
                        let regex2 = Regex::new(r"^([0-9]+)$").unwrap();

                    #[allow(clippy::unwrap_used)]
                    match regex1.captures(&v).or_else(|| regex2.captures(&v)) {
                        None => bail!("Invalid Discogs release ID: {}", v),
                        Some(captures) => Ok(captures.get(1).unwrap().as_str().to_owned())
                    }
                })
                .map(Some),
            1 => Ok(None),
            _ => bail!("Unsupported option")
        }
    }
}
