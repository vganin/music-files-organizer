use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use itertools::Itertools;
use reqwest::Url;
use walkdir::WalkDir;

use crate::{AddCoversArguments, Console, console_print, DiscogsClient, pb_finish_with_message, pb_set_message, tag};
use crate::discogs::model::DiscogsRelease;
use crate::util::console_styleable::ConsoleStyleable;
use crate::util::path_extensions::PathExtensions;
use crate::util::r#const::{COVER_EXTENSIONS, COVER_FILE_NAME_WITHOUT_EXTENSION};

pub fn add_covers(
    args: AddCoversArguments,
    discogs_client: &DiscogsClient,
    console: &mut Console,
) -> Result<()> {
    let root_path = args.to;
    let pb = console.new_default_spinner();

    let mut downloaded_covers_count = 0;

    let directories = WalkDir::new(&root_path).into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_dir())
        .filter(|e| {
            let display_path = &e.path().strip_prefix_or_same(&root_path).display();

            pb_set_message!(pb, "Processing {}", display_path.path_styled());

            if !args.skip_if_present {
                return true;
            }

            let path = e.path();
            for extension in COVER_EXTENSIONS {
                if Path::exists(&path.join(COVER_FILE_NAME_WITHOUT_EXTENSION).with_extension(extension)) {
                    console_print!(console, "Skipped {}", display_path.path_styled());
                    return false;
                }
            }

            true
        });

    for directory in directories {
        let path = directory.path();
        let discogs_image = WalkDir::new(path)
            .contents_first(true)
            .max_depth(1)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| !e.file_type().is_dir())
            .filter_map(|e| {
                let path = e.path();
                let format = path.extension_or_empty();
                tag::read_from_path(path, format)
            })
            .map_ok(|tag| -> Result<Option<DiscogsRelease>> {
                discogs_client.fetch_release_by_meta(
                    &[tag.artist().context("No artist")?.to_string()],
                    tag.album().context("No album")?,
                    tag.title().context("No title")?,
                    tag.total_tracks().map(|v| v as usize),
                    console,
                )
            })
            .flatten()
            .filter_map_ok(|v| v)
            .map_ok(|discogs_release| discogs_release.best_image().map(ToOwned::to_owned))
            .filter_map_ok(|v| v)
            .next();

        if let Some(discogs_image) = discogs_image {
            let cover_uri = &discogs_image?.resource_url;
            let cover_uri_as_file_path = PathBuf::from(Url::parse(cover_uri)?.path());
            let cover_extension = cover_uri_as_file_path.extension().context("Expected extension for cover")?;
            let cover_file_name = PathBuf::from(COVER_FILE_NAME_WITHOUT_EXTENSION).with_extension(cover_extension);
            let cover_path = path.join(cover_file_name);
            let display_path = directory.path().strip_prefix_or_same(&root_path).display();

            pb_set_message!(pb, "Downloading cover to {}", display_path.path_styled());

            discogs_client.download_cover(cover_uri, &cover_path, &pb, console)?;

            downloaded_covers_count += 1;
        }
    }

    pb_finish_with_message!(pb, "{}", format!("Downloaded {} cover(s)", downloaded_covers_count).styled().green());

    Ok(())
}
