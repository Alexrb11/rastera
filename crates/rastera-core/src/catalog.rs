use crate::{ErrorKind, ImageInfo, RasteraError, Result};
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

#[derive(Clone, Debug, Default)]
pub struct CatalogEdit {
    pub favorite: Option<bool>,
    pub tags: Vec<String>,
    pub remove_tags: bool,
}

impl CatalogEdit {
    pub fn apply(&self, annotation: &Annotation) -> Result<Annotation> {
        if self.tags.len() > 32
            || self.tags.iter().any(|tag| {
                tag.trim().is_empty()
                    || tag.chars().count() > 64
                    || tag.chars().any(char::is_control)
            })
        {
            return Err(RasteraError::new(
                ErrorKind::TooLarge,
                "Usa hasta 32 etiquetas de 64 caracteres, sin caracteres de control",
            ));
        }
        let mut updated = annotation.clone();
        if let Some(favorite) = self.favorite {
            updated.favorite = favorite;
        }
        if self.remove_tags {
            updated.tags.retain(|tag| !self.tags.contains(tag));
        } else if !self.tags.is_empty() {
            updated.tags.extend(self.tags.iter().cloned());
            updated.tags.sort();
            updated.tags.dedup();
            if updated.tags.len() > 32 {
                return Err(RasteraError::new(
                    ErrorKind::TooLarge,
                    "La imagen superaría el límite de 32 etiquetas; no se ha guardado la selección",
                ));
            }
        }
        Ok(updated)
    }
}

pub trait CatalogBackend: Send + 'static {
    fn load(&mut self, paths: &[PathBuf]) -> Result<Vec<(PathBuf, Annotation)>>;
    fn save(
        &mut self,
        path: &Path,
        annotation: &Annotation,
        info: Option<&ImageInfo>,
    ) -> Result<()>;
    /// Apply to the current stored annotations, atomically for all paths.
    fn edit(&mut self, paths: &[PathBuf], edit: &CatalogEdit)
    -> Result<Vec<(PathBuf, Annotation)>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_merge_or_remove_tags_without_replacing_other_annotation_fields() {
        let original = Annotation {
            favorite: true,
            tags: vec!["viaje".into(), "paisaje".into()],
            width: Some(800),
            height: Some(600),
            format: Some("JPEG".into()),
        };
        let add = CatalogEdit {
            tags: vec!["paisaje".into(), "aprobada".into()],
            ..Default::default()
        };
        assert_eq!(CatalogEdit::default().apply(&original).unwrap(), original);
        let updated = add.apply(&original).unwrap();
        assert_eq!(updated.tags, ["aprobada", "paisaje", "viaje"]);
        assert!(updated.favorite);
        assert_eq!(
            (updated.width, updated.height, updated.format.as_deref()),
            (Some(800), Some(600), Some("JPEG"))
        );
        let remove = CatalogEdit {
            favorite: Some(false),
            tags: vec!["paisaje".into()],
            remove_tags: true,
        };
        let updated = remove.apply(&updated).unwrap();
        assert_eq!(updated.tags, ["aprobada", "viaje"]);
        assert!(!updated.favorite);
        let full = Annotation {
            tags: (0..32).map(|index| format!("tag-{index}")).collect(),
            ..Default::default()
        };
        assert!(add.apply(&full).is_err());
        assert_eq!(full.tags.len(), 32);
        assert!(
            CatalogEdit {
                tags: vec!["bad\nlabel".into()],
                ..Default::default()
            }
            .apply(&original)
            .is_err()
        );
    }
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
