use std::{fs, io};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::fs::{File, metadata, OpenOptions};
use std::hash::Hash;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use id3::{frame, Tag, TagLike, Timestamp, v1, Version};
use indicatif::{ProgressBar, ProgressStyle};
use progress_streams::ProgressWriter;
use question::{Answer, Question};
use regex::Regex;
use reqwest::{blocking, Url};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue, USER_AGENT};
use sanitize_filename::{Options, sanitize_with_options};

const DISCOGS_RELEASE_ID_TAG: &str = "DISCOGS_RELEASE_ID";
const DISCOGS_TOKEN_FILENAME: &str = ".discogs_token";

const PROGRESS_TICK_MS: u64 = 100u64;

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(parse(from_os_str), value_name = "INPUT FILE")]
    input_file_path: PathBuf,

    #[clap(parse(from_os_str), value_name = "OUTPUT FILE")]
    output_file_path: PathBuf,

    #[clap(short, long)]
    discogs_token: Option<String>,

    #[clap(short, long)]
    clean: bool,
}

struct MusicFile {
    file_path: PathBuf,
    tag: Tag,
}

struct DiscogsReleaseInfo {
    json: serde_json::Value,
}

#[derive(Hash, PartialEq, Eq)]
struct ReleaseKey {
    artist: String,
    album: String,
}

struct MusicFileChange<'a> {
    bytes_to_transfer: u64,
    source: &'a MusicFile,
    target: MusicFile,
}

fn main() {
    let args = Args::parse();

    if args.input_file_path == args.output_file_path {
        panic!("Input and output paths should not be the same")
    }

    if !metadata(&args.output_file_path).unwrap().is_dir() {
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

    let source_music_files = inspect_path(&args.input_file_path);
    let discogs_releases = fetch_discogs_releases(&http_client, &headers, &source_music_files);
    let changes = calculate_changes(&source_music_files, &discogs_releases, &args.output_file_path);

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
        if args.clean {
            clean_release_folders(&changes);
        }
        write_music_files(&changes);
        download_covers(&http_client, &headers, &changes, &discogs_releases);
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
    match Tag::read_from_path(&path) {
        Ok(tag) => Some(MusicFile {
            file_path: PathBuf::from(path.as_ref()),
            tag,
        }),
        Err(_) => {
            return None;
        }
    }
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
            ("type", "release"),
            ("artist", &release_key.artist),
            ("release_title", &release_key.album),
        ],
        vec![
            ("type", "release"),
            ("title", &title_query),
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
        .unwrap();

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

fn calculate_changes<'a>(
    music_files: &'a Vec<MusicFile>,
    discogs_releases: &HashMap<ReleaseKey, DiscogsReleaseInfo>,
    target_path: &Path,
) -> Vec<MusicFileChange<'a>> {
    let mut result = Vec::with_capacity(music_files.len());

    for music_file in music_files {
        let release_info = discogs_releases.get(&release_key(music_file)).unwrap();
        let tag_from_discogs_info = tag_from_discogs_info(&music_file.tag, release_info);
        let target_path = PathBuf::from(target_path)
            .join(relative_file_path(
                &tag_from_discogs_info,
                music_file.file_path.extension().unwrap().to_str().unwrap(),
            ));

        result.push(MusicFileChange {
            bytes_to_transfer: fs::metadata(&music_file.file_path).unwrap().len(),
            source: music_file,
            target: MusicFile {
                file_path: target_path,
                tag: tag_from_discogs_info,
            },
        });
    }

    result
}

fn print_changes_details(changes: &Vec<MusicFileChange>) {
    for (index, change) in changes.iter().enumerate() {
        let source = change.source;
        let target = &change.target;

        let source_file_path = &source.file_path;
        let target_file_path = &target.file_path;
        let common_file_prefix = common_path::common_path(source_file_path, target_file_path).unwrap();
        println!(
            "{:02}. Copy \"{}\" -> \"{}\"",
            index + 1,
            source_file_path.strip_prefix(&common_file_prefix).unwrap().display(),
            target_file_path.strip_prefix(&common_file_prefix).unwrap().display(),
        );

        let source_tag = &source.tag;
        let target_tag = &target.tag;
        for target_frame in target_tag.frames() {
            let frame_id = target_frame.id();
            let source_frame_value = source_tag.get(frame_id).map(|v| v.content().to_string());
            let target_frame_value = Some(target_frame.content().to_string());
            if target_frame_value != source_frame_value {
                println!(
                    "    Change {}: \"{}\" -> \"{}\"",
                    target_frame.name(),
                    source_frame_value.unwrap_or(String::from("None")),
                    target_frame_value.unwrap_or(String::from("None")),
                );
            }
        }
    }
}

fn clean_release_folders(changes: &Vec<MusicFileChange>) {
    let mut paths = HashSet::new();

    for change in changes {
        let parent_path = PathBuf::from(change.target.file_path.parent().unwrap());
        paths.insert(parent_path);
    }

    for path in &paths {
        if fs::remove_dir_all(path).is_ok() {
            fs::create_dir_all(path).unwrap();
        }
    }
}

fn write_music_files(changes: &Vec<MusicFileChange>) {
    let total_bytes_to_transfer: u64 = changes.iter()
        .map(|v| v.bytes_to_transfer)
        .sum();

    let pb = default_progress_bar(total_bytes_to_transfer);

    for change in changes {
        let source = change.source;
        let target = &change.target;

        pb.set_message(format!("Copying \"{}\"", source.file_path.file_name().unwrap().to_str().unwrap()));

        fs::create_dir_all(target.file_path.parent().unwrap()).unwrap();

        let mut source_file = File::open(&source.file_path).unwrap();
        let mut target_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&target.file_path)
            .unwrap();

        io::copy(
            &mut source_file,
            &mut ProgressWriter::new(&mut target_file, |bytes| pb.inc(bytes as u64)),
        ).unwrap();

        target_file.seek(io::SeekFrom::Start(0)).unwrap();
        v1::Tag::remove(&mut target_file).unwrap();

        target_file.seek(io::SeekFrom::Start(0)).unwrap();
        target.tag.write_to(&mut target_file, Version::Id3v24).unwrap();
    }

    pb.finish_with_message(format!("Copied {} file(s)", changes.len()));
}

