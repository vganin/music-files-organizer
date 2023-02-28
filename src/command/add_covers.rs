use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use reqwest::Url;
use walkdir::WalkDir;

use DiscogsReleaseMatchResult::{Matched, Unmatched};

use crate::{AddCoversArguments, Console, console_print, DiscogsMatcher, pb_finish_with_message, pb_set_message};
use crate::discogs::matcher::DiscogsReleaseMatchResult;
use crate::music_file::MusicFile;
use crate::util::console_styleable::ConsoleStyleable;
use crate::util::path_extensions::PathExtensions;
use crate::util::r#const::{COVER_EXTENSIONS, COVER_FILE_NAME_WITHOUT_EXTENSION};

pub fn add_covers(
    args: AddCoversArguments,
    discogs_matcher: &DiscogsMatcher,
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
        let first_music_file = WalkDir::new(path)
            .contents_first(true)
            .max_depth(1)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| !e.file_type().is_dir())
            .map(|e| MusicFile::from_path(e.path()))
            .next()
            .map_or(Ok(None), |r| r.map(Some))?
            .flatten();

        let Some(first_music_file) = first_music_file else {
            // No valid tags in directory, skipping
            continue;
        };

        let discogs_image = discogs_matcher.match_music_files([first_music_file].iter(), console)?.first()
            .and_then(|discogs_match_result| {
                match discogs_match_result {
                    Matched { release, .. } => release.image.as_ref().map(ToOwned::to_owned),
                    Unmatched(_) => None
                }
            });

        if let Some(discogs_image) = discogs_image {
            let cover_uri = &discogs_image.url;
            let cover_uri_as_file_path = PathBuf::from(Url::parse(cover_uri)?.path());
            let cover_extension = cover_uri_as_file_path.extension().context("Expected extension for cover")?;
            let cover_file_name = PathBuf::from(COVER_FILE_NAME_WITHOUT_EXTENSION).with_extension(cover_extension);
            let cover_path = path.join(cover_file_name);
            let display_path = directory.path().strip_prefix_or_same(&root_path).display();

            pb_set_message!(pb, "Downloading cover to {}", display_path.path_styled());

            discogs_matcher.download_cover(cover_uri, &cover_path, &pb, console)?;

            downloaded_covers_count += 1;
        } else {
            console_print!(console, "{}", format!("Failed to fetch cover for {}", path.display().path_styled()).error_styled());
        }
    }

    pb_finish_with_message!(pb, "{}", format!("Downloaded {} cover(s)", downloaded_covers_count).styled().green());

    Ok(())
}
