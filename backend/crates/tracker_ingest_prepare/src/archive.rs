use std::{
    fs::File,
    io::{Cursor, Read, Seek},
    path::Path,
};

use anyhow::{Context, Result, anyhow};
use zip::ZipArchive;

use crate::scan::{HeaderProbe, read_first_non_empty_line_from_reader};

const ARCHIVE_MEMBER_DELIMITER: &str = "!/";

pub(crate) struct ArchiveMemberScan {
    pub member_path: String,
    pub byte_size: i64,
    pub header_probe: HeaderProbe,
}

pub(crate) struct ArchiveRejectedScan {
    pub member_path: String,
    pub byte_size: i64,
    pub reason_text: String,
}

pub(crate) enum ArchiveScanEntry {
    Candidate(ArchiveMemberScan),
    Rejected(ArchiveRejectedScan),
}

pub fn decode_archive_member_path(member_path: &str) -> Result<Vec<String>> {
    if member_path.is_empty() {
        return Err(anyhow!("archive member path is empty"));
    }

    let segments = member_path
        .split(ARCHIVE_MEMBER_DELIMITER)
        .map(decode_segment)
        .collect::<Vec<_>>();
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(anyhow!("archive member path contains an empty segment"));
    }
    Ok(segments)
}

pub(crate) fn encode_archive_member_path(segments: &[String]) -> String {
    segments
        .iter()
        .map(|segment| encode_segment(segment))
        .collect::<Vec<_>>()
        .join(ARCHIVE_MEMBER_DELIMITER)
}

pub(crate) fn list_archive_members(path: &Path) -> Result<Vec<ArchiveScanEntry>> {
    let file =
        File::open(path).with_context(|| format!("failed to open archive `{}`", path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to read ZIP archive `{}`", path.display()))?;
    let mut members = Vec::new();

    collect_archive_members(&mut archive, &[], &mut members)?;

    Ok(members)
}

pub(crate) fn hash_archive_member(path: &Path, member_path: &str) -> Result<String> {
    let segments = decode_archive_member_path(member_path)?;
    let file =
        File::open(path).with_context(|| format!("failed to open archive `{}`", path.display()))?;
    hash_archive_member_from_reader(file, &segments, member_path)
}

fn collect_archive_members<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    parent_segments: &[String],
    output: &mut Vec<ArchiveScanEntry>,
) -> Result<()> {
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("failed to read ZIP entry #{index}"))?;
        if entry.is_dir() {
            continue;
        }

        let member_name = entry.name().to_string();
        let byte_size = entry.size() as i64;
        let mut locator_segments = parent_segments.to_vec();
        locator_segments.push(member_name.clone());
        let encoded_member_path = encode_archive_member_path(&locator_segments);

        if is_zip_member_path(&member_name) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).with_context(|| {
                format!("failed to read nested ZIP member `{encoded_member_path}`")
            })?;
            match ZipArchive::new(Cursor::new(bytes)) {
                Ok(mut nested_archive) => {
                    collect_archive_members(&mut nested_archive, &locator_segments, output)?;
                }
                Err(error) => {
                    output.push(ArchiveScanEntry::Rejected(ArchiveRejectedScan {
                        member_path: encoded_member_path,
                        byte_size,
                        reason_text: format!("Failed to read nested ZIP archive: {error}"),
                    }));
                }
            }
            continue;
        }

        let header_probe = read_first_non_empty_line_from_reader(entry)?;
        output.push(ArchiveScanEntry::Candidate(ArchiveMemberScan {
            member_path: encoded_member_path,
            byte_size,
            header_probe,
        }));
    }

    Ok(())
}

fn hash_archive_member_from_reader<R: Read + Seek>(
    reader: R,
    segments: &[String],
    member_path: &str,
) -> Result<String> {
    let (segment, tail) = segments
        .split_first()
        .ok_or_else(|| anyhow!("archive member path is empty"))?;
    let mut archive = ZipArchive::new(reader)
        .with_context(|| format!("failed to read ZIP archive for `{member_path}`"))?;
    let mut member = archive.by_name(segment).with_context(|| {
        format!("missing ZIP member `{segment}` while resolving `{member_path}`")
    })?;

    if tail.is_empty() {
        return super::scan::sha256_from_reader(member);
    }

    let mut bytes = Vec::new();
    member
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read nested ZIP member `{segment}`"))?;
    hash_archive_member_from_reader(Cursor::new(bytes), tail, member_path)
}

fn is_zip_member_path(member_path: &str) -> bool {
    Path::new(member_path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

fn encode_segment(segment: &str) -> String {
    segment
        .chars()
        .map(|ch| match ch {
            '%' => "%25".to_string(),
            '!' => "%21".to_string(),
            _ => ch.to_string(),
        })
        .collect()
}

fn decode_segment(segment: &str) -> String {
    let mut decoded = String::with_capacity(segment.len());
    let mut index = 0usize;

    while index < segment.len() {
        if let Some(encoded) = segment.get(index..index + 3) {
            match encoded {
                "%25" => {
                    decoded.push('%');
                    index += 3;
                    continue;
                }
                "%21" => {
                    decoded.push('!');
                    index += 3;
                    continue;
                }
                _ => {}
            }
        }

        let ch = segment[index..]
            .chars()
            .next()
            .expect("valid UTF-8 char boundary");
        decoded.push(ch);
        index += ch.len_utf8();
    }

    decoded
}
