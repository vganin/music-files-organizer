use std::fmt::{Display, Formatter};
use std::str::FromStr;

use anyhow::{bail, Result};

pub enum FrameId {
    Title,
    Album,
    AlbumArtist,
    Artist,
    Year,
    Track,
    TotalTracks,
    Disc,
    TotalDiscs,
    Genre,
    CustomText { key: String },
}

#[derive(PartialEq, Eq, Debug)]
pub enum FrameContent {
    Str(String),
    I32(i32),
    U32(u32),
}

impl FrameContent {
    pub fn as_str(&self) -> Result<&str> {
        match self {
            FrameContent::Str(v) => Ok(v),
            _ => bail!("Value is not a string"),
        }
    }

    pub fn as_i32(&self) -> Result<i32> {
        match self {
            FrameContent::I32(v) => Ok(*v),
            _ => bail!("Value is not a signed integer"),
        }
    }

    pub fn as_u32(&self) -> Result<u32> {
        match self {
            FrameContent::U32(v) => Ok(*v),
            _ => bail!("Value is not an unsigned integer"),
        }
    }
}

impl Display for FrameId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                FrameId::Title => "Title",
                FrameId::Album => "Album",
                FrameId::AlbumArtist => "Album Artist",
                FrameId::Artist => "Artist",
                FrameId::Year => "Year",
                FrameId::Track => "Track",
                FrameId::TotalTracks => "Total Tracks",
                FrameId::Disc => "Disc",
                FrameId::TotalDiscs => "Total Discs",
                FrameId::Genre => "Genre",
                FrameId::CustomText { key } => key,
            }
        )
    }
}

impl FromStr for FrameId {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "Title" => FrameId::Title,
            "Album" => FrameId::Album,
            "Album Artist" => FrameId::AlbumArtist,
            "Artist" => FrameId::Artist,
            "Year" => FrameId::Year,
            "Track" => FrameId::Track,
            "Total Tracks" => FrameId::TotalTracks,
            "Disc" => FrameId::Disc,
            "Total Discs" => FrameId::TotalDiscs,
            "Genre" => FrameId::Genre,
            key => FrameId::CustomText {
                key: key.to_owned(),
            },
        })
    }
}

impl ToString for FrameContent {
    fn to_string(&self) -> String {
        match self {
            FrameContent::Str(v) => v.to_owned(),
            FrameContent::I32(v) => v.to_string(),
            FrameContent::U32(v) => v.to_string(),
        }
    }
}
