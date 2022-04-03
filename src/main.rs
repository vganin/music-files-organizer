extern crate core;

use std::{fs, io};
use std::collections::{HashMap, HashSet};
use std::fs::{File, metadata};
use std::hash::Hash;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use dyn_clone::clone_box;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;
use progress_streams::ProgressWriter;
use question::{Answer, Question};
use regex::Regex;
use reqwest::{blocking, Url};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use sanitize_filename::{Options, sanitize_with_options};
use tempfile::NamedTempFile;
use walkdir::WalkDir;

use crate::tag::Tag;

mod tag;
mod transcode;

const DISCOGS_RELEASE_TAG: &str = "DISCOGS_RELEASE";
const DISCOGS_TOKEN_FILE_NAME: &str = ".discogs_token";

const COVER_FILE_NAME_WITHOUT_EXT: &str = "cover";

const PROGRESS_TICK_MS: u64 = 100u64;

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
struct Cli {
    #[clap(long)]
    discogs_token: Option<String>,

    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Import(ImportArgs),
    AddMissingCovers(AddMissingCoversArgs),
}

#[derive(Args)]
struct ImportArgs {
    #[clap(long, parse(from_os_str))]
    from: PathBuf,

    #[clap(long, parse(from_os_str))]
    to: PathBuf,
}

#[derive(Args)]
struct AddMissingCoversArgs {
    #[clap(long, parse(from_os_str))]
    to: PathBuf,
}

struct MusicFile {
    file_path: PathBuf,
    tag: Box<dyn Tag>,
}

struct DiscogsReleaseInfo {
    json: serde_json::Value,
}

struct DiscogsRelease {
    music_files: Vec<MusicFile>,
    discogs_info: DiscogsReleaseInfo,
}

struct MusicFileChange {
    source: MusicFile,
    target: MusicFile,
    transcode_to_mp4: bool,
    bytes_to_transfer: u64,
}

#[derive(Hash, PartialEq, Eq)]
struct CoverChange {
    path: PathBuf,
    uri: String,
}

#[derive(Hash, PartialEq, Eq)]
struct Cleanup {
    path: PathBuf,
}

struct ChangeList {
    music_files: Vec<MusicFileChange>,
    covers: Vec<CoverChange>,
    cleanups: Vec<Cleanup>,
}

fn main() {
    let cli = Cli::parse();

    let discogs_token = match cli.discogs_token {
        Some(x) => x.to_owned(),
        None => {
            let discogs_token_file = get_discogs_token_file_path()
                .expect("Supply discogs token with commandline argument (refer to --help)");
            fs::read_to_string(&discogs_token_file).ok()
                .expect(&format!("Supply discogs token with commandline argument (refer to --help) or with the file \"{}\"", discogs_token_file.display()))
                .trim().to_owned()
        }
    };
    let http_client = blocking::Client::new();
    let headers = common_headers(&discogs_token);

    match cli.command {
        Command::Import(args) => import(args, &http_client, &headers),
        Command::AddMissingCovers(args) => add_missing_covers(args, &http_client, &headers)
    };
}

fn import(args: ImportArgs, http_client: &blocking::Client, headers: &HeaderMap) {
    if !metadata(&args.to).unwrap().is_dir() {
        panic!("Output path is not directory")
    }

    let pb = default_spinner();

    let music_files = get_music_files(&args.from);
    let discogs_releases = fetch_discogs_releases(&http_client, &headers, music_files, &pb);
    let changes = calculate_changes(
        discogs_releases,
        &args.to,
        true,
        false,
    );

    if changes.music_files.is_empty() && changes.covers.is_empty() {
        pb.println(format!("Nothing to do, all good"));
        return;
    }

    if Question::new("Do you want to print changes?")
        .default(Answer::NO)
        .show_defaults()
        .confirm() == Answer::YES
    {
        print_changes_details(&changes, &pb);
    }

    if Question::new("Do you want to make changes?")
        .default(Answer::YES)
        .show_defaults()
        .confirm() == Answer::YES
    {
        write_music_files(&changes.music_files);
        download_covers(&changes.covers, &http_client, &headers);
        cleanup(&changes.cleanups);
    }
}

