use std::{f64, fs, thread};
use std::borrow::Borrow;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::mem::swap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
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
use crate::discogs::model::refined;
use crate::discogs::model::serialized;
use crate::music_file::MusicFile;
use crate::util::console_styleable::ConsoleStyleable;
use crate::util::path_extensions::PathExtensions;
use crate::util::string_extensions::StringExtensions;

pub struct DiscogsMatcher {
    http_client: blocking::Client,
}

const DISCOGS_TOKEN_FILE_NAME: &str = ".discogs_token";

impl DiscogsMatcher {
    pub fn with_optional_token(discogs_token: &Option<String>) -> Result<Self> {
        let discogs_token = match discogs_token {
            Some(x) => x.to_owned(),
            None => {
                let discogs_token_file = Self::get_discogs_token_file_path()
                    .with_context(|| format!("Supply discogs token with commandline argument (refer to --help) or with the file ~/{}", DISCOGS_TOKEN_FILE_NAME))?;
                fs::read_to_string(discogs_token_file)?.trim().to_owned()
            }
        };

        DiscogsMatcher::new(&discogs_token)
    }

    pub fn new(discogs_token: &str) -> Result<Self> {
        Ok(DiscogsMatcher {
            http_client: blocking::ClientBuilder::new()
                .default_headers(Self::common_headers(discogs_token)?)
                .build()?,
        })
    }

    fn common_headers(discogs_token: &str) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::try_from(format!(
                "{name}/{version} +{site}",
                name = env!("CARGO_PKG_NAME"),
                version = env!("CARGO_PKG_VERSION"),
                site = "https://github.com/vganin/music-files-organizer"
            ))?,
        );
        headers.insert(
            AUTHORIZATION,
            HeaderValue::try_from(format!("Discogs token={}", discogs_token))?,
        );
        Ok(headers)
    }

    fn get_discogs_token_file_path() -> Option<PathBuf> {
        Some(dirs::home_dir()?.join(DISCOGS_TOKEN_FILE_NAME))
    }
}

pub struct DiscogsTrackMatch<'a> {
    pub music_file: &'a MusicFile,
    pub track: refined::DiscogsTrack,
}

pub enum DiscogsReleaseMatchResult<'a> {
    Matched {
        tracks_matching: Vec<DiscogsTrackMatch<'a>>,
        release: refined::DiscogsRelease,
    },
    Unmatched(Vec<&'a MusicFile>),
}

