use tracker_parser_core::wide_corpus_triage::{
    WideCorpusTriageConfig, default_allowed_parse_issue_codes,
    default_committed_quarantine_sample_root, run_wide_corpus_triage,
};

#[test]
fn committed_quarantine_sample_root_contains_expected_hh_and_ts_files() {
    let root = default_committed_quarantine_sample_root();
    let hh_dir = root.join("hh");
    let ts_dir = root.join("ts");

    assert!(
        hh_dir.exists(),
        "missing hh quarantine sample dir at {}",
        hh_dir.display()
    );
    assert!(
        ts_dir.exists(),
        "missing ts quarantine sample dir at {}",
        ts_dir.display()
    );

    let hh_files = std::fs::read_dir(&hh_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .count();
    let ts_files = std::fs::read_dir(&ts_dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_file())
        .count();

    assert_eq!(hh_files, 3);
    assert_eq!(ts_files, 2);
}

#[test]
fn wide_corpus_triage_counts_known_sample_metrics() {
    let report = run_wide_corpus_triage(WideCorpusTriageConfig {
        roots: vec![default_committed_quarantine_sample_root()],
        allowed_issue_codes: default_allowed_parse_issue_codes(),
        example_limit: 3,
    })
    .unwrap();

    assert_eq!(report.source_files_total, 5);
    assert_eq!(report.source_files_parsed_ok, 5);
    assert_eq!(report.source_files_failed, 0);
    assert_eq!(report.hh_files_total, 3);
    assert_eq!(report.ts_files_total, 2);
    assert_eq!(report.hands_total, 4);
    assert_eq!(report.hands_normalized_exact, 3);
    assert_eq!(report.hands_normalized_uncertain, 1);
    assert_eq!(report.hands_normalized_inconsistent, 0);
    assert_eq!(report.allowed_issue_count, 8);
    assert_eq!(report.unexpected_issue_count, 0);
    assert_eq!(report.hands_with_unexpected_parse_issues, 0);
    assert_eq!(
        report
            .issue_counts_by_code
            .get("partial_reveal_show_line")
            .copied(),
        Some(2)
    );
    assert_eq!(
        report
            .issue_counts_by_code
            .get("partial_reveal_summary_show_surface")
            .copied(),
        Some(2)
    );
    assert_eq!(
        report
            .issue_counts_by_code
            .get("unsupported_no_show_line")
            .copied(),
        Some(2)
    );
    assert_eq!(
        report
            .issue_counts_by_code
            .get("ts_tail_finish_place_mismatch")
            .copied(),
        Some(1)
    );
    assert_eq!(
        report
            .issue_counts_by_code
            .get("ts_tail_total_received_mismatch")
            .copied(),
        Some(1)
    );
}

#[test]
fn empty_allowlist_promotes_known_issue_codes_to_unexpected() {
    let report = run_wide_corpus_triage(WideCorpusTriageConfig {
        roots: vec![default_committed_quarantine_sample_root()],
        allowed_issue_codes: std::collections::BTreeSet::new(),
        example_limit: 3,
    })
    .unwrap();

    assert_eq!(report.allowed_issue_count, 0);
    assert_eq!(report.unexpected_issue_count, 8);
    assert_eq!(report.hands_with_unexpected_parse_issues, 2);
}

#[test]
fn syntax_families_aggregate_examples_by_pattern_family() {
    let report = run_wide_corpus_triage(WideCorpusTriageConfig {
        roots: vec![default_committed_quarantine_sample_root()],
        allowed_issue_codes: default_allowed_parse_issue_codes(),
        example_limit: 3,
    })
    .unwrap();

    let partial_show_families = report
        .syntax_families
        .iter()
        .filter(|family| family.issue_code.as_deref() == Some("partial_reveal_show_line"))
        .collect::<Vec<_>>();

    assert_eq!(partial_show_families.len(), 1);

    let family = partial_show_families[0];
    assert_eq!(family.family_key, "hh_show_line::partial_reveal_show_line");
    assert_eq!(family.surface_kind, "hh_show_line");
    assert_eq!(family.hit_count, 2);
    assert_eq!(family.example_lines.len(), 2);
    assert!(
        family
            .example_lines
            .iter()
            .any(|line| line.contains("PartialA"))
    );
    assert!(
        family
            .example_lines
            .iter()
            .any(|line| line.contains("PartialB"))
    );
}