fn add_missing_covers(args: AddMissingCoversArgs, http_client: &blocking::Client, headers: &HeaderMap) {
    let root_path = args.to;
    let pb = default_spinner();
    let mut downloaded_covers_count = 0;

    WalkDir::new(&root_path).into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
        .inspect(|e| {
            let display_path = e.path().strip_prefix(&root_path).unwrap().display();
            pb.set_message(format!("Processing \"{}\"...", display_path));
        })
        .filter(|e| {
            let path = e.path();
            for extension in ["jpg", "jpeg", "png"] {
                if Path::exists(&path.join(COVER_FILE_NAME_WITHOUT_EXT).with_extension(extension)) {
                    return false;
                }
            }
            true
        })
        .for_each(|e| {
            let path = e.path();
            if let Some(cover_uri) = WalkDir::new(path)
                .contents_first(true)
                .max_depth(1)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|e| !e.file_type().is_dir())
                .filter_map(|e| {
                    let path = e.path();
                    let format = path.extension().unwrap().to_str().unwrap();
                    tag::read_from_path(&path, format)
                })
                .next()
                .and_then(|tag| {
                    fetch_discogs_release_info(
                        http_client,
                        headers,
                        &[tag.artist().unwrap().to_string()],
                        tag.album().unwrap(),
                        false,
                        &pb,
                    )
                })
                .and_then(|discogs_info| {
                    cover_uri_from_discogs_info(&discogs_info).map(ToOwned::to_owned)
                })
            {
                let cover_uri_as_file_path = PathBuf::from(Url::parse(&cover_uri).unwrap().path());
                let cover_extension = cover_uri_as_file_path.extension().unwrap();
                let cover_file_name = PathBuf::from(COVER_FILE_NAME_WITHOUT_EXT).with_extension(cover_extension);
                let cover_path = path.join(cover_file_name);
                let display_path = e.path().strip_prefix(&root_path).unwrap().display();
                pb.set_message(format!("Downloading cover to \"{}\"", display_path));
                download_cover(http_client, headers, &cover_uri, &cover_path, &pb);
                downloaded_covers_count += 1;
            }
        });

    pb.finish_with_message(format!("Downloaded {} cover(s)", downloaded_covers_count))
}

fn get_music_files(path: impl AsRef<Path>) -> Vec<MusicFile> {
    let pb = default_spinner();
    let result = WalkDir::new(path).into_iter()
        .filter_map(Result::ok)
        .filter(|e| !e.file_type().is_dir())
        .filter_map(|e| {
            let path = e.path();
            pb.set_message(format!("Analyzing \"{}\"...", path.file_name().unwrap().to_str().unwrap()));
            let format = path.extension().unwrap().to_str().unwrap();
            tag::read_from_path(&path, format).map(|tag| {
                MusicFile {
                    file_path: PathBuf::from(path),
                    tag,
                }
            })
        })
        .collect();
    pb.finish_and_clear();
    result
}

fn fetch_discogs_releases(
    http_client: &blocking::Client,
    headers: &HeaderMap,
    music_files: Vec<MusicFile>,
    pb: &ProgressBar,
) -> Vec<DiscogsRelease> {
    let mut files_grouped_by_parent_path: HashMap<PathBuf, Vec<MusicFile>> = HashMap::new();

    for music_file in music_files {
        let parent_path = music_file.file_path.parent().unwrap().to_owned();
        files_grouped_by_parent_path.entry(parent_path).or_default().push(music_file);
    }

    let mut result = Vec::new();

    for (path, music_files) in files_grouped_by_parent_path {
        let artists: Vec<String> = music_files.iter().map(|v| v.tag.artist().unwrap().to_string()).unique().collect();
        let albums: Vec<String> = music_files.iter().map(|v| v.tag.album().unwrap().to_string()).unique().collect();

        assert_eq!(
            albums.len(), 1, "Different albums in one folder \"{}\": [{}]",
            path.display(),
            albums.iter().map(|v| format!("\"{}\"", v)).join(", ")
        );

        let discogs_info = fetch_discogs_release_info(
            http_client,
            headers,
            &artists,
            &albums[0],
            true,
            pb,
        ).unwrap();

        result.push(DiscogsRelease {
            music_files,
            discogs_info,
        });
    }

    result
}