impl DiscogsMatcher {
    pub fn match_music_files<'a>(
        &self,
        music_files: impl Iterator<Item = &'a MusicFile>,
        force_discogs_release_id: &Option<String>,
    ) -> Result<Vec<DiscogsReleaseMatchResult<'a>>> {
        let mut files_grouped_by_parent_path: HashMap<&Path, Vec<&MusicFile>> = HashMap::new();
        for music_file in music_files {
            let parent_path = music_file.file_path.parent_or_empty();
            files_grouped_by_parent_path
                .entry(parent_path)
                .or_default()
                .push(music_file);
        }

        let mut result = Vec::new();

        for (path, music_files) in files_grouped_by_parent_path {
            let mut match_result: DiscogsReleaseMatchResult =
                DiscogsReleaseMatchResult::Unmatched(music_files.clone());

            if force_discogs_release_id.is_none() {
                console_print!(
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

                let common_search_params =
                    Self::common_search_params_from_music_files(&music_files);
                let release_urls = common_search_params.iter().flat_map(|params| {
                    self.search_master_release(params)
                        .chain(self.search_release(params))
                        .take(5) // No more than 5 release fetches per params combinations to give other combinations realistic chances
                });

                let mut checked_release_urls = HashSet::new();
                for release_url in release_urls {
                    let release_url = release_url?;
                    if checked_release_urls.contains(&release_url) {
                        continue;
                    } else {
                        checked_release_urls.insert(release_url.clone());
                    }

                    let serialized_release: serialized::DiscogsRelease =
                        self.fetch_by_url(release_url)?;
                    let refined_release = refined::DiscogsRelease::from(&serialized_release)?;

                    // FIXME: clone() is redundant here
                    match Self::match_release_with_music_files(
                        refined_release.clone(),
                        &music_files,
                        false,
                    ) {
                        None => continue,
                        Some(tracks_matching) => {
                            match_result = Matched {
                                tracks_matching,
                                release: refined_release,
                            };
                            break;
                        }
                    }
                }
            }

            if let DiscogsReleaseMatchResult::Unmatched(_) = match_result {
                let mut release_id = force_discogs_release_id
                    .as_ref()
                    .map(|v| Self::extract_discogs_id(v).map(|v| v.to_owned()))
                    .transpose()?;

                if release_id.is_none() {
                    release_id = Self::ask_for_release_id(&format!(
                        "Can't find release for {}",
                        path.display().path_styled()
                    ))?;
                }

                if let Some(release_id) = release_id {
                    let mut release_id = release_id;
                    loop {
                        let serialized_release = self.fetch_release_by_id(&release_id)?;
                        let refined_release = refined::DiscogsRelease::from(&serialized_release)?;

                        // FIXME: clone() is redundant here
                        match Self::match_release_with_music_files(
                            refined_release.clone(),
                            &music_files,
                            true,
                        ) {
                            None => {
                                match Self::ask_for_release_id(
                                    &format!("Failed to match with ID {}", release_id)
                                        .error_styled()
                                        .to_string(),
                                )? {
                                    None => break,
                                    Some(new_release_id) => release_id = new_release_id,
                                }
                            }
                            Some(tracks_matching) => {
                                match_result = Matched {
                                    tracks_matching,
                                    release: refined_release,
                                };
                                break;
                            }
                        }
                    }
                }
            }

            if let Matched { release, .. } = &match_result {
                console_print!("Will use {}", release.uri.as_str().path_styled());
            } else {
                console_print!("Will use file tags as is");
            }

            result.push(match_result);
        }

        Ok(result)
    }

    pub fn download_cover(&self, url: &str, path: &Path, pb: &ProgressBar) -> Result<()> {
        let mut response = self.get_ok(url)?;

        let mut file =
            &mut ProgressWriter::new(fs::File::create(path)?, |bytes| pb.inc(bytes as u64));

        pb.set_length(
            response
                .content_length()
                .context("Failed to get content length")?,
        );
        pb.set_position(0);

        response.copy_to(&mut file)?;

        Ok(())
    }

    fn match_release_with_music_files<'a>(
        release: refined::DiscogsRelease,
        music_files: &Vec<&'a MusicFile>,
        simplified_match: bool,
    ) -> Option<Vec<DiscogsTrackMatch<'a>>> {
        let track_list = release.tracks;

        if track_list.is_empty() || track_list.len() != music_files.len() {
            return None;
        }

        let mut tracks_matching: Vec<DiscogsTrackMatch> = vec![];

        for music_file in music_files {
            let tag = &music_file.tag;
            let track_title = tag
                .title()
                .or_else(|| music_file.file_path.file_stem().and_then(|v| v.to_str()))
                .unwrap_or_default();
            let sorted_by_title_similarity = track_list
                .iter()
                .sorted_by(|a, b| {
                    track_title
                        .similarity_score(&b.title)
                        .partial_cmp(&track_title.similarity_score(&a.title))
                        .unwrap()
                })
                .collect_vec();
            let Some(track) = sorted_by_title_similarity.iter().find(|track| {
                let disc_position_matched = || tag.disc().unwrap_or(1) == track.disc && tag.track_number() == Some(track.position);
                let title_matched = || track_title.is_similar(&track.title);
                let duration_matched = || {
                    const DURATION_DIFF_THRESHOLD: Duration = Duration::from_secs(30);
                    let Some(mut duration1) = music_file.duration else { return false; };
                    let Some(mut duration2) = track.duration else { return false; };
                    if duration2 < duration1 { swap(&mut duration1, &mut duration2); };
                    duration2 - duration1 < DURATION_DIFF_THRESHOLD
                };
                if simplified_match {
                    disc_position_matched()
                } else {
                    (title_matched() && duration_matched()) || (title_matched() && disc_position_matched())
                }
            }) else {
                return None;
            };

            tracks_matching.push(DiscogsTrackMatch {
                music_file,
                track: track.deref().clone(),
            })
        }

        Some(tracks_matching)
    }

    fn search_master_release<'a>(
        &'a self,
        params: &'a [(&str, String)],
    ) -> impl Iterator<Item = Result<String>> + '_ {
        std::iter::once_with(move || {
            let mut search_params: Vec<(&str, String)> = vec![("type", "master".to_owned())];
            search_params.extend_from_slice(params);
            self.fetch_search_results(search_params)
        })
        .map_ok(|v| v.results)
        .flatten_ok()
        .map_ok(|v| -> Result<String> {
            let master: serialized::DiscogsMaster = self.fetch_by_url(v.resource_url)?;
            Ok(master.main_release_url)
        })
        .flatten()
    }

    fn search_release<'a>(
        &'a self,
        params: &'a [(&str, String)],
    ) -> impl Iterator<Item = Result<String>> + '_ {
        std::iter::once_with(move || {
            let mut search_params: Vec<(&str, String)> = vec![("type", "release".to_owned())];
            search_params.extend_from_slice(params);
            self.fetch_search_results(search_params)
        })
        .map_ok(|v| v.results)
        .flatten_ok()
        .map_ok(|v| v.resource_url)
    }

    fn common_search_params_from_music_files(
        music_files: &[&MusicFile],
    ) -> Vec<Vec<(&'static str, String)>> {
        let artist = (
            "artist",
            music_files
                .iter()
                .filter_map(|v| v.tag.artist().map(StringExtensions::simplify))
                .unique()
                .join(" "),
        );
        let album = (
            "release_title",
            music_files
                .iter()
                .filter_map(|v| v.tag.album().map(StringExtensions::simplify))
                .unique()
                .join(" "),
        );
        let year = (
            "year",
            music_files
                .iter()
                .filter_map(|v| v.tag.year().map(|v| v.to_string()))
                .unique()
                .join(" "),
        );
        vec![
            vec![album.clone(), year.clone()],
            vec![album.clone()],
            vec![artist.clone(), album.clone(), year.clone()],
            vec![artist.clone(), album.clone()],
            vec![artist.clone()],
        ]
    }

    fn fetch_release_by_id(&self, release_id: &str) -> Result<serialized::DiscogsRelease> {
        let url = &format!("https://api.discogs.com/releases/{}", release_id);
        self.fetch_by_url(url)
    }

    fn fetch_search_results<I, K, V>(
        &self,
        params: I,
    ) -> Result<serialized::DiscogsSearchResultPage>
    where
        I: IntoIterator,
        I::Item: Borrow<(K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let url = Url::parse_with_params("https://api.discogs.com/database/search", params)?;
        self.fetch_by_url(url)
    }

    fn fetch_by_url<U, T>(&self, url: U) -> Result<T>
    where
        U: IntoUrl + Clone + Display,
        T: DeserializeOwned,
    {
        Ok(serde_json::from_value(
            self.get_ok(url)?.json::<serde_json::Value>()?,
        )?)
    }

    fn get_ok<T: IntoUrl + Clone + Display>(&self, url: T) -> Result<Response> {
        console_print!("Fetching {}", (&url).path_styled());
        loop {
            let response = self.http_client.get(url.clone()).send()?;
            let status = response.status();
            if status.is_success() {
                break Ok(response);
            } else if status == StatusCode::TOO_MANY_REQUESTS {
                console_print!(
                    "{}",
                    "Reached requests limit! Slowing down..."
                        .styled()
                        .bold()
                        .yellow()
                );
                let header_as_number = |header| -> Result<f64> {
                    response
                        .headers()
                        .get(header)
                        .map(|v| -> Result<f64> { Ok(v.to_str()?.parse::<f64>()?) })
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
                .with_prompt(
                    "Please enter Discogs release ID"
                        .styled()
                        .bold()
                        .to_string(),
                )
                .interact_text()
                .context("Failed to interact")
                .and_then(|v: String| Self::extract_discogs_id(&v).map(ToOwned::to_owned))
                .map(Some),
            1 => Ok(None),
            _ => bail!("Unsupported option"),
        }
    }

    fn extract_discogs_id(string: &str) -> Result<&str> {
        #[allow(clippy::unwrap_used)]
        let regex1 = Regex::new(r"^\[r([0-9]+)\]$").unwrap();
        #[allow(clippy::unwrap_used)]
        let regex2 = Regex::new(r"^([0-9]+)$").unwrap();

        #[allow(clippy::unwrap_used)]
        match regex1.captures(string).or_else(|| regex2.captures(string)) {
            None => bail!("Invalid Discogs release ID: {}", string),
            Some(captures) => Ok(captures.get(1).unwrap().as_str()),
        }
    }
}