fn download_covers(
    http_client: &blocking::Client,
    headers: &HeaderMap,
    changes: &Vec<MusicFileChange>,
    discogs_releases: &HashMap<ReleaseKey, DiscogsReleaseInfo>,
) {
    let mut paths = HashMap::new();

    for change in changes {
        let release_key = release_key(change.source);
        let parent_path = PathBuf::from(change.target.file_path.parent().unwrap());
        paths.insert(release_key, parent_path);
    }

    let pb = default_progress_bar(!0);

    let mut cover_number = 0;

    for (index, (release_key, path)) in paths.iter().enumerate() {
        let discogs_release = discogs_releases.get(release_key).unwrap();
        let images_array = discogs_release.json["images"].as_array().unwrap();
        let cover_uri = images_array.iter()
            .find(|v| v["type"].as_str().unwrap() == "primary")
            .map(|v| v["uri"].as_str().unwrap().to_string())
            .or_else(|| {
                images_array.iter()
                    .find(|v| v["type"].as_str().unwrap() == "secondary")
                    .map(|v| v["uri"].as_str().unwrap().to_string())
            });
        if let Some(cover_uri) = cover_uri {
            cover_number += 1;
            pb.set_message(format!("Downloading cover {}/{}", index + 1, paths.len()));
            download_cover(http_client, headers, &cover_uri, path, &pb);
        }
    }

    pb.finish_with_message(format!("Downloaded {} cover(s)", cover_number))
}

fn download_cover(
    http_client: &blocking::Client,
    headers: &HeaderMap,
    uri: &str,
    folder_path: &Path,
    pb: &ProgressBar,
) {
    let mut response = http_client.get(uri)
        .headers(headers.clone())
        .send()
        .unwrap();

    let extension = match response.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap() {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        x => panic!("Undefined image content type: {}", x)
    };

    let cover_path = folder_path.join("cover").with_extension(OsStr::new(extension));

    let mut file = &mut ProgressWriter::new(
        std::fs::File::create(&cover_path).unwrap(),
        |bytes| pb.inc(bytes as u64),
    );

    pb.set_length(response.content_length().unwrap());
    pb.set_position(0);

    response
        .copy_to(&mut file)
        .unwrap();
}

fn tag_from_discogs_info(original_tag: &Tag, info: &DiscogsReleaseInfo) -> Tag {
    let release_object = &info.json;
    let track_number = original_tag.track().unwrap();
    let track_list_object = release_object["tracklist"].as_array().unwrap();
    let track_object = &track_list_object.iter()
        .find(|v|
            v["type_"].as_str().unwrap() == "track" &&
                v["position"].as_str().to_owned().unwrap().parse::<u32>().unwrap() == track_number
        )
        .unwrap();
    let album_artists = release_object["artists"].as_array().unwrap().iter()
        .map(|v| fix_discogs_artist_name(v["name"].as_str().unwrap().trim()))
        .collect::<Vec<&str>>();
    let track_artists = track_object["artists"].as_array()
        .map(|array| {
            array.iter()
                .map(|v| fix_discogs_artist_name(v["name"].as_str().unwrap().trim()))
                .collect::<Vec<&str>>()
        });

    let mut tag = Tag::new();
    tag.set_title(track_object["title"].as_str().unwrap().trim());
    tag.set_album(release_object["title"].as_str().unwrap().trim());
    tag.set_album_artist(
        if album_artists.len() > 1 {
            "Various Artists"
        } else {
            album_artists.get(0).unwrap()
        }
    );
    tag.set_artist(
        track_artists
            .or(Some(album_artists))
            .unwrap()
            .join(" & ")
    );
    tag.set_date_recorded(
        Timestamp {
            year: release_object["year"].as_i64().unwrap() as i32,
            month: None,
            day: None,
            hour: None,
            minute: None,
            second: None,
        }
    );
    tag.set_track(track_number);
    tag.set_total_tracks(track_list_object.len() as u32);
    tag.set_genre(
        release_object["styles"].as_array().unwrap().iter()
            .map(|v| v.as_str().unwrap().trim())
            .collect::<Vec<&str>>()
            .join("; ")
    );
    tag.add_frame(frame::ExtendedText {
        description: DISCOGS_RELEASE_ID_TAG.to_owned(),
        value: release_object["uri"].as_str().unwrap().to_owned(),
    });
    tag
}

fn get_discogs_token_file_path() -> Option<PathBuf> {
    Some(dirs::home_dir()?.join(DISCOGS_TOKEN_FILENAME))
}

fn common_headers(discogs_token: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::try_from("orgtag").unwrap());
    headers.insert(AUTHORIZATION, HeaderValue::try_from(format!("Discogs token={}", discogs_token)).unwrap());
    headers
}

fn relative_file_path(tag: &Tag, ext: &str) -> PathBuf {
    let mut path = relative_folder_path(tag);
    path.push(sanitize_path(match tag.disc() {
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
    }));
    path
}

fn relative_folder_path(tag: &Tag) -> PathBuf {
    let mut path = PathBuf::new();
    path.push(sanitize_path(tag.album_artist().unwrap()));
    path.push(sanitize_path(format!("({}) {}", tag.date_recorded().unwrap().year, tag.album().unwrap())));
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