fn fetch_discogs_release_info(
    http_client: &blocking::Client,
    headers: &HeaderMap,
    artists: &[String],
    album: &str,
    ask_release_id_as_fallback: bool,
    pb: &ProgressBar,
) -> Option<DiscogsReleaseInfo> {
    pb.println(format!("Searching Discogs for \"{} - {}\"", &artists.join(", "), album));

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
            let search_url = Url::parse_with_params("https://api.discogs.com/database/search", search_params).unwrap();

            pb.println(format!("Fetching {}", search_url));

            http_client
                .get(search_url)
                .headers(headers.clone())
                .send()
                .unwrap()
                .json::<serde_json::Value>()
                .unwrap()
                ["results"][0]["resource_url"]
                .as_str()
                .map(|v| v.to_owned())
        })
        .find_map(Option::Some)
        .or_else(|| {
            if ask_release_id_as_fallback {
                match Question::new(format!("Can't find release for \"{} - {}\". Please enter release ID from Discogs:", artists.join(", "), album).as_str())
                    .ask()
                {
                    Some(Answer::RESPONSE(response)) => Some(format!("https://api.discogs.com/releases/{}", response)),
                    _ => None
                }
            } else {
                pb.println(format!("Can't find release for \"{} - {}\"", &artists.join(", "), album));
                None
            }
        })?;

    pb.println(format!("Fetching {}", release_url));

    let release_object = http_client
        .get(release_url)
        .headers(headers.clone())
        .send()
        .unwrap()
        .json::<serde_json::Value>()
        .unwrap()
        .clone();

    pb.println(format!("Will use {}", release_object["uri"].as_str().unwrap()));

    Some(DiscogsReleaseInfo {
        json: release_object
    })
}

fn calculate_changes(
    discogs_releases: Vec<DiscogsRelease>,
    import_path: &Path,
    clean_targets: bool,
    clean_sources: bool,
) -> ChangeList {
    let mut music_file_changes = Vec::new();
    let mut cover_changes = HashSet::new();

    for DiscogsRelease { music_files, discogs_info } in discogs_releases {
        for music_file in music_files {
            let source_tag = &music_file.tag;
            let target_tag = tag_from_discogs_info(source_tag, &discogs_info);
            let source_path = &music_file.file_path;
            let source_extension = source_path.extension().unwrap().to_str().unwrap();
            let transcode_to_mp4 = source_extension == "flac";
            let target_folder_path = import_path.join(music_folder_path(&*target_tag));
            let target_extension = if transcode_to_mp4 { "m4a" } else { source_extension };
            let target_path = target_folder_path.join(music_file_name(&*target_tag, target_extension));
            let bytes_to_transfer = fs::metadata(&source_path).unwrap().len();

            music_file_changes.push(MusicFileChange {
                source: music_file,
                target: MusicFile {
                    file_path: target_path,
                    tag: target_tag,
                },
                transcode_to_mp4,
                bytes_to_transfer,
            });

            if let Some(uri) = cover_uri_from_discogs_info(&discogs_info) {
                let uri_as_file_path = PathBuf::from(Url::parse(&uri).unwrap().path());
                let extension = uri_as_file_path.extension().unwrap();
                let file_name = PathBuf::from(COVER_FILE_NAME_WITHOUT_EXT).with_extension(extension);
                cover_changes.insert(CoverChange {
                    path: target_folder_path.join(file_name),
                    uri: uri.to_owned(),
                });
            }
        }
    }

    music_file_changes.sort_by(|lhs, rhs| {
        let lhs = &lhs.target.tag;
        let rhs = &rhs.target.tag;
        let lhs_album = lhs.album().unwrap();
        let rhs_album = rhs.album().unwrap();
        let lhs_year = lhs.year().unwrap();
        let rhs_year = rhs.year().unwrap();
        if lhs_album == rhs_album && lhs_year == rhs_year {
            lhs.track().cmp(&rhs.track())
        } else if lhs_year == rhs_year {
            lhs_album.cmp(rhs_album)
        } else {
            lhs_year.cmp(&rhs_year)
        }
    });

    let cover_changes = cover_changes.into_iter().collect();

    let cleanups = find_cleanups(
        &music_file_changes,
        &cover_changes,
        clean_targets,
        clean_sources,
    );

    ChangeList {
        music_files: music_file_changes,
        covers: cover_changes,
        cleanups,
    }
}

fn find_cleanups(
    music_files: &Vec<MusicFileChange>,
    covers: &Vec<CoverChange>,
    clean_targets: bool,
    clean_sources: bool,
) -> Vec<Cleanup> {
    let mut result = HashSet::new();

    let mut source_folder_paths = HashSet::new();
    let mut target_folder_paths = HashSet::new();
    let mut target_paths = HashSet::new();

    for change in music_files {
        source_folder_paths.insert(PathBuf::from(change.source.file_path.parent().unwrap()));
        target_folder_paths.insert(PathBuf::from(change.target.file_path.parent().unwrap()));
        target_paths.insert(change.target.file_path.to_owned());
    }

    for change in covers {
        target_folder_paths.insert(PathBuf::from(change.path.parent().unwrap()));
        target_paths.insert(change.path.to_owned());
    }

    if clean_targets {
        for target_folder_path in target_folder_paths {
            target_folder_path.read_dir().unwrap().for_each(|entry| {
                let entry = entry.unwrap();
                let path = entry.path();
                if !target_paths.contains(&path) {
                    result.insert(Cleanup { path });
                }
            });
        }
    }

    if clean_sources {
        for source_folder_path in source_folder_paths {
            source_folder_path.read_dir().unwrap().for_each(|entry| {
                let entry = entry.unwrap();
                let path = entry.path();
                if !target_paths.contains(&path) {
                    result.insert(Cleanup { path });
                }
            });
        }
    }

    result.into_iter().collect()
}

