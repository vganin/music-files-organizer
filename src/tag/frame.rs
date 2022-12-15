use std::fmt::{Display, Formatter};

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
            _ => bail!("Value is not a string")
        }
    }

    pub fn as_i32(&self) -> Result<i32> {
        match self {
            FrameContent::I32(v) => Ok(*v),
            _ => bail!("Value is not a signed integer")
        }
    }

    pub fn as_u32(&self) -> Result<u32> {
        match self {
            FrameContent::U32(v) => Ok(*v),
            _ => bail!("Value is not an unsigned integer")
        }
    }
}

impl Display for FrameId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", match self {
            FrameId::Title => "Title",
            FrameId::Album => "Album",
            FrameId::AlbumArtist => "Album Artist",
            FrameId::Artist => "Artist",
            FrameId::Year => "Year",
            FrameId::Track => "Track",
            FrameId::TotalTracks => "Total Tracks",
            FrameId::Disc => "Disc",
            FrameId::Genre => "Genre",
            FrameId::CustomText { key } => key,
        })
    }
}

impl FrameContent {
    pub fn stringify_content(&self) -> String {
        match self {
            FrameContent::Str(v) => v.to_owned(),
            FrameContent::I32(v) => v.to_string(),
            FrameContent::U32(v) => v.to_string(),
        }
    }
}
