use crate::{ImageInfo, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Annotation {
    pub favorite: bool,
    pub tags: Vec<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub format: Option<String>,
}

pub fn normalize_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = text
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(|tag| tag.chars().filter(|c| !c.is_control()).take(64).collect())
        .take(32)
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

pub trait CatalogBackend: Send + 'static {
    fn load(&mut self, paths: &[PathBuf]) -> Result<Vec<(PathBuf, Annotation)>>;
    fn save(
        &mut self,
        path: &Path,
        annotation: &Annotation,
        info: Option<&ImageInfo>,
    ) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tags_are_bounded_trimmed_and_deduplicated() {
        assert_eq!(
            normalize_tags(" Viajes, , Familia, Viajes,\n"),
            ["Familia", "Viajes"]
        );
        assert_eq!(normalize_tags(&"a".repeat(100))[0].len(), 64);
        assert!(
            normalize_tags(
                &(0..100)
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
            .len()
                <= 32
        );
    }
}
