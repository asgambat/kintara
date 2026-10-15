//! Metadata extraction and thumbnail generation.
//!
//! Both are done by shelling out to poppler (`pdfinfo`, `pdftoppm`) rather than
//! linking a PDF library. The native Rust options each have a catch: pdfium
//! means shipping a shared library alongside the binary, and the mupdf bindings
//! are AGPL, which conflicts with this project's Apache-2.0 licence. poppler is
//! one apt line in the container and does both jobs.
//!
//! This module is used by upload today and by the filesystem scanner later, so
//! a document gets the same treatment however it arrives.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub keywords: Option<String>,
    pub page_count: Option<i64>,
    pub year: Option<i64>,
}

/// Content hash, used to detect edits and to spot the same file arriving twice
/// under different names.
pub fn hash_bytes(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Reads PDF metadata via `pdfinfo`.
///
/// A failure here is never fatal — a PDF with no metadata, or a missing
/// poppler, should still produce a usable library entry with a filename-derived
/// title. Callers get an empty Metadata rather than an error.
pub async fn extract_pdf_metadata(path: &Path, poppler_timeout: Duration) -> Metadata {
    let output = tokio::time::timeout(
        poppler_timeout,
        Command::new("pdfinfo")
            .kill_on_drop(true)
            .arg(path)
            .stdin(Stdio::null())
            .output(),
    )
    .await;

    let Ok(output) = output else {
        tracing::warn!(path = %path.display(), "pdfinfo exceeded its timeout and was killed");
        return Metadata::default();
    };

    let Ok(output) = output else {
        tracing::debug!("pdfinfo unavailable; skipping metadata extraction");
        return Metadata::default();
    };

    if !output.status.success() {
        tracing::debug!(path = %path.display(), "pdfinfo returned a non-zero status");
        return Metadata::default();
    }

    parse_pdfinfo(&String::from_utf8_lossy(&output.stdout))
}

/// Parses `pdfinfo` key/value output.
///
/// Split out from the process call so it can be tested against real captured
/// output without needing poppler present.
pub fn parse_pdfinfo(text: &str) -> Metadata {
    let mut metadata = Metadata::default();

    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if value.is_empty() {
            continue;
        }

        match key.trim() {
            "Title" => metadata.title = Some(value.to_string()),
            "Author" => metadata.author = Some(value.to_string()),
            "Keywords" => metadata.keywords = Some(value.to_string()),
            "Pages" => metadata.page_count = value.parse().ok(),
            // "Sun Jun 11 20:00:00 2017 EDT". The year is not reliably the last
            // token — poppler appends a timezone when the PDF carries one — so
            // scan for the first plausible four-digit year instead of indexing
            // by position. Parsing the full date is not worth it for a year field.
            "CreationDate" => {
                metadata.year = value
                    .split_whitespace()
                    .filter_map(|token| token.parse::<i64>().ok())
                    .find(|year| (1400..=9999).contains(year));
            }
            _ => {}
        }
    }

    metadata
}

/// Renders page 1 of a PDF to a JPEG thumbnail, returning the filename written
/// into `thumbnail_dir`.
///
/// Returns None rather than erroring when poppler is missing or the render
/// fails — a library entry without a cover is fine, a failed import is not.
pub async fn generate_thumbnail(
    source: &Path,
    thumbnail_dir: &Path,
    document_id: i64,
    poppler_timeout: Duration,
) -> Option<String> {
    // The id in the name means a regenerated thumbnail lands on a new URL, so
    // the aggressive cache header on the thumbnail route stays correct.
    let stem = format!("doc-{document_id}-{}", chrono::Utc::now().timestamp());
    let prefix = thumbnail_dir.join(&stem);
    let filename = format!("{stem}.jpg");

    let result = tokio::time::timeout(
        poppler_timeout,
        Command::new("pdftoppm")
            .kill_on_drop(true)
            .args(["-jpeg", "-r", "72", "-f", "1", "-l", "1", "-singlefile"])
            .arg(source)
            .arg(&prefix)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
    )
    .await;

    let status = match result {
        Ok(Ok(status)) => Some(status),
        Ok(Err(_)) => {
            tracing::debug!("pdftoppm unavailable; skipping thumbnail");
            return None;
        }
        Err(_) => {
            tracing::warn!(
                source = %source.display(),
                timeout_secs = poppler_timeout.as_secs(),
                "pdftoppm exceeded its timeout and was killed"
            );
            None
        }
    };

    // A failed or killed run may still have written a partial JPEG. Nothing
    // points at it (the caller only records a name below on success), so it
    // would live in thumbnail_dir forever — remove it here instead.
    if status.is_none_or(|status| !status.success()) {
        let _ = tokio::fs::remove_file(thumbnail_dir.join(&filename)).await;
        tracing::debug!(source = %source.display(), "pdftoppm failed");
        return None;
    }

    if thumbnail_dir.join(&filename).exists() {
        Some(filename)
    } else {
        None
    }
}

