use std::fs;
use std::fs::metadata;
use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use id3::{Tag, TagLike};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(parse(from_os_str), value_name = "FILE")]
    file_path: PathBuf,
}

fn main() {
    let args = Args::parse();

    let file_metadata = metadata(&args.file_path).unwrap();

    if file_metadata.is_file() {
        inspect_file(&args.file_path)
    } else if file_metadata.is_dir() {
        for entry in fs::read_dir(args.file_path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            inspect_file(&path);
        }
    } else {
        panic!("Given path is not a file nor a directory")
    }
}

fn inspect_file(path: impl AsRef<Path>) {
    let tag = match Tag::read_from_path(path) {
        Ok(tag) => tag,
        Err(error) => {
            println!("Can't read tags: {}", error);
            return;
        }
    };

    // Get a bunch of frames...
    if let Some(artist) = tag.artist() {
        println!("artist: {}", artist);
    }
    if let Some(title) = tag.title() {
        println!("title: {}", title);
    }
    if let Some(album) = tag.album() {
        println!("album: {}", album);
    }

    // Get frames before getting their content for more complex tags.
    if let Some(artist) = tag.get("TPE1").and_then(|frame| frame.content().text()) {
        println!("artist: {}", artist);
    }
}