fn print_changes_details(changes: &ChangeList, pb: &ProgressBar) {
    let mut step_number = 1u32;

    for change in &changes.music_files {
        let source = &change.source;
        let target = &change.target;

        let source_file_path = &source.file_path;
        let target_file_path = &target.file_path;
        if source_file_path == target_file_path {
            pb.println(format!(
                "{:02}. {} \"{}\"",
                step_number,
                if change.transcode_to_mp4 { "Transcode" } else { "Update" },
                source_file_path.file_name().unwrap().to_str().unwrap(),
            ));
        } else {
            let common_file_prefix = common_path::common_path(source_file_path, target_file_path).unwrap();
            pb.println(format!(
                "{:02}. {} \"{}\" -> \"{}\"",
                step_number,
                if change.transcode_to_mp4 { "Transcode" } else { "Copy" },
                source_file_path.strip_prefix(&common_file_prefix).unwrap().display(),
                target_file_path.strip_prefix(&common_file_prefix).unwrap().display(),
            ));
        }

        let source_tag = &source.tag;
        let target_tag = &target.tag;
        for frame_id in target_tag.frame_ids() {
            let source_frame_value = source_tag.frame_content(&frame_id).map(|v| v.stringify_content());
            let target_frame_value = target_tag.frame_content(&frame_id).map(|v| v.stringify_content());
            if target_frame_value != source_frame_value {
                pb.println(format!(
                    "    Change {}: \"{}\" -> \"{}\"",
                    frame_id.description(),
                    source_frame_value.unwrap_or(String::from("None")),
                    target_frame_value.unwrap_or(String::from("None")),
                ));
            }
        }

        step_number += 1
    }

    for change in &changes.covers {
        pb.println(format!(
            "{:02}. Download cover by URI {} to \"{}\"",
            step_number,
            change.uri,
            change.path.display(),
        ));
        step_number += 1;
    }

    for cleanup in &changes.cleanups {
        pb.println(format!(
            "{:02}. ⚠️Remove \"{}\"",
            step_number,
            cleanup.path.display(),
        ));
        step_number += 1;
    }
}

fn write_music_files(changes: &Vec<MusicFileChange>) {
    if changes.is_empty() { return; };

    let total_bytes_to_transfer: u64 = changes.iter()
        .map(|v| v.bytes_to_transfer)
        .sum();

    let pb = default_progress_bar(total_bytes_to_transfer);

    for change in changes {
        let source = &change.source;
        let target = &change.target;
        let source_path = &source.file_path;
        let target_path = &target.file_path;
        let target_tag = &target.tag;

        pb.set_message(format!("Writing \"{}\"", source_path.file_name().unwrap().to_str().unwrap()));

        let mut temp_file = {
            if change.transcode_to_mp4 {
                let mut named_temp_file = NamedTempFile::new().unwrap();
                transcode::to_mp4(&source_path, named_temp_file.path());
                let mut tag = tag::read_from_path(named_temp_file.path(), "m4a").unwrap();
                tag.set_from(target_tag);
                tag.write_to(named_temp_file.as_file_mut());
                named_temp_file.into_file()
            } else {
                let mut source_file = File::open(&source_path).unwrap();
                let mut temp_file = tempfile::tempfile().unwrap();
                io::copy(&mut source_file, &mut temp_file).unwrap();
                target_tag.write_to(&mut temp_file);
                temp_file
            }
        };

        fs::create_dir_all(target_path.parent().unwrap()).unwrap();

        let mut target_file = ProgressWriter::new(
            File::create(&target_path).unwrap(),
            |bytes| pb.inc(bytes as u64),
        );
        temp_file.seek(io::SeekFrom::Start(0)).unwrap();

        io::copy(&mut temp_file, &mut target_file).unwrap();
    }

    pb.finish_with_message(format!("Written {} file(s)", &changes.len()));
}

