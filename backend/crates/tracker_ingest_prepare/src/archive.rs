use std::{fs::File, path::Path};

use anyhow::{Context, Result};
use zip::ZipArchive;

use crate::scan::{HeaderProbe, read_first_non_empty_line_from_reader};

pub(crate) struct ArchiveMemberScan {
    pub member_path: String,
    pub byte_size: i64,
    pub header_probe: HeaderProbe,
}

pub(crate) fn list_archive_members(path: &Path) -> Result<Vec<ArchiveMemberScan>> {
    let file =
        File::open(path).with_context(|| format!("failed to open archive `{}`", path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to read ZIP archive `{}`", path.display()))?;
    let mut members = Vec::new();

    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .with_context(|| format!("failed to read ZIP entry #{index}"))?;
        if entry.is_dir() {
            continue;
        }

        let member_path = entry.name().to_string();
        let byte_size = entry.size() as i64;
        let header_probe = read_first_non_empty_line_from_reader(entry)?;
        members.push(ArchiveMemberScan {
            member_path,
            byte_size,
            header_probe,
        });
    }

    Ok(members)
}

pub(crate) fn hash_archive_member(path: &Path, member_path: &str) -> Result<String> {
    let file =
        File::open(path).with_context(|| format!("failed to open archive `{}`", path.display()))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("failed to read ZIP archive `{}`", path.display()))?;
    let member = archive
        .by_name(member_path)
        .with_context(|| format!("missing ZIP member `{member_path}`"))?;
    super::scan::sha256_from_reader(member)
}
