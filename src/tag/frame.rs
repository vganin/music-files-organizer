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

#[derive(PartialEq)]
pub enum FrameContent {
    Str(String),
    I32(i32),
    U32(u32),
}

impl FrameContent {
    pub fn as_str(&self) -> Option<&str> {
        match self {
            FrameContent::Str(v) => Some(v),
            _ => None
        }
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            FrameContent::I32(v) => Some(*v),
            _ => None
        }
    }

    pub fn as_u32(&self) -> Option<u32> {
        match self {
            FrameContent::U32(v) => Some(*v),
            _ => None
        }
    }
}

impl FrameId {
    pub fn description(&self) -> &str {
        match self {
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
        }
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
