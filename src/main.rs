use std::fs;
use std::fs::metadata;
use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use id3::{Tag, TagLike};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(parse(from_os_str), value_name = "INPUT FILE")]
    input_file_path: PathBuf,

    #[clap(parse(from_os_str), value_name = "OUTPUT FILE")]
    output_file_path: PathBuf,
}

struct MusicFile {
    file_path: PathBuf,
    tag: Tag,
}

fn main() {
    let args = Args::parse();

    if !metadata(&args.output_file_path).unwrap().is_dir() {
        panic!("Output path is not directory")
    }

    let music_files = inspect_path(&args.input_file_path);

    for music_file in music_files {
        let tag = music_file.tag;
        let mut path = PathBuf::from(&args.output_file_path);

        path.push(tag.artist().unwrap());
        path.push(format!("({}) {}", tag.year().unwrap(), tag.album().unwrap()));
        match tag.disc() {
            Some(disc) => path.push(format!("{}.{}. {}", disc, tag.track().unwrap(), tag.title().unwrap())),
            None => path.push(format!("{}. {}", tag.track().unwrap(), tag.title().unwrap())),
        }

        println!("Will write {}", path.display());
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
