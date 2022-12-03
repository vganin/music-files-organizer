use std::path::Path;

pub trait PathExtensions {
    fn file_name_or_empty(&self) -> &str;
    fn extension_or_empty(&self) -> &str;
    fn strip_prefix_or_same(&self, base: impl AsRef<Path>) -> &Path;
    fn parent_or_empty(&self) -> &Path;
}

impl PathExtensions for Path {
    fn file_name_or_empty(&self) -> &str {
        self.file_name().unwrap_or_default().to_str().unwrap_or_default()
    }

    fn extension_or_empty(&self) -> &str {
        self.extension().unwrap_or_default().to_str().unwrap_or_default()
    }

    fn strip_prefix_or_same(&self, base: impl AsRef<Path>) -> &Path {
        self.strip_prefix(base).unwrap_or_else(|_| Path::new(""))
    }

    fn parent_or_empty(&self) -> &Path {
        self.parent().unwrap_or_else(|| Path::new(""))
    }
}
