use std::path::{Path, PathBuf};

use anyhow::Result;
use reqwest::Url;
use walkdir::WalkDir;

use crate::{AddMissingCoversArgs, Console, DiscogsClient, pb_finish_with_message, pb_set_message, tag};
use crate::util::console_styleable::ConsoleStyleable;
use crate::util::r#const::{COVER_EXTENSIONS, COVER_FILE_NAME_WITHOUT_EXTENSION};

pub fn add_missing_covers(args: AddMissingCoversArgs, discogs_client: &DiscogsClient, console: &mut Console) -> Result<()> {
    let root_path = args.to;
    let pb = console.new_default_spinner();
    let mut downloaded_covers_count = 0;

    for directory in WalkDir::new(&root_path).into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
        .inspect(|e| {
            let display_path = e.path().strip_prefix(&root_path).unwrap().display();
            pb_set_message!(pb, "Processing {}", display_path.path_styled());
        })
        .filter(|e| {
            if args.force_update {
                return true;
            }

            let path = e.path();
            for extension in COVER_EXTENSIONS {
                if Path::exists(&path.join(COVER_FILE_NAME_WITHOUT_EXTENSION).with_extension(extension)) {
                    return false;
                }
            }

            true
        })
    {
        let path = directory.path();
        if let Some(discogs_image) = WalkDir::new(path)
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
            .filter_map(Result::ok)
            .next()
            .and_then(|tag| {
                discogs_client.fetch_release_by_meta(
                    &[tag.artist().unwrap().to_string()],
                    tag.album().unwrap(),
                    tag.title().unwrap(),
                    tag.total_tracks().unwrap() as usize,
                    console,
                ).ok()
            })
            .flatten()
            .and_then(|discogs_release| discogs_release.best_image().map(ToOwned::to_owned))
        {
            let cover_uri = &discogs_image.resource_url;
            let cover_uri_as_file_path = PathBuf::from(Url::parse(cover_uri).unwrap().path());
            let cover_extension = cover_uri_as_file_path.extension().unwrap();
            let cover_file_name = PathBuf::from(COVER_FILE_NAME_WITHOUT_EXTENSION).with_extension(cover_extension);
            let cover_path = path.join(cover_file_name);
            let display_path = directory.path().strip_prefix(&root_path).unwrap().display();

            pb_set_message!(pb, "Downloading cover to {}", display_path.path_styled());

            discogs_client.download_cover(&cover_uri, &cover_path, &pb, console)?;

            downloaded_covers_count += 1;
        }
    }

    pb_finish_with_message!(pb, "{}", format!("Downloaded {} cover(s)", downloaded_covers_count).styled().green());

    Ok(())
}
