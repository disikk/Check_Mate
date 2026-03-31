use std::{
    collections::BTreeMap,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use tracker_ingest_runtime::{
    ClaimedJob as IngestClaimedJob, FileKind as IngestFileKind, JobExecutionError,
};

#[derive(Default)]
pub(crate) struct ArchiveReaderCache {
    archives: BTreeMap<PathBuf, zip::ZipArchive<Cursor<Vec<u8>>>>,
}

impl ArchiveReaderCache {
    pub(crate) fn read_member_bytes(&mut self, archive_path: &Path, member_path: &str) -> Result<Vec<u8>> {
        let segments = tracker_ingest_prepare::decode_archive_member_path(member_path)?;
        let (segment, tail) = segments
            .split_first()
            .ok_or_else(|| anyhow!("archive member path is empty"))?;

        let mut bytes = Vec::new();
        self.archive_mut(archive_path, member_path)?
            .by_name(segment)
            .with_context(|| {
                format!("missing ZIP member `{segment}` while resolving `{member_path}`")
            })?
            .read_to_end(&mut bytes)
            .with_context(|| format!("failed to read archive member `{segment}`"))?;

        if tail.is_empty() {
            return Ok(bytes);
        }

        read_archive_member_bytes_from_reader(Cursor::new(bytes), tail, member_path)
    }

    fn archive_mut(
        &mut self,
        archive_path: &Path,
        member_path: &str,
    ) -> Result<&mut zip::ZipArchive<Cursor<Vec<u8>>>> {
        let key = archive_path.to_path_buf();
        if !self.archives.contains_key(&key) {
            let bytes = std::fs::read(archive_path)
                .with_context(|| format!("failed to open archive `{}`", archive_path.display()))?;
            let archive = zip::ZipArchive::new(Cursor::new(bytes))
                .with_context(|| format!("failed to open ZIP `{member_path}`"))?;
            self.archives.insert(key.clone(), archive);
        }

        Ok(self
            .archives
            .get_mut(&key)
            .expect("archive cache entry inserted before lookup"))
    }
}

pub(crate) fn read_archive_member_text(
    archive_reader_cache: &mut ArchiveReaderCache,
    path: &str,
    member_path: &str,
) -> Result<String> {
    let bytes = archive_reader_cache.read_member_bytes(Path::new(path), member_path)?;
    String::from_utf8(bytes)
        .with_context(|| format!("failed to read ZIP member `{member_path}` as UTF-8 text"))
}

pub(crate) fn read_archive_member_bytes_from_reader<R: Read + std::io::Seek>(
    reader: R,
    segments: &[String],
    member_path: &str,
) -> Result<Vec<u8>> {
    let (segment, tail) = segments
        .split_first()
        .ok_or_else(|| anyhow!("archive member path is empty"))?;
    let mut archive = zip::ZipArchive::new(reader)
        .with_context(|| format!("failed to open ZIP `{member_path}`"))?;
    let mut member = archive.by_name(segment).with_context(|| {
        format!("missing ZIP member `{segment}` while resolving `{member_path}`")
    })?;
    let mut bytes = Vec::new();
    member
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read archive member `{segment}`"))?;

    if tail.is_empty() {
        return Ok(bytes);
    }

    read_archive_member_bytes_from_reader(Cursor::new(bytes), tail, member_path)
}

pub(crate) fn storage_path_from_uri(storage_uri: &str) -> std::result::Result<&str, JobExecutionError> {
    storage_uri
        .strip_prefix("local://")
        .ok_or_else(|| JobExecutionError::terminal("unsupported_storage_uri"))
}

pub(crate) fn load_ingest_job_input(
    archive_reader_cache: &mut ArchiveReaderCache,
    job: &IngestClaimedJob,
) -> std::result::Result<(String, String), JobExecutionError> {
    let storage_uri = job
        .storage_uri
        .as_deref()
        .ok_or_else(|| JobExecutionError::terminal("missing_storage_uri"))?;
    let path = storage_path_from_uri(storage_uri)?;

    match job.source_file_kind {
        Some(IngestFileKind::Archive) => {
            let member_path = job
                .member_path
                .as_deref()
                .ok_or_else(|| JobExecutionError::terminal("missing_archive_member_path"))?;
            let input = read_archive_member_text(archive_reader_cache, path, member_path)
                .map_err(|_| JobExecutionError::retriable("archive_member_read_failed"))?;
            if input.contains('\0') {
                return Err(JobExecutionError::terminal("archive_member_contains_nul"));
            }
            Ok((member_path.to_string(), input))
        }
        _ => {
            let input = std::fs::read_to_string(path)
                .map_err(|_| JobExecutionError::retriable("storage_read_failed"))?;
            if input.contains('\0') {
                return Err(JobExecutionError::terminal("storage_file_contains_nul"));
            }
            Ok((path.to_string(), input))
        }
    }
}