/// Derives a display title from a filename when the document has no embedded
/// title: strips the extension and tidies separators.
pub fn title_from_filename(filename: &str) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    let cleaned = stem.replace(['_', '-'], " ");
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");

    if cleaned.is_empty() {
        filename.to_string()
    } else {
        cleaned
    }
}

/// Strips directory components and anything that would let an uploaded name
/// escape the library root or collide with a hidden file.
pub fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("document");

    let cleaned: String = base
        .chars()
        .map(|c| match c {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();

    let cleaned = cleaned.trim_start_matches('.').trim().to_string();

    if cleaned.is_empty() {
        "document".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_pdfinfo_output() {
        // Captured from poppler 26.07.
        let text = "\
Title:           Attention Is All You Need
Subject:
Author:          Vaswani et al
Keywords:        transformers attention
Creator:         LaTeX
CreationDate:    Fri Aug 10 06:46:51 2026
Pages:           3
Page size:       612 x 792 pts (letter)
PDF version:     1.4";

        let metadata = parse_pdfinfo(text);
        assert_eq!(metadata.title.as_deref(), Some("Attention Is All You Need"));
        assert_eq!(metadata.author.as_deref(), Some("Vaswani et al"));
        assert_eq!(metadata.keywords.as_deref(), Some("transformers attention"));
        assert_eq!(metadata.page_count, Some(3));
        assert_eq!(metadata.year, Some(2026));
    }

    #[test]
    fn empty_fields_are_left_unset_rather_than_blank() {
        let metadata = parse_pdfinfo("Title:\nAuthor:   \nPages: 2");
        assert_eq!(metadata.title, None);
        assert_eq!(metadata.author, None);
        assert_eq!(metadata.page_count, Some(2));
    }

    #[test]
    fn a_nonsense_creation_date_does_not_become_a_year() {
        let metadata = parse_pdfinfo("CreationDate:    garbage");
        assert_eq!(metadata.year, None);
    }

    #[test]
    fn a_creation_date_with_a_timezone_still_yields_a_year() {
        // Captured from poppler 26.07 for a PDF carrying a timezone. The year
        // is not the last token here, which a naive parser gets wrong.
        let metadata = parse_pdfinfo("CreationDate:    Sun Jun 11 20:00:00 2017 EDT");
        assert_eq!(metadata.year, Some(2017));
    }

    #[test]
    fn a_time_only_creation_date_does_not_yield_a_bogus_year() {
        let metadata = parse_pdfinfo("CreationDate:    20:00:00");
        assert_eq!(metadata.year, None);
    }

    #[test]
    fn titles_are_derived_from_filenames_readably() {
        assert_eq!(title_from_filename("attention_is_all.pdf"), "attention is all");
        assert_eq!(title_from_filename("my-paper.pdf"), "my paper");
        assert_eq!(title_from_filename("notes.md"), "notes");
    }

    #[test]
    fn uploaded_filenames_cannot_escape_the_library() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("/absolute/path.pdf"), "path.pdf");
        assert_eq!(sanitize_filename(".hidden"), "hidden");
        assert_eq!(sanitize_filename(""), "document");
        assert_eq!(sanitize_filename("a:b*c?.pdf"), "a_b_c_.pdf");
    }

    /// A run that outlives its timeout (real; a wedged poppler) must leave no
    /// partial JPEG behind — acceptance of TODO T06 "no orphan thumbnails".
    #[tokio::test]
    async fn a_thumbnail_that_hits_the_timeout_writes_no_file() {
        use std::time::Duration;

        let dir = tempfile::tempdir().expect("tempdir");
        let pdf = dir.path().join("sample.pdf");
        std::fs::write(&pdf, b"%PDF-1.4 placeholder").expect("write pdf");

        let name =
            generate_thumbnail(&pdf, dir.path(), 1, Duration::from_millis(1)).await;
        assert!(name.is_none(), "the run must be given up, not completed");
        assert!(
            std::fs::read_dir(dir.path())
                .expect("list dir")
                .filter_map(|entry| entry.ok())
                .all(|entry| entry.path().extension().is_none_or(|ext| ext != "jpg")),
            "no partial .jpg may remain"
        );
    }

    #[test]
    fn hashing_is_stable_and_distinguishes_content() {
        assert_eq!(hash_bytes(b"abc"), hash_bytes(b"abc"));
        assert_ne!(hash_bytes(b"abc"), hash_bytes(b"abd"));
    }
}