fn download_covers(
    changes: &Vec<CoverChange>,
    http_client: &blocking::Client,
    headers: &HeaderMap,
) {
    if changes.is_empty() { return; };

    let count = changes.len();
    let pb = default_progress_bar(!0);

    for (index, change) in changes.iter().enumerate() {
        pb.set_message(format!("Downloading cover {}/{}", index + 1, count));
        download_cover(http_client, headers, &change.uri, &change.path, &pb);
    }

    pb.finish_with_message(format!("Downloaded {} cover(s)", count))
}

fn cleanup(cleanups: &[Cleanup]) {
    for cleanup in cleanups {
        let path = &cleanup.path;
        let metadata = fs::metadata(path).unwrap();
        if metadata.is_dir() {
            fs::remove_dir_all(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }
}

fn download_cover(
    http_client: &blocking::Client,
    headers: &HeaderMap,
    uri: &str,
    path: &Path,
    pb: &ProgressBar,
) {
    let mut response = http_client.get(uri)
        .headers(headers.clone())
        .send()
        .unwrap();

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

fn tag_from_discogs_info(original_tag: &Box<dyn Tag>, info: &DiscogsReleaseInfo) -> Box<dyn Tag> {
    let release = &info.json;
    let track_number = original_tag.track().unwrap();
    let track_list = release["tracklist"].as_array().unwrap().iter()
        .filter(|v| v["type_"].as_str().unwrap() == "track")
        .collect::<Vec<&serde_json::Value>>();
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
    new_tag.set_title(track["title"].as_str().unwrap().trim().to_owned());
    new_tag.set_album(release["title"].as_str().unwrap().trim().to_owned());
    new_tag.set_album_artist(
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
    );
    new_tag.set_artist(
        track_artists
            .or(Some(album_artists))
            .unwrap()
            .iter()
            .flat_map(|v| [v.0, v.1])
            .collect::<Vec<&str>>()
            .join(" ")
            .trim()
            .to_owned()
    );
    new_tag.set_year(release["year"].as_i64().unwrap() as i32);
    new_tag.set_track(track_number);
    new_tag.set_total_tracks(track_list.len() as u32);
    new_tag.set_genre(
        release["styles"].as_array().unwrap().iter()
            .map(|v| v.as_str().unwrap().trim())
            .collect::<Vec<&str>>()
            .join("; ")
    );
    new_tag.set_custom_text(DISCOGS_RELEASE_TAG.to_owned(), release["uri"].as_str().unwrap().to_owned());
    new_tag
}

fn cover_uri_from_discogs_info(info: &DiscogsReleaseInfo) -> Option<&str> {
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

fn get_discogs_token_file_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(DISCOGS_TOKEN_FILE_NAME))
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

fn music_file_name(tag: &dyn Tag, ext: &str) -> String {
    sanitize_path(match tag.disc() {
        Some(disc) => format!(
            "{disc:02}.{track:02}. {title}.{ext}",
            disc = disc,
            track = tag.track().unwrap(),
            title = tag.title().unwrap(),
            ext = ext,
        ),
        None => format!(
            "{track:02}. {title}.{ext}",
            track = tag.track().unwrap(),
            title = tag.title().unwrap(),
            ext = ext,
        ),
    })
}

fn music_folder_path(tag: &dyn Tag) -> PathBuf {
    let mut path = PathBuf::new();
    path.push(sanitize_path(tag.album_artist().unwrap()));
    path.push(sanitize_path(format!("({}) {}", tag.year().unwrap(), tag.album().unwrap())));
    path
}

fn sanitize_path<S: AsRef<str>>(name: S) -> String {
    let mut options = Options::default();
    options.replacement = "-";
    sanitize_with_options(name, options)
}

fn fix_discogs_artist_name(name: &str) -> &str {
    let regex = Regex::new(r".*( \([0-9]+\))").unwrap();
    match regex.captures(name) {
        Some(captures) => {
            let range = captures.get(1).unwrap().range();
            &name[..range.start]
        }
        None => name
    }
}

fn default_progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.red/yellow} [{elapsed_precise}] [{bar:50.red/yellow}] {bytes}/{total_bytes} {wide_msg:.bold.dim}")
            .progress_chars(":: ")
    );
    pb.enable_steady_tick(PROGRESS_TICK_MS);
    pb
}

fn default_spinner() -> ProgressBar {
    let pb = ProgressBar::new(!0);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.red/yellow} [{elapsed_precise}] {wide_msg:.bold.dim}")
    );
    pb.enable_steady_tick(PROGRESS_TICK_MS);
    pb
}
