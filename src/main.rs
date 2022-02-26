use std::{fs, io};
use std::ffi::OsStr;
use std::fs::metadata;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use id3::{Tag, TagLike, Timestamp, v1, Version};
use reqwest::{blocking, Url};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, USER_AGENT};

static USER_AGENT_VALUE: &str = "orgtag";

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(parse(from_os_str), value_name = "INPUT FILE")]
    input_file_path: PathBuf,

    #[clap(parse(from_os_str), value_name = "OUTPUT FILE")]
    output_file_path: PathBuf,

    #[clap(short, long)]
    discogs_token: String,

    #[clap(short, long)]
    clean: bool,
}

struct MusicFile {
    file_path: PathBuf,
    tag: Tag,
}

fn main() {
    let args = Args::parse();

    if args.input_file_path == args.output_file_path {
        panic!("Input and output paths should not be the same")
    }

    if !metadata(&args.output_file_path).unwrap().is_dir() {
        panic!("Output path is not directory")
    }

    if args.clean {
        fs::remove_dir_all(&args.output_file_path).unwrap();
        fs::create_dir(&args.output_file_path).unwrap();
    }

    let music_files = inspect_path(&args.input_file_path);

    let http_client = blocking::Client::new();

    for music_file in music_files {
        let source_path = music_file.file_path;
        let tag = music_file.tag;
        let extension = source_path.extension().unwrap().to_str().unwrap();

        let mut target_path = PathBuf::from(&args.output_file_path);
        target_path.push(tag.artist().unwrap());
        target_path.push(format!("({}) {}", tag.year().unwrap(), tag.album().unwrap()));
        match tag.disc() {
            Some(disc) => target_path.push(format!(
                "{disc:02}.{track:02}. {title}.{ext}",
                disc = disc,
                track = tag.track().unwrap(),
                title = tag.title().unwrap(),
                ext = extension,
            )),
            None => target_path.push(format!(
                "{track:02}. {title}.{ext}",
                track = tag.track().unwrap(),
                title = tag.title().unwrap(),
                ext = extension,
            )),
        }

        println!("Will copy \"{}\" to \"{}\"", source_path.display(), target_path.display());

        fs::create_dir_all(target_path.parent().unwrap()).unwrap();
        fs::copy(&source_path, &target_path).unwrap();

        {
            let mut file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&target_path)
                .unwrap();
            file.seek(io::SeekFrom::Start(0)).unwrap();
            v1::Tag::remove(&mut file).unwrap();
        }

        let (improved_tag, image_uri) = discogs_tag_and_image_uri(&tag, &http_client, &args.discogs_token);
        improved_tag.write_to_path(&target_path, Version::Id3v24).unwrap();

        if let Some(uri) = image_uri {
            let mut response = http_client.get(&uri)
                .header(USER_AGENT, USER_AGENT_VALUE)
                .header(AUTHORIZATION, &args.discogs_token)
                .send()
                .unwrap();

            let extension = match response.headers().get(CONTENT_TYPE).unwrap().to_str().unwrap() {
                "image/jpeg" => "jpg",
                "image/png" => "png",
                _ => panic!()
            };

            let cover_path = target_path.parent().unwrap()
                .join("cover").with_extension(OsStr::new(extension));

            println!("Will use {} as a cover: {}", &uri, cover_path.display());

            response
                .copy_to(&mut std::fs::File::create(&cover_path).unwrap())
                .unwrap();
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
    println!("Inspecting file {}", path.as_ref().display());
    match Tag::read_from_path(&path) {
        Ok(tag) => Some(MusicFile {
            file_path: PathBuf::from(path.as_ref()),
            tag,
        }),
        Err(error) => {
            println!("Can't read tags: {}", error);
            return None;
        }
    }
}

fn discogs_tag_and_image_uri(original_tag: &Tag, http_client: &blocking::Client, token: &str) -> (Tag, Option<String>) {
    let base_url = Url::parse("https://api.discogs.com/").unwrap();
    let auth_value = format!("Discogs token={}", token);

    let search_object = http_client
        .get(
            Url::parse_with_params(base_url.join("database/search").unwrap().as_str(), &[
                ("type", "release"),
                ("artist", original_tag.artist().unwrap()),
                ("release_title", original_tag.album().unwrap()),
            ]).unwrap()
        )
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(AUTHORIZATION, &auth_value)
        .send()
        .unwrap()
        .json::<serde_json::Value>()
        .unwrap()
        .clone();

    let release_object = http_client
        .get(search_object["results"][0]["resource_url"].as_str().unwrap())
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(AUTHORIZATION, &auth_value)
        .send()
        .unwrap()
        .json::<serde_json::Value>()
        .unwrap()
        .clone();

    let track_number = original_tag.track().unwrap();
    let track_index = (track_number as usize) - 1;
    let track_object = &release_object["tracklist"][track_index];
    let artists = release_object["artists"].as_array().unwrap();

    let mut new_tag = Tag::new();
    new_tag.set_title(track_object["title"].as_str().unwrap());
    new_tag.set_album(release_object["title"].as_str().unwrap());
    new_tag.set_artist(artists.iter().map(|v| v["name"].as_str().unwrap()).collect::<Vec<&str>>().join(" & "));
    new_tag.set_album_artist(artists.iter().map(|v| v["name"].as_str().unwrap()).collect::<Vec<&str>>().join(" & "));
    new_tag.set_date_recorded(Timestamp {
        year: release_object["year"].as_i64().unwrap() as i32,
        month: None,
        day: None,
        hour: None,
        minute: None,
        second: None,
    });
    new_tag.set_track(track_number);
    new_tag.set_genre(release_object["styles"].as_array().unwrap()
        .iter().map(|v| v.as_str().unwrap()).collect::<Vec<&str>>().join("; "));

    let primary_image_uri = release_object["images"].as_array().unwrap().iter()
        .find(|v| v["type"].as_str().unwrap() == "primary")
        .map(|v| v["uri"].as_str().unwrap().to_string())
        .clone();

    (new_tag, primary_image_uri)
}
