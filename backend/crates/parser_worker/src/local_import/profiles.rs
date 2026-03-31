use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct IngestStageProfile {
    pub parse_ms: u64,
    pub normalize_ms: u64,
    pub persist_ms: u64,
    pub materialize_ms: u64,
    pub finalize_ms: u64,
}

impl IngestStageProfile {
    pub(crate) fn add_assign(&mut self, other: IngestStageProfile) {
        self.parse_ms += other.parse_ms;
        self.normalize_ms += other.normalize_ms;
        self.persist_ms += other.persist_ms;
        self.materialize_ms += other.materialize_ms;
        self.finalize_ms += other.finalize_ms;
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct PrepareProfile {
    pub scan_ms: u64,
    pub pair_ms: u64,
    pub hash_ms: u64,
    pub enqueue_ms: u64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct ComputeProfile {
    pub parse_ms: u64,
    pub normalize_ms: u64,
    pub derive_hand_local_ms: u64,
    pub derive_tournament_ms: u64,
    pub persist_db_ms: u64,
    pub materialize_ms: u64,
    pub finalize_ms: u64,
    // Sub-timing breakdown within persist_db_ms (diagnostic)
    pub persist_upsert_roots_ms: u64,
    pub persist_delete_ms: u64,
    pub persist_canonical_ms: u64,
    pub persist_normalized_ms: u64,
    pub persist_derived_ms: u64,
    pub persist_hand_order_ms: u64,
}

impl ComputeProfile {
    pub(crate) fn add_assign(&mut self, other: ComputeProfile) {
        self.parse_ms += other.parse_ms;
        self.normalize_ms += other.normalize_ms;
        self.derive_hand_local_ms += other.derive_hand_local_ms;
        self.derive_tournament_ms += other.derive_tournament_ms;
        self.persist_db_ms += other.persist_db_ms;
        self.materialize_ms += other.materialize_ms;
        self.finalize_ms += other.finalize_ms;
        self.persist_upsert_roots_ms += other.persist_upsert_roots_ms;
        self.persist_delete_ms += other.persist_delete_ms;
        self.persist_canonical_ms += other.persist_canonical_ms;
        self.persist_normalized_ms += other.persist_normalized_ms;
        self.persist_derived_ms += other.persist_derived_ms;
        self.persist_hand_order_ms += other.persist_hand_order_ms;
    }

    pub(crate) fn legacy_stage_profile(self) -> IngestStageProfile {
        IngestStageProfile {
            parse_ms: self.parse_ms,
            normalize_ms: self.normalize_ms,
            persist_ms: self.derive_hand_local_ms + self.derive_tournament_ms + self.persist_db_ms,
            materialize_ms: self.materialize_ms,
            finalize_ms: self.finalize_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct IngestE2eProfile {
    pub prepare: PrepareProfile,
    pub runtime: ComputeProfile,
    pub prep_elapsed_ms: u64,
    pub runner_elapsed_ms: u64,
    pub e2e_elapsed_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct IngestRunProfile {
    pub processed_jobs: usize,
    pub file_jobs: usize,
    pub finalize_jobs: usize,
    pub files_persisted: usize,
    pub hands_persisted: usize,
    pub runtime_profile: ComputeProfile,
    pub stage_profile: IngestStageProfile,
}

impl IngestRunProfile {
    pub(crate) fn record_file_job(&mut self, report: &LocalImportReport) {
        self.file_jobs += 1;
        self.files_persisted += 1;
        self.hands_persisted += report.hands_persisted;
        self.runtime_profile.add_assign(report.runtime_profile);
        self.stage_profile.add_assign(report.stage_profile);
    }

    pub(crate) fn record_finalize_job(&mut self, runtime_profile: ComputeProfile) {
        self.finalize_jobs += 1;
        self.runtime_profile.add_assign(runtime_profile);
        self.stage_profile
            .add_assign(runtime_profile.legacy_stage_profile());
    }

    pub(crate) fn add_assign(&mut self, other: IngestRunProfile) {
        self.processed_jobs += other.processed_jobs;
        self.file_jobs += other.file_jobs;
        self.finalize_jobs += other.finalize_jobs;
        self.files_persisted += other.files_persisted;
        self.hands_persisted += other.hands_persisted;
        self.runtime_profile.add_assign(other.runtime_profile);
        self.stage_profile.add_assign(other.stage_profile);
    }
}

#[derive(Debug)]
pub struct LocalImportReport {
    pub file_kind: &'static str,
    pub source_file_id: Uuid,
    pub import_job_id: Uuid,
    pub tournament_id: Uuid,
    pub fragments_persisted: usize,
    pub hands_persisted: usize,
    pub runtime_profile: ComputeProfile,
    pub stage_profile: IngestStageProfile,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DirImportReport {
    pub prepare_report: tracker_ingest_prepare::PrepareReport,
    pub rejected_by_reason: std::collections::BTreeMap<String, usize>,
    pub bundle_id: Option<Uuid>,
    pub workers_used: usize,
    pub processed_jobs: usize,
    pub file_jobs: usize,
    pub finalize_jobs: usize,
    pub hands_persisted: usize,
    pub prep_elapsed_ms: u64,
    pub runner_elapsed_ms: u64,
    pub e2e_elapsed_ms: u64,
    pub hands_per_minute: f64,
    pub hands_per_minute_runner: f64,
    pub hands_per_minute_e2e: f64,
    pub e2e_profile: IngestE2eProfile,
    pub stage_profile: IngestStageProfile,
}

#[derive(Debug)]
pub struct TimezoneUpdateReport {
    pub user_id: Uuid,
    pub timezone_name: Option<String>,
    pub affected_profiles: usize,
    pub tournaments_recomputed: u64,
    pub hands_recomputed: u64,
}
