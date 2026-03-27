use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;
use thiserror::Error;

use crate::{
    SourceKind, detect_source_kind,
    models::{CertaintyState, ParseIssue, ParseIssueCode},
    normalizer::normalize_hand,
    parsers::{
        hand_history::{parse_canonical_hand, split_hand_history},
        tournament_summary::parse_tournament_summary,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WideCorpusTriageConfig {
    pub roots: Vec<PathBuf>,
    pub allowed_issue_codes: BTreeSet<String>,
    pub example_limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SyntaxFamilyReport {
    pub family_key: String,
    pub surface_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_failure_kind: Option<String>,
    pub hit_count: usize,
    pub example_lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct WideCorpusTriageReport {
    pub source_files_total: usize,
    pub source_files_parsed_ok: usize,
    pub source_files_failed: usize,
    pub hh_files_total: usize,
    pub ts_files_total: usize,
    pub hands_total: usize,
    pub hands_normalized_exact: usize,
    pub hands_normalized_estimated: usize,
    pub hands_normalized_uncertain: usize,
    pub hands_normalized_inconsistent: usize,
    pub hands_with_unexpected_parse_issues: usize,
    pub allowed_issue_count: usize,
    pub unexpected_issue_count: usize,
    pub issue_counts_by_code: BTreeMap<String, usize>,
    pub syntax_families: Vec<SyntaxFamilyReport>,
}

#[derive(Debug, Error)]
pub enum WideCorpusTriageError {
    #[error("wide corpus triage requires at least one root path")]
    MissingRoots,
    #[error("wide corpus triage example_limit must be positive")]
    InvalidExampleLimit,
    #[error("wide corpus root does not exist: {path}")]
    MissingRoot { path: String },
    #[error("failed to read wide corpus directory {path}: {source}")]
    ReadRoot {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug)]
struct FileProcessFailure {
    failure_kind: &'static str,
    example: String,
}

#[derive(Debug, Default)]
struct FileTriageStats {
    hands_total: usize,
    hands_normalized_exact: usize,
    hands_normalized_estimated: usize,
    hands_normalized_uncertain: usize,
    hands_normalized_inconsistent: usize,
    hands_with_unexpected_parse_issues: usize,
    allowed_issue_count: usize,
    unexpected_issue_count: usize,
    issue_counts_by_code: BTreeMap<String, usize>,
    syntax_families: BTreeMap<String, SyntaxFamilyAccumulator>,
}

#[derive(Debug)]
struct SyntaxFamilyAccumulator {
    surface_kind: String,
    issue_code: Option<String>,
    parse_failure_kind: Option<String>,
    hit_count: usize,
    example_lines: Vec<String>,
}

impl SyntaxFamilyAccumulator {
    fn new(
        surface_kind: String,
        issue_code: Option<String>,
        parse_failure_kind: Option<String>,
    ) -> Self {
        Self {
            surface_kind,
            issue_code,
            parse_failure_kind,
            hit_count: 0,
            example_lines: Vec::new(),
        }
    }

    fn record_example(&mut self, example_limit: usize, example_line: String) {
        self.hit_count += 1;
        if self.example_lines.len() >= example_limit {
            return;
        }
        if self
            .example_lines
            .iter()
            .any(|existing| existing == &example_line)
        {
            return;
        }
        self.example_lines.push(example_line);
    }
}

#[derive(Debug, Default)]
struct ReportBuilder {
    report: WideCorpusTriageReport,
    syntax_families: BTreeMap<String, SyntaxFamilyAccumulator>,
}

impl ReportBuilder {
    fn record_source_file_seen(&mut self, kind: SourceKind) {
        self.report.source_files_total += 1;
        match kind {
            SourceKind::HandHistory => self.report.hh_files_total += 1,
            SourceKind::TournamentSummary => self.report.ts_files_total += 1,
        }
    }

    fn record_source_file_success(&mut self) {
        self.report.source_files_parsed_ok += 1;
    }

    fn record_source_file_failure(
        &mut self,
        kind: SourceKind,
        failure_kind: &'static str,
        example: String,
        example_limit: usize,
    ) {
        self.report.source_files_failed += 1;
        let surface_kind = match kind {
            SourceKind::HandHistory => "hh_file",
            SourceKind::TournamentSummary => "ts_file",
        };
        record_family(
            &mut self.syntax_families,
            surface_kind,
            None,
            Some(failure_kind.to_string()),
            example_limit,
            example,
        );
    }

    fn merge_file_stats(&mut self, stats: FileTriageStats) {
        self.report.hands_total += stats.hands_total;
        self.report.hands_normalized_exact += stats.hands_normalized_exact;
        self.report.hands_normalized_estimated += stats.hands_normalized_estimated;
        self.report.hands_normalized_uncertain += stats.hands_normalized_uncertain;
        self.report.hands_normalized_inconsistent += stats.hands_normalized_inconsistent;
        self.report.hands_with_unexpected_parse_issues += stats.hands_with_unexpected_parse_issues;
        self.report.allowed_issue_count += stats.allowed_issue_count;
        self.report.unexpected_issue_count += stats.unexpected_issue_count;

        for (code, count) in stats.issue_counts_by_code {
            *self.report.issue_counts_by_code.entry(code).or_default() += count;
        }
        merge_family_maps(&mut self.syntax_families, stats.syntax_families);
    }

    fn finish(mut self) -> WideCorpusTriageReport {
        self.report.syntax_families = self
            .syntax_families
            .into_iter()
            .map(|(family_key, family)| SyntaxFamilyReport {
                family_key,
                surface_kind: family.surface_kind,
                issue_code: family.issue_code,
                parse_failure_kind: family.parse_failure_kind,
                hit_count: family.hit_count,
                example_lines: family.example_lines,
            })
            .collect();
        self.report
    }
}

pub fn default_committed_quarantine_sample_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mbr/quarantine_sample")
}

pub fn default_local_quarantine_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.local/wide_corpus_quarantine")
}

pub fn default_allowed_parse_issue_codes() -> BTreeSet<String> {
    [
        ParseIssueCode::PartialRevealShowLine,
        ParseIssueCode::PartialRevealSummaryShowSurface,
        ParseIssueCode::UnsupportedNoShowLine,
        ParseIssueCode::TsTailFinishPlaceMismatch,
        ParseIssueCode::TsTailTotalReceivedMismatch,
    ]
    .into_iter()
    .map(|code| code.as_str().to_string())
    .collect()
}

pub fn run_wide_corpus_triage(
    config: WideCorpusTriageConfig,
) -> Result<WideCorpusTriageReport, WideCorpusTriageError> {
    if config.roots.is_empty() {
        return Err(WideCorpusTriageError::MissingRoots);
    }
    if config.example_limit == 0 {
        return Err(WideCorpusTriageError::InvalidExampleLimit);
    }

    let mut builder = ReportBuilder::default();
    for root in &config.roots {
        if !root.exists() {
            return Err(WideCorpusTriageError::MissingRoot {
                path: root.display().to_string(),
            });
        }
        process_kind_dir(
            root,
            SourceKind::HandHistory,
            &config.allowed_issue_codes,
            config.example_limit,
            &mut builder,
        )?;
        process_kind_dir(
            root,
            SourceKind::TournamentSummary,
            &config.allowed_issue_codes,
            config.example_limit,
            &mut builder,
        )?;
    }

    Ok(builder.finish())
}

fn process_kind_dir(
    root: &Path,
    kind: SourceKind,
    allowed_issue_codes: &BTreeSet<String>,
    example_limit: usize,
    builder: &mut ReportBuilder,
) -> Result<(), WideCorpusTriageError> {
    let dir = match kind {
        SourceKind::HandHistory => root.join("hh"),
        SourceKind::TournamentSummary => root.join("ts"),
    };
    if !dir.exists() {
        return Ok(());
    }

    let mut file_paths = fs::read_dir(&dir)
        .map_err(|source| WideCorpusTriageError::ReadRoot {
            path: dir.display().to_string(),
            source,
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    file_paths.sort();

    for file_path in file_paths {
        builder.record_source_file_seen(kind);
        let result = match kind {
            SourceKind::HandHistory => {
                process_hh_file(&file_path, allowed_issue_codes, example_limit)
            }
            SourceKind::TournamentSummary => {
                process_ts_file(&file_path, allowed_issue_codes, example_limit)
            }
        };

        match result {
            Ok(stats) => {
                builder.record_source_file_success();
                builder.merge_file_stats(stats);
            }
            Err(failure) => builder.record_source_file_failure(
                kind,
                failure.failure_kind,
                failure.example,
                example_limit,
            ),
        }
    }

    Ok(())
}

fn process_hh_file(
    file_path: &Path,
    allowed_issue_codes: &BTreeSet<String>,
    example_limit: usize,
) -> Result<FileTriageStats, FileProcessFailure> {
    let raw = fs::read_to_string(file_path).map_err(|error| FileProcessFailure {
        failure_kind: "read_to_string_failed",
        example: format!("{}: {error}", file_path.display()),
    })?;
    let detected = detect_source_kind(&raw).map_err(|error| FileProcessFailure {
        failure_kind: "detect_source_kind_failed",
        example: format!("{}: {error}", file_path.display()),
    })?;
    if detected != SourceKind::HandHistory {
        return Err(FileProcessFailure {
            failure_kind: "source_kind_mismatch",
            example: format!(
                "{}: expected hand_history, got {:?}",
                file_path.display(),
                detected
            ),
        });
    }

    let records = split_hand_history(&raw).map_err(|error| FileProcessFailure {
        failure_kind: "split_hand_history_failed",
        example: format!("{}: {error}", file_path.display()),
    })?;
    let mut stats = FileTriageStats::default();

    for record in records {
        let hand = parse_canonical_hand(&record.raw_text).map_err(|error| FileProcessFailure {
            failure_kind: "parse_canonical_hand_failed",
            example: format!(
                "{}: {}: {error}",
                file_path.display(),
                record.header.hand_id
            ),
        })?;
        let normalized = normalize_hand(&hand).map_err(|error| FileProcessFailure {
            failure_kind: "normalize_hand_failed",
            example: format!("{}: {}: {error}", file_path.display(), hand.header.hand_id),
        })?;

        stats.hands_total += 1;
        match normalized.settlement.certainty_state {
            CertaintyState::Exact => stats.hands_normalized_exact += 1,
            CertaintyState::Estimated => stats.hands_normalized_estimated += 1,
            CertaintyState::Uncertain => stats.hands_normalized_uncertain += 1,
            CertaintyState::Inconsistent => stats.hands_normalized_inconsistent += 1,
        }

        let mut hand_has_unexpected_parse_issue = false;
        for issue in &hand.parse_issues {
            let is_allowed = record_issue(&mut stats, issue, allowed_issue_codes, example_limit);
            hand_has_unexpected_parse_issue |= !is_allowed;
        }
        if hand_has_unexpected_parse_issue {
            stats.hands_with_unexpected_parse_issues += 1;
        }
    }

    Ok(stats)
}

fn process_ts_file(
    file_path: &Path,
    allowed_issue_codes: &BTreeSet<String>,
    example_limit: usize,
) -> Result<FileTriageStats, FileProcessFailure> {
    let raw = fs::read_to_string(file_path).map_err(|error| FileProcessFailure {
        failure_kind: "read_to_string_failed",
        example: format!("{}: {error}", file_path.display()),
    })?;
    let detected = detect_source_kind(&raw).map_err(|error| FileProcessFailure {
        failure_kind: "detect_source_kind_failed",
        example: format!("{}: {error}", file_path.display()),
    })?;
    if detected != SourceKind::TournamentSummary {
        return Err(FileProcessFailure {
            failure_kind: "source_kind_mismatch",
            example: format!(
                "{}: expected tournament_summary, got {:?}",
                file_path.display(),
                detected
            ),
        });
    }

    let summary = parse_tournament_summary(&raw).map_err(|error| FileProcessFailure {
        failure_kind: "parse_tournament_summary_failed",
        example: format!("{}: {error}", file_path.display()),
    })?;
    let mut stats = FileTriageStats::default();
    for issue in &summary.parse_issues {
        record_issue(&mut stats, issue, allowed_issue_codes, example_limit);
    }
    Ok(stats)
}

fn record_issue(
    stats: &mut FileTriageStats,
    issue: &ParseIssue,
    allowed_issue_codes: &BTreeSet<String>,
    example_limit: usize,
) -> bool {
    let code = issue.code.as_str().to_string();
    *stats.issue_counts_by_code.entry(code.clone()).or_default() += 1;

    let is_allowed = allowed_issue_codes.contains(&code);
    if is_allowed {
        stats.allowed_issue_count += 1;
    } else {
        stats.unexpected_issue_count += 1;
    }

    let example = issue
        .raw_line
        .clone()
        .unwrap_or_else(|| issue.message.clone());
    record_family(
        &mut stats.syntax_families,
        surface_kind_for_issue(issue.code),
        Some(code),
        None,
        example_limit,
        example,
    );

    is_allowed
}

fn surface_kind_for_issue(code: ParseIssueCode) -> &'static str {
    match code {
        ParseIssueCode::UnparsedLine | ParseIssueCode::ParserWarning => "hh_line",
        ParseIssueCode::UnparsedSummarySeatLine | ParseIssueCode::UnparsedSummarySeatTail => {
            "hh_summary_line"
        }
        ParseIssueCode::UnsupportedNoShowLine | ParseIssueCode::PartialRevealShowLine => {
            "hh_show_line"
        }
        ParseIssueCode::PartialRevealSummaryShowSurface => "hh_summary_show_line",
        ParseIssueCode::TsTailFinishPlaceMismatch | ParseIssueCode::TsTailTotalReceivedMismatch => {
            "ts_tail"
        }
        ParseIssueCode::HeroCardsMissingSeat
        | ParseIssueCode::ShowdownPlayerMissingSeat
        | ParseIssueCode::SummarySeatOutcomeSeatMismatch
        | ParseIssueCode::SummarySeatOutcomeMissingSeat
        | ParseIssueCode::ActionPlayerMissingSeat => "import_boundary",
    }
}

fn record_family(
    families: &mut BTreeMap<String, SyntaxFamilyAccumulator>,
    surface_kind: &str,
    issue_code: Option<String>,
    parse_failure_kind: Option<String>,
    example_limit: usize,
    example_line: String,
) {
    let family_key = match (&issue_code, &parse_failure_kind) {
        (Some(issue_code), None) => format!("{surface_kind}::{issue_code}"),
        (None, Some(parse_failure_kind)) => format!("{surface_kind}::{parse_failure_kind}"),
        (Some(issue_code), Some(parse_failure_kind)) => {
            format!("{surface_kind}::{issue_code}::{parse_failure_kind}")
        }
        (None, None) => format!("{surface_kind}::unknown"),
    };

    let family = families.entry(family_key).or_insert_with(|| {
        SyntaxFamilyAccumulator::new(
            surface_kind.to_string(),
            issue_code.clone(),
            parse_failure_kind.clone(),
        )
    });
    family.record_example(example_limit, example_line);
}

fn merge_family_maps(
    target: &mut BTreeMap<String, SyntaxFamilyAccumulator>,
    source: BTreeMap<String, SyntaxFamilyAccumulator>,
) {
    for (family_key, source_family) in source {
        let target_family = target.entry(family_key).or_insert_with(|| {
            SyntaxFamilyAccumulator::new(
                source_family.surface_kind.clone(),
                source_family.issue_code.clone(),
                source_family.parse_failure_kind.clone(),
            )
        });
        target_family.hit_count += source_family.hit_count;
        for example in source_family.example_lines {
            if target_family
                .example_lines
                .iter()
                .any(|existing| existing == &example)
            {
                continue;
            }
            target_family.example_lines.push(example);
        }
    }
}
