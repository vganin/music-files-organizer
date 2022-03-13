extern crate core;

use std::{fs, io};
use std::collections::{HashMap, HashSet};
use std::fs::{File, metadata};
use std::hash::Hash;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use progress_streams::ProgressWriter;
use question::{Answer, Question};
use regex::Regex;
use reqwest::{blocking, Url};
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use sanitize_filename::{Options, sanitize_with_options};

use crate::tag::Tag;

mod tag;

const DISCOGS_RELEASE_TAG: &str = "DISCOGS_RELEASE";
const DISCOGS_TOKEN_FILE_NAME: &str = ".discogs_token";

const COVER_FILE_NAME_WITHOUT_EXT: &str = "cover";

const PROGRESS_TICK_MS: u64 = 100u64;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(long, parse(from_os_str), name = "from")]
    from_path: PathBuf,

    #[clap(long, parse(from_os_str), name = "to")]
    to_path: PathBuf,

    #[clap(long)]
    discogs_token: Option<String>,

    #[clap(long)]
    clean: bool,
}

struct MusicFile {
    file_path: PathBuf,
    tag: Box<dyn Tag>,
}

struct DiscogsReleaseInfo {
    json: serde_json::Value,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
struct ReleaseKey {
    artist: String,
    album: String,
}

struct MusicFileChange {
    source: MusicFile,
    target: MusicFile,
    bytes_to_transfer: u64,
}

#[derive(Hash, PartialEq, Eq)]
struct CoverChange {
    path: PathBuf,
    uri: String,
}

struct ChangeList {
    music_files: Vec<MusicFileChange>,
    covers: Vec<CoverChange>,
}

fn main() {
    let args = Args::parse();

    if !metadata(&args.from_path).unwrap().is_dir() {
        panic!("Output path is not directory")
    }

    let discogs_token = match &args.discogs_token {
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

    let source_music_files = inspect_path(&args.from_path);
    let discogs_releases = fetch_discogs_releases(&http_client, &headers, &source_music_files);
    let changes = calculate_changes(source_music_files, &discogs_releases, &args.to_path);

    if changes.music_files.is_empty() && changes.covers.is_empty() {
        println!("Nothing to do, all good");
        return;
    }

    if Question::new("Do you want to print changes?")
        .default(Answer::NO)
        .show_defaults()
        .confirm() == Answer::YES
    {
        print_changes_details(&changes);
    }

    if Question::new("Do you want to make changes?")
        .default(Answer::YES)
        .show_defaults()
        .confirm() == Answer::YES
    {
        write_music_files(&changes.music_files);
        download_covers(&changes.covers, &http_client, &headers);
        if args.clean {
            clean_target_folders(&changes);
        }
    }
}

fn inspect_path(path: impl AsRef<Path>) -> Vec<MusicFile> {
    let file_metadata = metadata(&path).unwrap();
    if file_metadata.is_file() {
        vec![inspect_file(&path)].into_iter().flatten().collect()
    } else if file_metadata.is_dir() {
        inspect_directory(&path)
    } else {
        vec![]
    }
}

fn inspect_directory(path: impl AsRef<Path>) -> Vec<MusicFile> {
    return fs::read_dir(path).unwrap()
        .flat_map(|entry| {
            let entry = entry.unwrap();
            let path = entry.path();
            inspect_path(&path)
        })
        .collect();
}

fn inspect_file(path: impl AsRef<Path>) -> Option<MusicFile> {
    tag::read_from_path(&path).map(|tag| {
        MusicFile {
            file_path: PathBuf::from(path.as_ref()),
            tag,
        }
    })
}

fn fetch_discogs_releases(
    http_client: &blocking::Client,
    headers: &HeaderMap,
    music_files: &Vec<MusicFile>,
) -> HashMap<ReleaseKey, DiscogsReleaseInfo> {
    let mut result: HashMap<ReleaseKey, DiscogsReleaseInfo> = HashMap::new();

    for music_file in music_files {
        let release_key = release_key(music_file);
        if !result.contains_key(&release_key) {
            let discogs_release_info = fetch_discogs_release(http_client, headers, &release_key);
            result.insert(release_key, discogs_release_info);
        }
    }

    result
}

fn fetch_discogs_release(
    http_client: &blocking::Client,
    headers: &HeaderMap,
    release_key: &ReleaseKey,
) -> DiscogsReleaseInfo {
    println!("Searching Discogs for \"{} - {}\"", release_key.artist, release_key.album);

    let title_query = format!("{} - {}", &release_key.artist, &release_key.album);

    let search_params_tries = vec![
        vec![
            ("type", "master"),
            ("artist", &release_key.artist),
            ("release_title", &release_key.album),
        ],
        vec![
            ("type", "release"),
            ("artist", &release_key.artist),
            ("release_title", &release_key.album),
        ],
        vec![
            ("type", "release"),
            ("query", &title_query),
        ],
    ];

    let release_url = search_params_tries.iter()
        .filter_map(|search_params| {
            let search_url = Url::parse_with_params("https://api.discogs.com/database/search", search_params).unwrap();

            println!("Fetching {}", search_url);

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
        .unwrap_or_else(|| {
            match Question::new(format!("Can't find release for \"{} - {}\". Please enter release ID from Discogs:", release_key.artist, release_key.album).as_str())
                .ask()
            {
                Some(Answer::RESPONSE(response)) => format!("https://api.discogs.com/releases/{}", response),
                _ => panic!("Abort")
            }
        });

    println!("Fetching {}", release_url);

    let release_object = http_client
        .get(release_url)
        .headers(headers.clone())
        .send()
        .unwrap()
        .json::<serde_json::Value>()
        .unwrap()
        .clone();

    println!("Will use {}", release_object["uri"].as_str().unwrap());

    DiscogsReleaseInfo {
        json: release_object
    }
}

fn calculate_changes(
    source_music_files: Vec<MusicFile>,
    discogs_releases: &HashMap<ReleaseKey, DiscogsReleaseInfo>,
    import_path: &Path,
) -> ChangeList {
    let mut music_file_changes = Vec::with_capacity(source_music_files.len());
    let mut cover_changes = HashSet::new();

    for source in source_music_files {
        let release_info = discogs_releases.get(&release_key(&source)).unwrap();
        let source_tag = &*source.tag;
        let target_tag = tag_from_discogs_info(source_tag, release_info);
        let source_path = &source.file_path;
        let target_folder_path = import_path.join(music_folder_path(&*target_tag));
        let target_path = target_folder_path.join(music_file_name(
            &*target_tag, source_path.extension().unwrap().to_str().unwrap()));
        let bytes_to_transfer = fs::metadata(&source_path).unwrap().len();

        music_file_changes.push(MusicFileChange {
            source,
            target: MusicFile {
                file_path: target_path,
                tag: target_tag,
            },
            bytes_to_transfer,
        });

        if let Some(uri) = cover_uri_from_discogs_info(release_info) {
            let uri_as_file_path = PathBuf::from(Url::parse(&uri).unwrap().path());
            let extension = uri_as_file_path.extension().unwrap();
            let file_name = PathBuf::from(COVER_FILE_NAME_WITHOUT_EXT).with_extension(extension);
            cover_changes.insert(CoverChange {
                path: target_folder_path.join(file_name),
                uri: uri.to_owned(),
            });
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

    ChangeList {
        music_files: music_file_changes,
        covers: cover_changes.into_iter().collect(),
    }
}

fn print_changes_details(changes: &ChangeList) {
    let mut step_number = 1u32;

    for change in &changes.music_files {
        let source = &change.source;
        let target = &change.target;

        let source_file_path = &source.file_path;
        let target_file_path = &target.file_path;
        if source_file_path == target_file_path {
            println!(
                "{:02}. Update \"{}\"",
                step_number,
                source_file_path.file_name().unwrap().to_str().unwrap(),
            );
        } else {
            let common_file_prefix = common_path::common_path(source_file_path, target_file_path).unwrap();
            println!(
                "{:02}. Copy \"{}\" -> \"{}\"",
                step_number,
                source_file_path.strip_prefix(&common_file_prefix).unwrap().display(),
                target_file_path.strip_prefix(&common_file_prefix).unwrap().display(),
            );
        }

        let source_tag = &source.tag;
        let target_tag = &target.tag;
        for frame_id in target_tag.frame_ids() {
            let source_frame_value = source_tag.frame_content_as_string(&frame_id);
            let target_frame_value = target_tag.frame_content_as_string(&frame_id);
            if target_frame_value != source_frame_value {
                println!(
                    "    Change {}: \"{}\" -> \"{}\"",
                    target_tag.frame_human_readable_title(&frame_id).unwrap(),
                    source_frame_value.unwrap_or(String::from("None")),
                    target_frame_value.unwrap_or(String::from("None")),
                );
            }
        }

        step_number += 1
    }

    for change in &changes.covers {
        println!(
            "{:02}. Download cover by URI {} to \"{}\"",
            step_number,
            change.uri,
            &change.path.display(),
        );
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
            let mut source_file = File::open(&source_path).unwrap();
            let mut temp_file = tempfile::tempfile().unwrap();

            io::copy(&mut source_file, &mut temp_file).unwrap();
            temp_file.seek(io::SeekFrom::Start(0)).unwrap();
            target_tag.write_to(&mut temp_file);
            temp_file.seek(io::SeekFrom::Start(0)).unwrap();
            temp_file
        };

        fs::create_dir_all(target_path.parent().unwrap()).unwrap();

        let mut target_file = ProgressWriter::new(
            File::create(&target_path).unwrap(),
            |bytes| pb.inc(bytes as u64),
        );

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

fn clean_target_folders(changes: &ChangeList) {
    let mut target_folder_paths = HashSet::new();
    let mut target_paths = HashSet::new();

    for change in &changes.music_files {
        target_folder_paths.insert(PathBuf::from(change.target.file_path.parent().unwrap()));
        target_paths.insert(change.target.file_path.to_owned());
    }

    for change in &changes.covers {
        target_folder_paths.insert(PathBuf::from(change.path.parent().unwrap()));
        target_paths.insert(change.path.to_owned());
    }

    for target_folder_path in target_folder_paths {
        target_folder_path.read_dir().unwrap().for_each(|entry| {
            let entry = entry.unwrap();
            let path = entry.path();
            if !target_paths.contains(&path) {
                println!("{}", path.display());
                fs::remove_file(path).unwrap()
            }
        });
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

fn tag_from_discogs_info(original_tag: &dyn Tag, info: &DiscogsReleaseInfo) -> Box<dyn Tag> {
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

    let mut tag = tag::new(original_tag.kind());
    tag.set_title(track["title"].as_str().unwrap().trim().to_owned());
    tag.set_album(release["title"].as_str().unwrap().trim().to_owned());
    tag.set_album_artist(
        if album_artists.len() > 1 {
            "Various Artists"
        } else {
            album_artists.get(0).unwrap().0
        }.to_owned()
    );
    tag.set_artist(
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
    tag.set_year(release["year"].as_i64().unwrap() as i32);
    tag.set_track(track_number);
    tag.set_total_tracks(track_list.len() as u32);
    tag.set_genre(
        release["styles"].as_array().unwrap().iter()
            .map(|v| v.as_str().unwrap().trim())
            .collect::<Vec<&str>>()
            .join("; ")
    );
    tag.set_custom_tag(DISCOGS_RELEASE_TAG.to_owned(), release["uri"].as_str().unwrap().to_owned());
    tag
}

fn cover_uri_from_discogs_info(info: &DiscogsReleaseInfo) -> Option<&str> {
    let images_array = info.json["images"].as_array().unwrap();
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
    headers.insert(USER_AGENT, HeaderValue::try_from("orgtag").unwrap());
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

fn release_key(music_file: &MusicFile) -> ReleaseKey {
    ReleaseKey {
        artist: music_file.tag.artist().unwrap().to_string(),
        album: music_file.tag.album().unwrap().to_string(),
    }
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
    pb.set_style(default_progress_style());
    pb.enable_steady_tick(PROGRESS_TICK_MS);
    pb
}

fn default_progress_style() -> ProgressStyle {
    ProgressStyle::default_bar()
        .template("{spinner:.red/yellow} [{elapsed_precise}] [{bar:50.red/yellow}] {bytes}/{total_bytes} {wide_msg:.bold.dim}")
        .progress_chars(":: ")
}

