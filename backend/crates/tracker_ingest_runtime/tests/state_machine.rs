use tracker_ingest_runtime::{
    BundleStatus, FileJobStatus, FinalizeReadiness, compute_bundle_status,
};

#[test]
fn bundle_reaches_succeeded_when_all_files_succeed_and_finalize_completes() {
    let status = compute_bundle_status(
        &[FileJobStatus::Succeeded, FileJobStatus::Succeeded],
        FinalizeReadiness::Completed,
    );

    assert_eq!(status, BundleStatus::Succeeded);
}

#[test]
fn bundle_reaches_partial_success_when_success_and_terminal_failure_mix() {
    let status = compute_bundle_status(
        &[FileJobStatus::Succeeded, FileJobStatus::FailedTerminal],
        FinalizeReadiness::Completed,
    );

    assert_eq!(status, BundleStatus::PartialSuccess);
}

#[test]
fn retriable_failures_keep_bundle_non_terminal_before_finalize() {
    let status = compute_bundle_status(
        &[FileJobStatus::Succeeded, FileJobStatus::FailedRetriable],
        FinalizeReadiness::NotReady,
    );

    assert_eq!(status, BundleStatus::Running);
}

#[test]
fn bundle_fails_when_finalize_fails_after_terminal_file_jobs() {
    let status = compute_bundle_status(
        &[FileJobStatus::Succeeded, FileJobStatus::FailedTerminal],
        FinalizeReadiness::Failed,
    );

    assert_eq!(status, BundleStatus::Failed);
}
