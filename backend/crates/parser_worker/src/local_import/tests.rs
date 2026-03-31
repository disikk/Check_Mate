    use super::*;
    use mbr_stats_runtime::big_ko::expected_hero_mystery_cents;
    use mbr_stats_runtime::{
        CanonicalStatNumericValue, CanonicalStatState, FtDashboardDataState, FtDashboardFilters,
        FtValueState, MysteryEnvelope, SeedStatsFilters, query_canonical_stats, query_ft_dashboard,
        query_seed_stats,
    };
    use std::{
        io::Write,
        path::PathBuf,
        sync::{Mutex, OnceLock},
    };
    use tempfile::tempdir;
    use tracker_query_runtime::{
        FeatureRef, FilterCondition, FilterOperator, FilterValue, HandQueryRequest,
        query_matching_hand_ids,
    };

    const FT_HAND_ID: &str = "BR1064987693";
    const FIRST_FT_HAND_ID: &str = "BR1064986938";
    const BOUNDARY_RUSH_HAND_ID: &str = "BR1065004819";
    const EARLY_RUSH_HAND_ID: &str = "BR1065004261";
    const MULTI_COLLECT_HAND_ID: &str = "BR1064987148";
    const DEV_ORG_NAME: &str = "Check Mate Dev Org";
    const DEV_USER_EMAIL: &str = "mbr-dev-student@example.com";
    const DEV_PLAYER_NAME: &str = "Hero";
    const FULL_PACK_FIXTURE_PAIRS: &[(&str, &str)] = &[
        (
            "GG20260316 - Tournament #271767530 - Mystery Battle Royale 25.txt",
            "GG20260316-0307 - Mystery Battle Royale 25.txt",
        ),
        (
            "GG20260316 - Tournament #271767841 - Mystery Battle Royale 25.txt",
            "GG20260316-0312 - Mystery Battle Royale 25.txt",
        ),
        (
            "GG20260316 - Tournament #271768265 - Mystery Battle Royale 25.txt",
            "GG20260316-0316 - Mystery Battle Royale 25.txt",
        ),
        (
            "GG20260316 - Tournament #271768505 - Mystery Battle Royale 25.txt",
            "GG20260316-0319 - Mystery Battle Royale 25.txt",
        ),
        (
            "GG20260316 - Tournament #271768917 - Mystery Battle Royale 25.txt",
            "GG20260316-0323 - Mystery Battle Royale 25.txt",
        ),
        (
            "GG20260316 - Tournament #271769484 - Mystery Battle Royale 25.txt",
            "GG20260316-0338 - Mystery Battle Royale 25.txt",
        ),
        (
            "GG20260316 - Tournament #271769772 - Mystery Battle Royale 25.txt",
            "GG20260316-0342 - Mystery Battle Royale 25.txt",
        ),
        (
            "GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
            "GG20260316-0344 - Mystery Battle Royale 25.txt",
        ),
        (
            "GG20260316 - Tournament #271771269 - Mystery Battle Royale 25.txt",
            "GG20260316-0351 - Mystery Battle Royale 25.txt",
        ),
    ];

    #[test]
    fn build_batched_values_clause_numbers_placeholders_row_by_row() {
        let clause =
            build_batched_values_clause(2, &["{}", "CAST({} AS integer)", "COALESCE({}, NULL)"]);

        assert_eq!(
            clause,
            "($1, CAST($2 AS integer), COALESCE($3, NULL)), ($4, CAST($5 AS integer), COALESCE($6, NULL))"
        );
    }

    #[test]
    fn build_batched_values_clause_returns_empty_for_zero_rows() {
        assert_eq!(build_batched_values_clause(0, &["{}", "{}"]), "");
    }

    #[test]
    fn build_batched_insert_statement_places_suffix_after_values() {
        let statement = build_batched_insert_statement(
            "INSERT INTO demo_table (a, b)",
            &["{}", "{}"],
            2,
            Some("ON CONFLICT (a) DO UPDATE SET b = EXCLUDED.b RETURNING id"),
        );

        assert_eq!(
            statement,
            "INSERT INTO demo_table (a, b) VALUES ($1, $2), ($3, $4) ON CONFLICT (a) DO UPDATE SET b = EXCLUDED.b RETURNING id"
        );
    }

    fn hand_query_request(
        organization_id: Uuid,
        player_profile_id: Uuid,
        hero_filters: Vec<FilterCondition>,
        opponent_filters: Vec<FilterCondition>,
    ) -> HandQueryRequest {
        HandQueryRequest {
            organization_id,
            player_profile_id,
            hero_filters,
            opponent_filters,
        }
    }

    fn db_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static DB_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

        DB_TEST_MUTEX.get_or_init(|| Mutex::new(())).lock().unwrap()
    }

    fn import_path(path: &str) -> Result<LocalImportReport> {
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .context("CHECK_MATE_DATABASE_URL must exist for integration test")?;
        let mut client = Client::connect(&database_url, NoTls)
            .context("failed to connect to PostgreSQL for test import")?;
        let player_profile_id = ensure_test_import_actor(&mut client)?;
        drop(client);
        super::import_path_with_database_url(&database_url, path, player_profile_id)
    }

    fn import_path_with_database_url(database_url: &str, path: &str) -> Result<LocalImportReport> {
        let mut client = Client::connect(database_url, NoTls)
            .context("failed to connect to PostgreSQL for test import")?;
        let player_profile_id = ensure_test_import_actor(&mut client)?;
        drop(client);
        super::import_path_with_database_url(database_url, path, player_profile_id)
    }

    fn seed_import_actor(
        client: &mut Client,
        organization_name: &str,
        user_email: &str,
        screen_name: &str,
        primary_alias: Option<&str>,
        timezone_name: Option<&str>,
    ) -> Result<(Uuid, Uuid, Uuid)> {
        let organization_id: Uuid = if let Some(row) = client.query_opt(
            "SELECT id FROM org.organizations WHERE name = $1",
            &[&organization_name],
        )? {
            row.get(0)
        } else {
            client
                .query_one(
                    "INSERT INTO org.organizations (name) VALUES ($1) RETURNING id",
                    &[&organization_name],
                )?
                .get(0)
        };

        let user_id: Uuid = if let Some(row) =
            client.query_opt("SELECT id FROM auth.users WHERE email = $1", &[&user_email])?
        {
            row.get(0)
        } else {
            client
                .query_one(
                    "INSERT INTO auth.users (email, auth_provider, status, timezone_name)
                     VALUES ($1, 'seed', 'active', $2)
                     RETURNING id",
                    &[&user_email, &timezone_name],
                )?
                .get(0)
        };
        client.execute(
            "UPDATE auth.users
             SET timezone_name = $2
             WHERE id = $1",
            &[&user_id, &timezone_name],
        )?;

        client.execute(
            "INSERT INTO org.organization_memberships (organization_id, user_id, role)
             VALUES ($1, $2, 'student')
             ON CONFLICT (organization_id, user_id) DO NOTHING",
            &[&organization_id, &user_id],
        )?;

        let player_profile_id: Uuid = if let Some(row) = client.query_opt(
            "SELECT id
             FROM core.player_profiles
             WHERE organization_id = $1
               AND room = 'gg'
               AND screen_name = $2",
            &[&organization_id, &screen_name],
        )? {
            row.get(0)
        } else {
            client
                .query_one(
                    "INSERT INTO core.player_profiles (organization_id, owner_user_id, room, network, screen_name)
                     VALUES ($1, $2, 'gg', 'gg', $3)
                     RETURNING id",
                    &[&organization_id, &user_id, &screen_name],
                )?
                .get(0)
        };

        if let Some(primary_alias) = primary_alias {
            client.execute(
                "INSERT INTO core.player_aliases (
                    organization_id,
                    player_profile_id,
                    room,
                    alias,
                    is_primary,
                    source
                )
                VALUES ($1, $2, 'gg', $3, TRUE, 'test_context')
                ON CONFLICT (player_profile_id, room, alias)
                DO UPDATE SET
                    is_primary = TRUE,
                    source = EXCLUDED.source",
                &[&organization_id, &player_profile_id, &primary_alias],
            )?;
        }

        Ok((organization_id, user_id, player_profile_id))
    }

    fn ensure_test_import_actor(client: &mut Client) -> Result<Uuid> {
        let (_organization_id, _user_id, player_profile_id) = seed_import_actor(
            client,
            DEV_ORG_NAME,
            DEV_USER_EMAIL,
            DEV_PLAYER_NAME,
            Some(DEV_PLAYER_NAME),
            None,
        )?;
        Ok(player_profile_id)
    }

    fn assert_canonical_float_close(
        actual: &Option<CanonicalStatNumericValue>,
        expected: f64,
        stat_key: &str,
    ) {
        match actual {
            Some(CanonicalStatNumericValue::Float(value)) => {
                assert!(
                    (value - expected).abs() < 1e-6,
                    "{stat_key} expected {expected}, got {value}"
                );
            }
            other => panic!("{stat_key} expected float value, got {other:?}"),
        }
    }

    #[test]
    fn load_ingest_job_input_reads_archive_member_text() {
        let archive_path =
            std::env::temp_dir().join(format!("check-mate-archive-{}.zip", Uuid::new_v4()));
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("nested/member.hh", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"hello from zip member").unwrap();
        writer.finish().unwrap();

        let job = IngestClaimedJob {
            job_id: Uuid::new_v4(),
            bundle_id: Uuid::new_v4(),
            bundle_file_id: Some(Uuid::new_v4()),
            source_file_id: Some(Uuid::new_v4()),
            source_file_member_id: Some(Uuid::new_v4()),
            job_kind: tracker_ingest_runtime::JobKind::FileIngest,
            organization_id: Uuid::new_v4(),
            player_profile_id: Uuid::new_v4(),
            storage_uri: Some(format!("local://{}", archive_path.display())),
            source_file_kind: Some(IngestFileKind::Archive),
            member_path: Some("nested/member.hh".to_string()),
            file_kind: Some(IngestFileKind::HandHistory),
            attempt_no: 1,
        };

        let mut archive_reader_cache = ArchiveReaderCache::default();
        let (logical_path, input) = load_ingest_job_input(&mut archive_reader_cache, &job).unwrap();

        assert_eq!(logical_path, "nested/member.hh".to_string());
        assert_eq!(input, "hello from zip member".to_string());

        fs::remove_file(archive_path).unwrap();
    }

    #[test]
    fn archive_reader_cache_reuses_loaded_archive_after_source_file_is_deleted() {
        let archive_path =
            std::env::temp_dir().join(format!("check-mate-archive-cache-{}.zip", Uuid::new_v4()));
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("first.hh", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"first member").unwrap();
        writer
            .start_file("second.hh", zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"second member").unwrap();
        writer.finish().unwrap();

        let mut cache = ArchiveReaderCache::default();
        let first = cache.read_member_bytes(&archive_path, "first.hh").unwrap();
        assert_eq!(first, b"first member");

        fs::remove_file(&archive_path).unwrap();

        let second = cache.read_member_bytes(&archive_path, "second.hh").unwrap();
        assert_eq!(second, b"second member");
    }

    #[test]
    fn load_ingest_job_input_reads_nested_archive_member_text() {
        let archive_path =
            std::env::temp_dir().join(format!("check-mate-nested-archive-{}.zip", Uuid::new_v4()));
        let inner_bytes = {
            let cursor = std::io::Cursor::new(Vec::new());
            let mut writer = zip::ZipWriter::new(cursor);
            writer
                .start_file(
                    "deep/member.hh".to_string(),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(b"hello from nested zip member").unwrap();
            writer.finish().unwrap().into_inner()
        };
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "nested/inner.zip".to_string(),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(&inner_bytes).unwrap();
        writer.finish().unwrap();

        let job = IngestClaimedJob {
            job_id: Uuid::new_v4(),
            bundle_id: Uuid::new_v4(),
            bundle_file_id: Some(Uuid::new_v4()),
            source_file_id: Some(Uuid::new_v4()),
            source_file_member_id: Some(Uuid::new_v4()),
            job_kind: tracker_ingest_runtime::JobKind::FileIngest,
            organization_id: Uuid::new_v4(),
            player_profile_id: Uuid::new_v4(),
            storage_uri: Some(format!("local://{}", archive_path.display())),
            source_file_kind: Some(IngestFileKind::Archive),
            member_path: Some("nested/inner.zip!/deep/member.hh".to_string()),
            file_kind: Some(IngestFileKind::HandHistory),
            attempt_no: 1,
        };

        let mut archive_reader_cache = ArchiveReaderCache::default();
        let (logical_path, input) = load_ingest_job_input(&mut archive_reader_cache, &job).unwrap();

        assert_eq!(logical_path, "nested/inner.zip!/deep/member.hh".to_string());
        assert_eq!(input, "hello from nested zip member".to_string());

        fs::remove_file(archive_path).unwrap();
    }

    #[test]
    fn load_ingest_job_input_rejects_archive_member_text_with_nul() {
        let archive_path =
            std::env::temp_dir().join(format!("check-mate-archive-nul-{}.zip", Uuid::new_v4()));
        let file = fs::File::create(&archive_path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "nested/member.hh".to_string(),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"hello\0from zip member").unwrap();
        writer.finish().unwrap();

        let job = IngestClaimedJob {
            job_id: Uuid::new_v4(),
            bundle_id: Uuid::new_v4(),
            bundle_file_id: Some(Uuid::new_v4()),
            source_file_id: Some(Uuid::new_v4()),
            source_file_member_id: Some(Uuid::new_v4()),
            job_kind: tracker_ingest_runtime::JobKind::FileIngest,
            organization_id: Uuid::new_v4(),
            player_profile_id: Uuid::new_v4(),
            storage_uri: Some(format!("local://{}", archive_path.display())),
            source_file_kind: Some(IngestFileKind::Archive),
            member_path: Some("nested/member.hh".to_string()),
            file_kind: Some(IngestFileKind::HandHistory),
            attempt_no: 1,
        };

        let mut archive_reader_cache = ArchiveReaderCache::default();
        let error = load_ingest_job_input(&mut archive_reader_cache, &job).unwrap_err();

        assert_eq!(error.disposition(), FailureDisposition::Terminal);
        assert_eq!(error.error_code(), "archive_member_contains_nul");

        fs::remove_file(archive_path).unwrap();
    }

    #[test]
    fn build_prepared_archive_input_orders_ts_then_hh_and_attaches_reject_diagnostics() {
        let dir = tempdir().unwrap();
        let ts_path = dir.path().join("winner.ts.txt");
        let hh_path = dir.path().join("winner.hh.txt");
        let orphan_path = dir.path().join("orphan.ts.txt");

        fs::write(
            &ts_path,
            fs::read_to_string(
                fixture_path(
                    "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
                ),
            )
            .unwrap(),
        )
        .unwrap();
        fs::write(
            &hh_path,
            fs::read_to_string(fixture_path(
                "../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt",
            ))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            &orphan_path,
            fs::read_to_string(
                fixture_path(
                    "../../fixtures/mbr/ts/GG20260316 - Tournament #271767530 - Mystery Battle Royale 25.txt",
                ),
            )
            .unwrap(),
        )
        .unwrap();

        let report = tracker_ingest_prepare::prepare_path(dir.path()).unwrap();
        let materialized = build_prepared_archive_input(dir.path(), &report)
            .expect("prepared pair archive should materialize")
            .expect("prepared pair archive should exist");

        assert_eq!(materialized.ingest_file.file_kind, IngestFileKind::Archive);
        assert_eq!(materialized.ingest_file.members.len(), 2);
        assert_eq!(
            materialized.ingest_file.members[0].member_kind,
            IngestFileKind::TournamentSummary
        );
        assert_eq!(
            materialized.ingest_file.members[0].depends_on_member_index,
            None
        );
        assert_eq!(
            materialized.ingest_file.members[1].member_kind,
            IngestFileKind::HandHistory
        );
        assert_eq!(
            materialized.ingest_file.members[1].depends_on_member_index,
            Some(0)
        );
        assert_eq!(materialized.ingest_file.diagnostics.len(), 1);
        assert_eq!(materialized.ingest_file.diagnostics[0].code, "missing_hh");
        assert_eq!(
            materialized.ingest_file.diagnostics[0]
                .member_path
                .as_deref(),
            Some("orphan.ts.txt")
        );
    }

    #[test]
    fn compute_profile_collapses_into_legacy_stage_profile() {
        let profile = ComputeProfile {
            parse_ms: 10,
            normalize_ms: 11,
            derive_hand_local_ms: 12,
            derive_tournament_ms: 13,
            persist_db_ms: 14,
            materialize_ms: 15,
            finalize_ms: 16,
            ..ComputeProfile::default()
        };

        assert_eq!(
            profile.legacy_stage_profile(),
            IngestStageProfile {
                parse_ms: 10,
                normalize_ms: 11,
                persist_ms: 39,
                materialize_ms: 15,
                finalize_ms: 16,
            }
        );
    }

    #[test]
    fn summarize_rejected_by_reason_counts_each_code() {
        let report = PrepareReport {
            scanned_files: 3,
            paired_tournaments: vec![],
            rejected_tournaments: vec![
                RejectedTournament {
                    tournament_id: Some("1".to_string()),
                    files: vec![],
                    reason_code: RejectReasonCode::MissingHh,
                    reason_text: "missing hh".to_string(),
                },
                RejectedTournament {
                    tournament_id: Some("2".to_string()),
                    files: vec![],
                    reason_code: RejectReasonCode::MissingHh,
                    reason_text: "missing hh again".to_string(),
                },
                RejectedTournament {
                    tournament_id: None,
                    files: vec![],
                    reason_code: RejectReasonCode::UnsupportedSource,
                    reason_text: "unsupported".to_string(),
                },
            ],
            scan_ms: 1,
            pair_ms: 2,
            hash_ms: 3,
        };

        assert_eq!(
            summarize_rejected_by_reason(&report),
            BTreeMap::from([
                ("missing_hh".to_string(), 2usize),
                ("unsupported_source".to_string(), 1usize),
            ])
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn dir_import_path_enqueues_pair_first_bundle_and_runs_parallel_workers() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        let player_profile_id = ensure_test_import_actor(&mut setup_client).unwrap();
        drop(setup_client);

        let dir = tempdir().unwrap();
        fs::write(
            dir.path().join("winner.ts.txt"),
            fs::read_to_string(fixture_path(
                "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
            ))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            dir.path().join("winner.hh.txt"),
            fs::read_to_string(fixture_path(
                "../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt",
            ))
            .unwrap(),
        )
        .unwrap();
        fs::write(
            dir.path().join("orphan.ts.txt"),
            fs::read_to_string(fixture_path(
                "../../fixtures/mbr/ts/GG20260316 - Tournament #271767530 - Mystery Battle Royale 25.txt",
            ))
            .unwrap(),
        )
        .unwrap();

        let report = super::dir_import_with_database_url(
            &database_url,
            dir.path().to_str().unwrap(),
            player_profile_id,
            2,
        )
        .unwrap();

        assert_eq!(report.prepare_report.paired_tournaments.len(), 1);
        assert_eq!(report.prepare_report.rejected_tournaments.len(), 1);
        assert_eq!(report.rejected_by_reason.get("missing_hh"), Some(&1));
        assert_eq!(report.workers_used, 2);
        assert_eq!(report.file_jobs, 2);
        assert_eq!(report.finalize_jobs, 1);
        assert!(report.bundle_id.is_some());
        assert!(report.hands_persisted > 0);
        assert_eq!(report.hands_per_minute, report.hands_per_minute_runner);
        assert!(report.e2e_elapsed_ms >= report.runner_elapsed_ms);
        assert!(report.e2e_profile.prep_elapsed_ms <= report.e2e_elapsed_ms);
        assert_eq!(
            report.e2e_profile.prepare.scan_ms,
            report.prepare_report.scan_ms
        );
        assert_eq!(
            report.stage_profile,
            report.e2e_profile.runtime.legacy_stage_profile()
        );

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let summary = load_bundle_summary(&mut client, report.bundle_id.unwrap()).unwrap();
        assert_eq!(summary.status, IngestBundleStatus::Succeeded);
    }

    #[test]
    fn builds_canonical_rows_for_ft_all_in_hand() {
        let hand_text = first_ft_hand_text();
        let hand = parse_canonical_hand(&hand_text).unwrap();

        let rows = build_canonical_persistence(&hand).unwrap();

        assert_eq!(rows.seats.len(), 2);
        assert_eq!(rows.hole_cards.len(), 2);
        assert_eq!(rows.actions.len(), 9);
        assert_eq!(rows.showdowns.len(), 2);

        assert_eq!(
            rows.seats,
            vec![
                HandSeatRow {
                    seat_no: 3,
                    player_name: "f02e54a6".to_string(),
                    starting_stack: 1_992,
                    is_hero: false,
                    is_button: true,
                    is_sitting_out: false,
                },
                HandSeatRow {
                    seat_no: 7,
                    player_name: "Hero".to_string(),
                    starting_stack: 16_008,
                    is_hero: true,
                    is_button: false,
                    is_sitting_out: false,
                },
            ]
        );

        assert_eq!(
            rows.actions[4],
            HandActionRow {
                sequence_no: 4,
                street: "preflop".to_string(),
                seat_no: Some(3),
                action_type: "raise_to".to_string(),
                raw_amount: Some(1_512),
                to_amount: Some(1_912),
                is_all_in: true,
                all_in_reason: Some("raise_exhausted".to_string()),
                forced_all_in_preflop: false,
                references_previous_bet: true,
                raw_line: "f02e54a6: raises 1,512 to 1,912 and is all-in".to_string(),
            }
        );

        assert_eq!(
            rows.board,
            Some(HandBoardRow {
                flop1: Some("7d".to_string()),
                flop2: Some("2s".to_string()),
                flop3: Some("8h".to_string()),
                turn: Some("2c".to_string()),
                river: Some("Kh".to_string()),
            })
        );

        assert!(rows.parse_issues.is_empty());
    }

    #[test]
    fn classifies_parse_issues_with_structured_severity_at_parser_worker_boundary() {
        let mut hand = parse_canonical_hand(&first_ft_hand_text()).unwrap();
        hand.parse_issues
            .push(tracker_parser_core::models::ParseIssue {
                severity: tracker_parser_core::models::ParseIssueSeverity::Warning,
                code: tracker_parser_core::models::ParseIssueCode::UnparsedLine,
                message: "unparsed_line: Dealer note: test-only unexpected line".to_string(),
                raw_line: Some("Dealer note: test-only unexpected line".to_string()),
                payload: Some(tracker_parser_core::models::ParseIssuePayload::RawLine {
                    raw_line: "Dealer note: test-only unexpected line".to_string(),
                }),
            });
        hand.actions
            .push(tracker_parser_core::models::HandActionEvent {
                seq: 999,
                street: Street::Summary,
                player_name: Some("Ghost".to_string()),
                action_type: ActionType::Fold,
                is_forced: false,
                is_all_in: false,
                all_in_reason: None,
                forced_all_in_preflop: false,
                amount: None,
                to_amount: None,
                cards: None,
                raw_line: "Ghost: folds".to_string(),
            });

        let rows = build_canonical_persistence(&hand).unwrap();

        assert!(rows.parse_issues.contains(&ParseIssueRow {
            severity: "warning".to_string(),
            code: "unparsed_line".to_string(),
            message: "unparsed_line: Dealer note: test-only unexpected line".to_string(),
            raw_line: Some("Dealer note: test-only unexpected line".to_string()),
            payload: serde_json::json!({
                "raw_line": "Dealer note: test-only unexpected line"
            }),
        }));
        assert!(rows.parse_issues.contains(&ParseIssueRow {
            severity: "error".to_string(),
            code: "action_player_missing_seat".to_string(),
            message: "action references `Ghost` without seat row".to_string(),
            raw_line: Some("Ghost: folds".to_string()),
            payload: serde_json::json!({
                "player_name": "Ghost",
                "raw_line": "Ghost: folds"
            }),
        }));
    }

    #[test]
    fn builds_summary_seat_outcome_rows_and_parse_issues_from_summary_surface() {
        let hand = parse_canonical_hand(&summary_outcome_hand_text()).unwrap();
        let rows = build_canonical_persistence(&hand).unwrap();

        assert_eq!(rows.summary_seat_outcomes.len(), 8);
        assert!(rows.summary_seat_outcomes.iter().any(|row| {
            row.seat_no == 1
                && row.position_marker.as_deref() == Some("button")
                && row.outcome_kind == "won"
                && row.won_amount == Some(110)
        }));
        assert!(rows.summary_seat_outcomes.iter().any(|row| {
            row.seat_no == 4
                && row.outcome_kind == "showed_lost"
                && row.shown_cards.as_ref() == Some(&vec!["Qh".to_string(), "Kh".to_string()])
        }));
        assert!(
            rows.summary_seat_outcomes
                .iter()
                .any(|row| row.seat_no == 6 && row.outcome_kind == "lost")
        );
        assert!(
            rows.summary_seat_outcomes
                .iter()
                .any(|row| row.seat_no == 7 && row.outcome_kind == "mucked")
        );
        assert!(
            rows.summary_seat_outcomes
                .iter()
                .any(|row| row.seat_no == 8 && row.outcome_kind == "collected")
        );
        assert!(
            !rows
                .summary_seat_outcomes
                .iter()
                .any(|row| row.seat_no == 2 && row.player_name == "Hero")
        );
        assert!(rows.parse_issues.iter().any(|issue| {
            issue.code == "summary_seat_outcome_seat_mismatch"
                && issue.raw_line.as_deref() == Some("Seat 2: Hero lost")
        }));
        assert!(rows.parse_issues.iter().any(|issue| {
            issue.code == "unparsed_summary_seat_tail"
                && issue.raw_line.as_deref() == Some("Seat 9: VillainX (button) ???")
        }));
    }

    #[test]
    fn builds_cm04_action_metadata_and_sitting_out_seat_flags() {
        let hand = parse_canonical_hand(&cm04_import_surface_hand_text()).unwrap();
        let rows = build_canonical_persistence(&hand).unwrap();

        assert!(
            rows.seats
                .iter()
                .any(|row| row.player_name == "Sitout" && row.is_sitting_out)
        );
        assert!(rows.actions.iter().any(|row| {
            row.action_type == "post_sb"
                && row.all_in_reason.as_deref() == Some("blind_exhausted")
                && row.forced_all_in_preflop
        }));
        assert!(
            rows.actions
                .iter()
                .any(|row| row.action_type == "post_dead" && row.seat_no == Some(4))
        );
        assert!(rows.parse_issues.iter().any(|issue| {
            issue.code == "partial_reveal_show_line"
                && issue.raw_line.as_deref() == Some("VillainDead: shows [5d]")
        }));
        assert!(rows.parse_issues.iter().any(|issue| {
            issue.code == "unsupported_no_show_line"
                && issue.raw_line.as_deref() == Some("VillainNoShow: doesn't show hand")
        }));
    }

    #[test]
    fn builds_hand_state_resolution_for_ft_all_in_hand() {
        let hand_text = first_ft_hand_text();
        let hand = parse_canonical_hand(&hand_text).unwrap();
        let normalized = normalize_hand(&hand).unwrap();

        let row = build_hand_state_resolution(&normalized);

        assert_eq!(row.resolution_version, HAND_RESOLUTION_VERSION);
        assert!(row.chip_conservation_ok);
        assert!(row.pot_conservation_ok);
        assert_eq!(row.settlement_state, "exact");
        assert_eq!(row.rake_amount, 0);
        assert_eq!(row.final_stacks.get("Hero"), Some(&18_000));
        assert_eq!(row.final_stacks.get("f02e54a6"), Some(&0));
        assert!(row.invariant_issues.is_empty());
        assert_eq!(row.settlement.certainty_state, CertaintyState::Exact);
        assert!(row.settlement.issues.is_empty());
    }

    #[test]
    fn builds_hand_elimination_rows_for_ft_all_in_hand() {
        let hand_text = first_ft_hand_text();
        let hand = parse_canonical_hand(&hand_text).unwrap();
        let normalized = normalize_hand(&hand).unwrap();

        assert_eq!(normalized.eliminations.len(), 1);
        assert_eq!(normalized.eliminations[0].eliminated_seat_no, 3);
        assert_eq!(
            normalized.eliminations[0].eliminated_player_name,
            "f02e54a6"
        );
        assert_eq!(normalized.eliminations[0].last_busting_pot_no, Some(1));
        assert_eq!(
            normalized.eliminations[0].ko_winner_set,
            vec!["Hero".to_string()]
        );

        let rows = build_hand_elimination_rows(&normalized);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].pots_participated_by_busted, vec![1]);
        assert_eq!(rows[0].pots_causing_bust, vec![1]);
        assert_eq!(rows[0].last_busting_pot_no, Some(1));
        assert_eq!(rows[0].ko_winner_set, vec!["Hero".to_string()]);
        assert_eq!(
            rows[0].ko_share_fraction_by_winner,
            vec![HandEliminationKoShareRow {
                seat_no: 7,
                player_name: "Hero".to_string(),
                share_fraction: "1.000000".to_string(),
            }]
        );
        assert_eq!(rows[0].elimination_certainty_state, "exact");
        assert_eq!(rows[0].ko_certainty_state, "exact");
    }

    #[test]
    fn builds_cm06_joint_ko_elimination_rows() {
        let hand = parse_canonical_hand(&cm06_joint_ko_hand_text()).unwrap();
        let normalized = normalize_hand(&hand).unwrap();

        let rows = build_hand_elimination_rows(&normalized);
        let medium = rows
            .iter()
            .find(|row| row.eliminated_player_name == "Medium")
            .unwrap();

        assert_eq!(medium.pots_participated_by_busted, vec![1, 2]);
        assert_eq!(medium.pots_causing_bust, vec![2]);
        assert_eq!(medium.last_busting_pot_no, Some(2));
        assert_eq!(medium.ko_winner_set, vec!["Hero".to_string()]);
        assert_eq!(
            medium.ko_share_fraction_by_winner,
            vec![HandEliminationKoShareRow {
                seat_no: 1,
                player_name: "Hero".to_string(),
                share_fraction: "1.000000".to_string(),
            }]
        );
        assert_eq!(medium.elimination_certainty_state, "exact");
        assert_eq!(medium.ko_certainty_state, "exact");
    }

    #[test]
    fn builds_pot_and_return_rows_for_ft_hands() {
        let ft_hand = parse_canonical_hand(&first_ft_hand_text()).unwrap();
        let ft_normalized = normalize_hand(&ft_hand).unwrap();

        let pot_rows = build_hand_pot_rows(&ft_normalized);
        let eligibility_rows = build_hand_pot_eligibility_rows(&ft_normalized);
        let contribution_rows = build_hand_pot_contribution_rows(&ft_normalized);
        let winner_rows = build_hand_pot_winner_rows(&ft_normalized);
        let return_rows = build_hand_return_rows(&ft_normalized);

        assert_eq!(pot_rows.len(), 1);
        assert_eq!(pot_rows[0].pot_no, 1);
        assert_eq!(pot_rows[0].pot_type, "main");
        assert_eq!(pot_rows[0].amount, 3_984);
        assert_eq!(eligibility_rows.len(), 2);
        assert_eq!(contribution_rows.len(), 2);
        assert_eq!(winner_rows.len(), 1);
        assert_eq!(winner_rows[0].pot_no, 1);
        assert_eq!(winner_rows[0].seat_no, 7);
        assert_eq!(winner_rows[0].share_amount, 3_984);
        assert!(return_rows.is_empty());

        let uncalled_hand = parse_canonical_hand(&second_ft_hand_text()).unwrap();
        let uncalled_normalized = normalize_hand(&uncalled_hand).unwrap();
        let uncalled_returns = build_hand_return_rows(&uncalled_normalized);

        assert_eq!(uncalled_returns.len(), 1);
        assert_eq!(uncalled_returns[0].seat_no, 7);
        assert_eq!(uncalled_returns[0].amount, 15_048);
        assert_eq!(uncalled_returns[0].reason, "uncalled");
    }

    #[test]
    fn builds_cm05_pot_eligibility_and_settlement_issue_rows() {
        let hand = parse_canonical_hand(&cm05_hidden_showdown_hand_text()).unwrap();
        let normalized = normalize_hand(&hand).unwrap();

        let resolution_row = build_hand_state_resolution(&normalized);
        let eligibility_rows = build_hand_pot_eligibility_rows(&normalized);
        let winner_rows = build_hand_pot_winner_rows(&normalized);

        assert!(winner_rows.is_empty());
        assert_eq!(eligibility_rows.len(), 6);
        assert!(resolution_row.invariant_issues.is_empty());
        assert_eq!(resolution_row.settlement_state, "uncertain");
        assert_eq!(resolution_row.settlement.pots.len(), 2);
        assert_eq!(
            resolution_row
                .settlement
                .pots
                .iter()
                .map(|pot| pot.issues.clone())
                .collect::<Vec<_>>(),
            vec![
                vec![
                    tracker_parser_core::models::PotSettlementIssue::AmbiguousHiddenShowdown {
                        eligible_players: vec!["Hero".to_string(), "Villain".to_string()],
                    }
                ],
                vec![
                    tracker_parser_core::models::PotSettlementIssue::AmbiguousHiddenShowdown {
                        eligible_players: vec!["Hero".to_string(), "Villain".to_string()],
                    }
                ],
            ]
        );
    }

    #[test]
    fn builds_per_target_attempt_rows_for_multiway_hero_push() {
        let hand = manual_attempt_test_hand(
            vec![
                ("Hero", 1, 1_000),
                ("ShortOne", 2, 400),
                ("ShortTwo", 3, 300),
            ],
            vec![
                manual_action(
                    1,
                    Street::Preflop,
                    Some("Hero"),
                    ActionType::RaiseTo,
                    Some(1_000),
                    Some(1_000),
                    true,
                    Some(tracker_parser_core::models::AllInReason::Voluntary),
                    false,
                    "Hero: raises to 1000 and is all-in",
                ),
                manual_action(
                    2,
                    Street::Preflop,
                    Some("ShortOne"),
                    ActionType::Call,
                    Some(400),
                    None,
                    true,
                    Some(tracker_parser_core::models::AllInReason::CallExhausted),
                    false,
                    "ShortOne: calls 400 and is all-in",
                ),
                manual_action(
                    3,
                    Street::Preflop,
                    Some("ShortTwo"),
                    ActionType::Call,
                    Some(300),
                    None,
                    true,
                    Some(tracker_parser_core::models::AllInReason::CallExhausted),
                    false,
                    "ShortTwo: calls 300 and is all-in",
                ),
            ],
        );

        let attempts = build_hand_ko_attempt_rows(&hand);
        let opportunities = build_hand_ko_opportunity_rows(&hand);

        assert_eq!(attempts.len(), 2);
        assert_eq!(opportunities.len(), 2);
        assert_eq!(
            attempts
                .iter()
                .map(|row| (
                    row.target_seat_no,
                    row.attempt_kind.as_str(),
                    row.street.as_str()
                ))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([
                (2_i32, "hero_push", "preflop"),
                (3_i32, "hero_push", "preflop"),
            ])
        );
    }

    #[test]
    fn builds_forced_auto_all_in_attempt_and_opportunity_rows_from_edge_matrix_fixture() {
        let hands = all_hands_from_fixture("GG20260325-phase0-exact-core-edge-matrix.txt");
        let ante_exhausted = hands
            .iter()
            .find(|hand| hand.header.hand_id == "BRCM0403")
            .unwrap();
        let blind_exhausted = hands
            .iter()
            .find(|hand| hand.header.hand_id == "BRCM0404")
            .unwrap();

        let ante_attempts = build_hand_ko_attempt_rows(ante_exhausted);
        let ante_opportunities = build_hand_ko_opportunity_rows(ante_exhausted);
        let blind_attempts = build_hand_ko_attempt_rows(blind_exhausted);
        let blind_opportunities = build_hand_ko_opportunity_rows(blind_exhausted);

        assert_eq!(ante_attempts.len(), 1);
        assert_eq!(ante_opportunities.len(), 1);
        assert_eq!(ante_attempts[0].target_player_name, "ShortAnte");
        assert_eq!(ante_attempts[0].attempt_kind, "forced_auto_all_in");
        assert_eq!(ante_attempts[0].street, "preflop");
        assert!(ante_attempts[0].is_forced_all_in);
        assert_eq!(ante_opportunities[0].opportunity_kind, "forced_auto_all_in");
        assert!(ante_opportunities[0].is_forced_all_in);

        assert_eq!(blind_attempts.len(), 1);
        assert_eq!(blind_opportunities.len(), 1);
        assert_eq!(blind_attempts[0].target_player_name, "ShortBb");
        assert_eq!(blind_attempts[0].attempt_kind, "forced_auto_all_in");
        assert_eq!(blind_attempts[0].street, "preflop");
        assert!(blind_attempts[0].is_forced_all_in);
        assert_eq!(
            blind_opportunities[0].opportunity_kind,
            "forced_auto_all_in"
        );
        assert!(blind_opportunities[0].is_forced_all_in);
    }

    #[test]
    fn does_not_create_attempt_or_opportunity_without_confrontation() {
        let hands = all_hands_from_fixture("GG20260325-phase0-exact-core-edge-matrix.txt");
        let no_confrontation = hands
            .iter()
            .find(|hand| hand.header.hand_id == "BRCM0405")
            .unwrap();

        assert!(build_hand_ko_attempt_rows(no_confrontation).is_empty());
        assert!(build_hand_ko_opportunity_rows(no_confrontation).is_empty());
    }

    #[test]
    fn caps_attempts_and_opportunities_to_one_row_per_target() {
        let hand = manual_attempt_test_hand(
            vec![("Hero", 1, 1_000), ("Target", 2, 400)],
            vec![
                manual_action(
                    1,
                    Street::Preflop,
                    Some("Hero"),
                    ActionType::RaiseTo,
                    Some(1_000),
                    Some(1_000),
                    true,
                    Some(tracker_parser_core::models::AllInReason::Voluntary),
                    false,
                    "Hero: raises to 1000 and is all-in",
                ),
                manual_action(
                    2,
                    Street::Preflop,
                    Some("Target"),
                    ActionType::Call,
                    Some(400),
                    None,
                    true,
                    Some(tracker_parser_core::models::AllInReason::CallExhausted),
                    false,
                    "Target: calls 400 and is all-in",
                ),
                manual_action(
                    3,
                    Street::Turn,
                    Some("Target"),
                    ActionType::Bet,
                    Some(0),
                    Some(0),
                    true,
                    Some(tracker_parser_core::models::AllInReason::RaiseExhausted),
                    false,
                    "Target: impossible duplicate all-in marker",
                ),
            ],
        );

        assert_eq!(build_hand_ko_attempt_rows(&hand).len(), 1);
        assert_eq!(build_hand_ko_opportunity_rows(&hand).len(), 1);
    }

    #[test]
    fn builds_mbr_stage_resolution_for_ft_and_rush_hands() {
        let hands = all_hands_from_fixture("GG20260316-0344 - Mystery Battle Royale 25.txt");
        let rows = build_mbr_stage_resolutions(Uuid::nil(), &hands);

        let ft_row = rows.get(FIRST_FT_HAND_ID).unwrap();
        assert_eq!(ft_row.player_profile_id, Uuid::nil());
        assert!(ft_row.played_ft_hand);
        assert!(ft_row.is_ft_hand);
        assert_eq!(ft_row.played_ft_hand_state, "exact");
        assert_eq!(ft_row.ft_players_remaining_exact, Some(9));
        assert!(!ft_row.is_stage_2);
        assert!(!ft_row.is_stage_3_4);
        assert!(!ft_row.is_stage_4_5);
        assert!(!ft_row.is_stage_5_6);
        assert!(ft_row.is_stage_6_9);
        assert!(!ft_row.is_boundary_hand);
        assert!(!ft_row.entered_boundary_zone);
        assert_eq!(ft_row.entered_boundary_zone_state, "exact");
        assert_eq!(ft_row.boundary_resolution_state, "exact");
        assert_eq!(ft_row.boundary_candidate_count, 1);
        assert_eq!(
            ft_row.boundary_resolution_method,
            "timeline_last_non_ft_candidate_v2"
        );
        assert_eq!(ft_row.boundary_confidence_class, "single_candidate");
        assert_eq!(ft_row.ft_table_size, Some(9));
        assert_eq!(ft_row.boundary_ko_state, "uncertain");

        let boundary_row = rows.get(BOUNDARY_RUSH_HAND_ID).unwrap();
        assert_eq!(boundary_row.player_profile_id, Uuid::nil());
        assert!(!boundary_row.played_ft_hand);
        assert!(!boundary_row.is_ft_hand);
        assert_eq!(boundary_row.played_ft_hand_state, "exact");
        assert_eq!(boundary_row.ft_players_remaining_exact, None);
        assert!(!boundary_row.is_stage_2);
        assert!(!boundary_row.is_stage_3_4);
        assert!(!boundary_row.is_stage_4_5);
        assert!(!boundary_row.is_stage_5_6);
        assert!(!boundary_row.is_stage_6_9);
        assert!(boundary_row.is_boundary_hand);
        assert!(boundary_row.entered_boundary_zone);
        assert_eq!(boundary_row.entered_boundary_zone_state, "exact");
        assert_eq!(boundary_row.boundary_resolution_state, "exact");
        assert_eq!(boundary_row.boundary_candidate_count, 1);
        assert_eq!(boundary_row.ft_table_size, None);
        assert!(boundary_row.boundary_ko_min.is_none());
        assert!(boundary_row.boundary_ko_ev.is_none());
        assert!(boundary_row.boundary_ko_max.is_none());
        assert_eq!(boundary_row.boundary_ko_state, "uncertain");

        let early_rush_row = rows.get(EARLY_RUSH_HAND_ID).unwrap();
        assert!(!early_rush_row.played_ft_hand);
        assert!(!early_rush_row.is_ft_hand);
        assert_eq!(early_rush_row.ft_players_remaining_exact, None);
        assert!(!early_rush_row.is_stage_2);
        assert!(!early_rush_row.is_stage_3_4);
        assert!(!early_rush_row.is_stage_4_5);
        assert!(!early_rush_row.is_stage_5_6);
        assert!(!early_rush_row.is_stage_6_9);
        assert!(!early_rush_row.is_boundary_hand);
        assert!(!early_rush_row.entered_boundary_zone);
        assert_eq!(early_rush_row.entered_boundary_zone_state, "exact");
    }

    #[test]
    fn builds_formal_stage_predicates_from_exact_ft_player_counts() {
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "rush-boundary".to_string(),
                    played_at: "2026/03/16 10:40:00".to_string(),
                    max_players: 5,
                    seat_count: 5,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-9".to_string(),
                    played_at: "2026/03/16 10:41:00".to_string(),
                    max_players: 9,
                    seat_count: 9,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-8".to_string(),
                    played_at: "2026/03/16 10:42:00".to_string(),
                    max_players: 9,
                    seat_count: 8,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-7".to_string(),
                    played_at: "2026/03/16 10:43:00".to_string(),
                    max_players: 9,
                    seat_count: 7,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-6".to_string(),
                    played_at: "2026/03/16 10:44:00".to_string(),
                    max_players: 9,
                    seat_count: 6,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-5".to_string(),
                    played_at: "2026/03/16 10:45:00".to_string(),
                    max_players: 9,
                    seat_count: 5,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-4".to_string(),
                    played_at: "2026/03/16 10:46:00".to_string(),
                    max_players: 9,
                    seat_count: 4,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-3".to_string(),
                    played_at: "2026/03/16 10:47:00".to_string(),
                    max_players: 9,
                    seat_count: 3,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-2".to_string(),
                    played_at: "2026/03/16 10:48:00".to_string(),
                    max_players: 9,
                    seat_count: 2,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        let boundary = rows.get("rush-boundary").unwrap();
        assert!(!boundary.is_ft_hand);
        assert_eq!(boundary.ft_players_remaining_exact, None);
        assert!(!boundary.is_stage_2);
        assert!(!boundary.is_stage_3_4);
        assert!(!boundary.is_stage_4_5);
        assert!(!boundary.is_stage_5_6);
        assert!(!boundary.is_stage_6_9);
        assert!(boundary.is_boundary_hand);

        let ft_9 = rows.get("ft-9").unwrap();
        assert!(ft_9.is_ft_hand);
        assert_eq!(ft_9.ft_players_remaining_exact, Some(9));
        assert!(ft_9.is_stage_6_9);
        assert!(!ft_9.is_stage_5_6);

        let ft_8 = rows.get("ft-8").unwrap();
        assert!(ft_8.is_ft_hand);
        assert_eq!(ft_8.ft_players_remaining_exact, Some(8));
        assert!(ft_8.is_stage_6_9);

        let ft_7 = rows.get("ft-7").unwrap();
        assert!(ft_7.is_ft_hand);
        assert_eq!(ft_7.ft_players_remaining_exact, Some(7));
        assert!(ft_7.is_stage_6_9);

        let ft_6 = rows.get("ft-6").unwrap();
        assert!(ft_6.is_ft_hand);
        assert_eq!(ft_6.ft_players_remaining_exact, Some(6));
        assert!(ft_6.is_stage_5_6);
        assert!(ft_6.is_stage_6_9);
        assert!(!ft_6.is_stage_4_5);

        let ft_5 = rows.get("ft-5").unwrap();
        assert!(ft_5.is_ft_hand);
        assert_eq!(ft_5.ft_players_remaining_exact, Some(5));
        assert!(ft_5.is_stage_4_5);
        assert!(ft_5.is_stage_5_6);
        assert!(!ft_5.is_stage_6_9);

        let ft_4 = rows.get("ft-4").unwrap();
        assert!(ft_4.is_ft_hand);
        assert_eq!(ft_4.ft_players_remaining_exact, Some(4));
        assert!(ft_4.is_stage_3_4);
        assert!(ft_4.is_stage_4_5);
        assert!(!ft_4.is_stage_5_6);

        let ft_3 = rows.get("ft-3").unwrap();
        assert!(ft_3.is_ft_hand);
        assert_eq!(ft_3.ft_players_remaining_exact, Some(3));
        assert!(ft_3.is_stage_3_4);
        assert!(!ft_3.is_stage_4_5);

        let ft_2 = rows.get("ft-2").unwrap();
        assert!(ft_2.is_ft_hand);
        assert_eq!(ft_2.ft_players_remaining_exact, Some(2));
        assert!(ft_2.is_stage_2);
        assert!(!ft_2.is_stage_3_4);
        assert!(!ft_2.is_stage_4_5);
        assert!(!ft_2.is_stage_5_6);
        assert!(!ft_2.is_stage_6_9);
    }

    #[test]
    fn resolves_tournament_entry_economics_for_first_place_with_mystery_component() {
        let summary = tracker_parser_core::models::TournamentSummary {
            tournament_id: 271770266,
            tournament_name: "Mystery Battle Royale $25".to_string(),
            game_name: "Hold'em No Limit".to_string(),
            buy_in_cents: 1_250,
            rake_cents: 200,
            bounty_cents: 1_050,
            entrants: 18,
            total_prize_pool_cents: 41_400,
            started_at: "2026/03/16 10:19:41".to_string(),
            hero_name: "Hero".to_string(),
            finish_place: 1,
            payout_cents: 20_500,
            confirmed_finish_place: Some(1),
            confirmed_payout_cents: Some(20_500),
            parse_issues: Vec::new(),
        };
        let economics = resolve_tournament_entry_economics(&summary, 10_000).unwrap();

        assert_eq!(economics.regular_prize_cents, 10_000);
        assert_eq!(economics.mystery_money_cents, 10_500);
    }

    #[test]
    fn rejects_negative_mystery_component_for_tournament_entry_economics() {
        let summary = tracker_parser_core::models::TournamentSummary {
            tournament_id: 271770266,
            tournament_name: "Mystery Battle Royale $25".to_string(),
            game_name: "Hold'em No Limit".to_string(),
            buy_in_cents: 1_250,
            rake_cents: 200,
            bounty_cents: 1_050,
            entrants: 18,
            total_prize_pool_cents: 41_400,
            started_at: "2026/03/16 10:19:41".to_string(),
            hero_name: "Hero".to_string(),
            finish_place: 1,
            payout_cents: 5_000,
            confirmed_finish_place: Some(1),
            confirmed_payout_cents: Some(5_000),
            parse_issues: Vec::new(),
        };

        let error = resolve_tournament_entry_economics(&summary, 10_000).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("mystery_money_total cannot be negative")
        );
    }

    #[test]
    fn builds_warning_parse_issues_for_tournament_summary_tail_conflicts() {
        let summary = tracker_parser_core::parsers::tournament_summary::parse_tournament_summary(
            &fs::read_to_string(fixture_path(
                "../../fixtures/mbr/ts/GG20260325 - Tournament #271770266 - Tail conflict.txt",
            ))
            .unwrap(),
        )
        .unwrap();

        let issues = tournament_summary_parse_issues(&summary);

        assert_eq!(issues.len(), 2);
        assert!(issues.contains(&ParseIssueRow {
            severity: "warning".to_string(),
            code: "ts_tail_finish_place_mismatch".to_string(),
            message: "result line finish_place=1 conflicts with tail finish_place=2".to_string(),
            raw_line: None,
            payload: serde_json::json!({
                "result_finish_place": 1,
                "tail_finish_place": 2
            }),
        }));
        assert!(issues.contains(&ParseIssueRow {
            severity: "warning".to_string(),
            code: "ts_tail_total_received_mismatch".to_string(),
            message:
                "result line payout_cents=20500 conflicts with tail payout_cents=20400".to_string(),
            raw_line: None,
            payload: serde_json::json!({
                "result_payout_cents": 20500,
                "tail_payout_cents": 20400
            }),
        }));
    }

    #[test]
    fn keeps_boundary_ko_values_exact_only_for_exact_single_candidate() {
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "rush-early".to_string(),
                    played_at: "2026/03/16 10:40:00".to_string(),
                    max_players: 5,
                    seat_count: 5,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "rush-boundary".to_string(),
                    played_at: "2026/03/16 10:41:00".to_string(),
                    max_players: 5,
                    seat_count: 5,
                    exact_hero_boundary_ko_share: Some(0.5),
                },
                StageHandFact {
                    hand_id: "ft-first".to_string(),
                    played_at: "2026/03/16 10:42:00".to_string(),
                    max_players: 9,
                    seat_count: 7,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        let boundary = rows.get("rush-boundary").unwrap();
        assert!(boundary.entered_boundary_zone);
        assert_eq!(boundary.entered_boundary_zone_state, "exact");
        assert_eq!(boundary.boundary_resolution_state, "exact");
        assert_eq!(boundary.boundary_candidate_count, 1);
        assert_eq!(
            boundary.boundary_resolution_method,
            "timeline_last_non_ft_candidate_v2"
        );
        assert_eq!(boundary.boundary_confidence_class, "single_candidate");
        assert_eq!(boundary.boundary_ko_min.as_deref(), Some("0.500000"));
        assert_eq!(boundary.boundary_ko_ev.as_deref(), Some("0.500000"));
        assert_eq!(boundary.boundary_ko_max.as_deref(), Some("0.500000"));
        assert_eq!(
            boundary.boundary_ko_method.as_deref(),
            Some("timeline_last_non_ft_candidate_v2")
        );
        assert_eq!(boundary.boundary_ko_certainty.as_deref(), Some("exact"));
        assert_eq!(boundary.boundary_ko_state, "exact");

        let ft = rows.get("ft-first").unwrap();
        assert!(ft.played_ft_hand);
        assert_eq!(ft.ft_table_size, Some(7));
        assert_eq!(ft.boundary_ko_state, "uncertain");
        assert!(ft.boundary_ko_ev.is_none());
    }

    #[test]
    fn keeps_boundary_fields_unresolved_when_no_final_table_exists() {
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "rush-1".to_string(),
                    played_at: "2026/03/16 10:40:00".to_string(),
                    max_players: 5,
                    seat_count: 5,
                    exact_hero_boundary_ko_share: Some(1.0),
                },
                StageHandFact {
                    hand_id: "rush-2".to_string(),
                    played_at: "2026/03/16 10:41:00".to_string(),
                    max_players: 5,
                    seat_count: 5,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        let rush = rows.get("rush-1").unwrap();
        assert!(!rush.entered_boundary_zone);
        assert_eq!(rush.entered_boundary_zone_state, "exact");
        assert_eq!(rush.boundary_resolution_state, "uncertain");
        assert_eq!(rush.boundary_candidate_count, 0);
        assert_eq!(rush.boundary_confidence_class, "no_exact_ft_hand");
        assert_eq!(rush.boundary_ko_state, "uncertain");
        assert!(rush.boundary_ko_ev.is_none());
        assert!(rush.boundary_ko_min.is_none());
        assert!(rush.boundary_ko_max.is_none());
    }

    #[test]
    fn selects_last_two_max_when_it_is_the_last_non_ft_candidate_before_first_final_table() {
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "rush-5-max".to_string(),
                    played_at: "2026/03/16 10:40:00".to_string(),
                    max_players: 5,
                    seat_count: 5,
                    exact_hero_boundary_ko_share: Some(0.25),
                },
                StageHandFact {
                    hand_id: "rush-2-max".to_string(),
                    played_at: "2026/03/16 10:41:00".to_string(),
                    max_players: 2,
                    seat_count: 2,
                    exact_hero_boundary_ko_share: Some(1.0),
                },
                StageHandFact {
                    hand_id: "ft-first".to_string(),
                    played_at: "2026/03/16 10:42:00".to_string(),
                    max_players: 9,
                    seat_count: 8,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        let boundary = rows.get("rush-2-max").unwrap();
        assert!(boundary.entered_boundary_zone);
        assert_eq!(boundary.entered_boundary_zone_state, "exact");
        assert_eq!(boundary.boundary_resolution_state, "exact");
        assert_eq!(boundary.boundary_candidate_count, 1);
        assert_eq!(boundary.boundary_ko_ev.as_deref(), Some("1.000000"));
        assert_eq!(boundary.boundary_ko_state, "exact");

        let earlier = rows.get("rush-5-max").unwrap();
        assert!(!earlier.entered_boundary_zone);
        assert_eq!(earlier.entered_boundary_zone_state, "exact");
        assert_eq!(earlier.boundary_ko_state, "uncertain");
        assert!(earlier.boundary_ko_ev.is_none());
    }

    #[test]
    fn selects_last_three_max_when_it_is_the_last_non_ft_candidate_before_first_final_table() {
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "rush-5-max".to_string(),
                    played_at: "2026/03/16 10:40:00".to_string(),
                    max_players: 5,
                    seat_count: 5,
                    exact_hero_boundary_ko_share: Some(0.25),
                },
                StageHandFact {
                    hand_id: "rush-3-max".to_string(),
                    played_at: "2026/03/16 10:41:00".to_string(),
                    max_players: 3,
                    seat_count: 3,
                    exact_hero_boundary_ko_share: Some(0.75),
                },
                StageHandFact {
                    hand_id: "ft-first".to_string(),
                    played_at: "2026/03/16 10:42:00".to_string(),
                    max_players: 9,
                    seat_count: 8,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        let boundary = rows.get("rush-3-max").unwrap();
        assert!(boundary.entered_boundary_zone);
        assert_eq!(boundary.entered_boundary_zone_state, "exact");
        assert_eq!(boundary.boundary_resolution_state, "exact");
        assert_eq!(boundary.boundary_candidate_count, 1);
        assert_eq!(boundary.boundary_ko_ev.as_deref(), Some("0.750000"));
        assert_eq!(boundary.boundary_ko_state, "exact");
    }

    #[test]
    fn selects_last_four_max_when_it_is_the_last_non_ft_candidate_before_first_final_table() {
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "rush-5-max".to_string(),
                    played_at: "2026/03/16 10:40:00".to_string(),
                    max_players: 5,
                    seat_count: 5,
                    exact_hero_boundary_ko_share: Some(0.25),
                },
                StageHandFact {
                    hand_id: "rush-4-max".to_string(),
                    played_at: "2026/03/16 10:41:00".to_string(),
                    max_players: 4,
                    seat_count: 4,
                    exact_hero_boundary_ko_share: Some(0.5),
                },
                StageHandFact {
                    hand_id: "ft-first".to_string(),
                    played_at: "2026/03/16 10:42:00".to_string(),
                    max_players: 9,
                    seat_count: 8,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        let boundary = rows.get("rush-4-max").unwrap();
        assert!(boundary.entered_boundary_zone);
        assert_eq!(boundary.entered_boundary_zone_state, "exact");
        assert_eq!(boundary.boundary_resolution_state, "exact");
        assert_eq!(boundary.boundary_candidate_count, 1);
        assert_eq!(boundary.boundary_ko_ev.as_deref(), Some("0.500000"));
        assert_eq!(boundary.boundary_ko_state, "exact");
    }

    #[test]
    fn keeps_last_five_max_when_it_is_still_the_last_non_ft_candidate_before_first_final_table() {
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "rush-4-max".to_string(),
                    played_at: "2026/03/16 10:40:00".to_string(),
                    max_players: 4,
                    seat_count: 4,
                    exact_hero_boundary_ko_share: Some(0.25),
                },
                StageHandFact {
                    hand_id: "rush-5-max".to_string(),
                    played_at: "2026/03/16 10:41:00".to_string(),
                    max_players: 5,
                    seat_count: 5,
                    exact_hero_boundary_ko_share: Some(0.5),
                },
                StageHandFact {
                    hand_id: "ft-first".to_string(),
                    played_at: "2026/03/16 10:42:00".to_string(),
                    max_players: 9,
                    seat_count: 9,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        let boundary = rows.get("rush-5-max").unwrap();
        assert!(boundary.entered_boundary_zone);
        assert_eq!(boundary.entered_boundary_zone_state, "exact");
        assert_eq!(boundary.boundary_resolution_state, "exact");
        assert_eq!(boundary.boundary_candidate_count, 1);
        assert_eq!(boundary.boundary_ko_ev.as_deref(), Some("0.500000"));
        assert_eq!(boundary.boundary_ko_state, "exact");
    }

    #[test]
    fn marks_multiple_last_non_ft_candidates_as_uncertain_boundary_set() {
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "rush-4-max-a".to_string(),
                    played_at: "2026/03/16 10:41:00".to_string(),
                    max_players: 4,
                    seat_count: 4,
                    exact_hero_boundary_ko_share: Some(0.5),
                },
                StageHandFact {
                    hand_id: "rush-2-max-b".to_string(),
                    played_at: "2026/03/16 10:41:00".to_string(),
                    max_players: 2,
                    seat_count: 2,
                    exact_hero_boundary_ko_share: Some(1.0),
                },
                StageHandFact {
                    hand_id: "ft-first".to_string(),
                    played_at: "2026/03/16 10:42:00".to_string(),
                    max_players: 9,
                    seat_count: 7,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        let first = rows.get("rush-4-max-a").unwrap();
        assert!(first.entered_boundary_zone);
        assert_eq!(first.entered_boundary_zone_state, "estimated");
        assert_eq!(first.boundary_resolution_state, "uncertain");
        assert_eq!(first.boundary_candidate_count, 2);
        assert_eq!(
            first.boundary_confidence_class,
            "multi_candidate_same_timestamp"
        );
        assert!(first.boundary_ko_ev.is_none());
        assert_eq!(first.boundary_ko_state, "uncertain");

        let second = rows.get("rush-2-max-b").unwrap();
        assert!(second.entered_boundary_zone);
        assert_eq!(second.entered_boundary_zone_state, "estimated");
        assert_eq!(second.boundary_resolution_state, "uncertain");
        assert_eq!(second.boundary_candidate_count, 2);
        assert!(second.boundary_ko_ev.is_none());
        assert_eq!(second.boundary_ko_state, "uncertain");
    }

    // --- F3-T1: Synthetic edge-case tests for boundary/stage/pre-FT ---

    #[test]
    fn synthetic_no_ft_tournament_has_no_boundary_and_no_stage_predicates() {
        // Tournament where all hands are rush (non-FT): no boundary, no played_ft_hand
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "rush-1".to_string(),
                    played_at: "2026/03/16 10:00:00".to_string(),
                    max_players: 4,
                    seat_count: 4,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "rush-2".to_string(),
                    played_at: "2026/03/16 10:01:00".to_string(),
                    max_players: 3,
                    seat_count: 3,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        for row in rows.values() {
            assert!(!row.played_ft_hand, "no rush hand should be played_ft_hand");
            assert!(
                !row.entered_boundary_zone,
                "no boundary zone in no-FT tournament"
            );
            assert!(row.ft_table_size.is_none(), "ft_table_size null for non-FT");
            assert!(!row.is_ft_hand);
            assert!(!row.is_stage_2);
            assert!(!row.is_stage_3_4);
            assert!(!row.is_stage_4_5);
            assert!(!row.is_stage_5_6);
            assert!(!row.is_stage_6_9);
            assert!(!row.is_boundary_hand);
        }
    }

    #[test]
    fn synthetic_single_candidate_boundary_is_exact() {
        // One rush hand, then one FT hand — boundary resolution is exact
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "boundary".to_string(),
                    played_at: "2026/03/16 10:00:00".to_string(),
                    max_players: 4,
                    seat_count: 4,
                    exact_hero_boundary_ko_share: Some(1.0),
                },
                StageHandFact {
                    hand_id: "ft-first".to_string(),
                    played_at: "2026/03/16 10:01:00".to_string(),
                    max_players: 9,
                    seat_count: 9,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        let boundary = rows.get("boundary").unwrap();
        assert!(boundary.entered_boundary_zone);
        assert!(boundary.is_boundary_hand);
        assert_eq!(boundary.boundary_resolution_state, "exact");
        assert_eq!(boundary.boundary_confidence_class, "single_candidate");
        assert_eq!(boundary.boundary_candidate_count, 1);
        // Boundary KO share should propagate for exact single candidate
        assert!(boundary.boundary_ko_ev.is_some());
    }

    #[test]
    fn synthetic_ft_hand_has_correct_stage_predicates_by_seat_count() {
        // FT hands with varying seat counts to verify all stage predicates
        let rows = build_mbr_stage_resolutions_from_facts(
            Uuid::nil(),
            &[
                StageHandFact {
                    hand_id: "ft-9".to_string(),
                    played_at: "2026/03/16 10:00:00".to_string(),
                    max_players: 9,
                    seat_count: 9,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-6".to_string(),
                    played_at: "2026/03/16 10:01:00".to_string(),
                    max_players: 9,
                    seat_count: 6,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-4".to_string(),
                    played_at: "2026/03/16 10:02:00".to_string(),
                    max_players: 9,
                    seat_count: 4,
                    exact_hero_boundary_ko_share: None,
                },
                StageHandFact {
                    hand_id: "ft-2".to_string(),
                    played_at: "2026/03/16 10:03:00".to_string(),
                    max_players: 9,
                    seat_count: 2,
                    exact_hero_boundary_ko_share: None,
                },
            ],
        );

        // 9-player: is_stage_6_9 = true, is_ft_hand = true
        let ft9 = rows.get("ft-9").unwrap();
        assert!(ft9.played_ft_hand);
        assert!(ft9.is_ft_hand);
        assert!(ft9.is_stage_6_9);
        assert!(!ft9.is_stage_5_6);
        assert!(!ft9.is_stage_3_4);
        assert!(!ft9.is_stage_2);
        assert_eq!(ft9.ft_players_remaining_exact, Some(9));

        // 6-player: is_stage_5_6 = true, is_stage_6_9 = true
        let ft6 = rows.get("ft-6").unwrap();
        assert!(ft6.is_stage_5_6);
        assert!(ft6.is_stage_6_9);
        assert!(!ft6.is_stage_3_4);
        assert_eq!(ft6.ft_players_remaining_exact, Some(6));

        // 4-player: is_stage_3_4 = true, is_stage_4_5 = true
        let ft4 = rows.get("ft-4").unwrap();
        assert!(ft4.is_stage_3_4);
        assert!(ft4.is_stage_4_5);
        assert!(!ft4.is_stage_5_6);
        assert!(!ft4.is_stage_2);
        assert_eq!(ft4.ft_players_remaining_exact, Some(4));

        // 2-player: is_stage_2 = true
        let ft2 = rows.get("ft-2").unwrap();
        assert!(ft2.is_stage_2);
        assert!(!ft2.is_stage_3_4);
        assert_eq!(ft2.ft_players_remaining_exact, Some(2));
    }

    #[test]
    fn synthetic_ft_helper_with_incomplete_start_detects_fewer_than_nine() {
        // First FT hand has only 7 seats — ft_started_incomplete = true
        let helper = build_mbr_tournament_ft_helper_row(
            Uuid::nil(),
            Uuid::nil(),
            &[TournamentFtHelperSourceHand {
                hand_id: Uuid::from_u128(1),
                tournament_hand_order: 1,
                external_hand_id: "ft-1".to_string(),
                hand_started_at_local: "2026/03/16 10:00:00".to_string(),
                played_ft_hand: true,
                played_ft_hand_state: "exact".to_string(),
                ft_table_size: Some(7),
                entered_boundary_zone: false,
                boundary_resolution_state: "exact".to_string(),
                hero_starting_stack: Some(5000),
                big_blind: 200,
            }],
        );

        assert!(helper.reached_ft_exact);
        assert_eq!(helper.first_ft_table_size, Some(7));
        assert_eq!(helper.ft_started_incomplete, Some(true));
        assert_eq!(helper.deepest_ft_size_reached, Some(7));
    }

    #[test]
    fn synthetic_pre_ft_helper_tracks_deepest_ft_size() {
        // Tournament that goes from 9 → 5 → 2 — deepest should be 2
        let helper = build_mbr_tournament_ft_helper_row(
            Uuid::nil(),
            Uuid::nil(),
            &[
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(1),
                    tournament_hand_order: 1,
                    external_hand_id: "ft-a".to_string(),
                    hand_started_at_local: "2026/03/16 10:00:00".to_string(),
                    played_ft_hand: true,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: Some(9),
                    entered_boundary_zone: false,
                    boundary_resolution_state: "exact".to_string(),
                    hero_starting_stack: Some(10000),
                    big_blind: 200,
                },
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(2),
                    tournament_hand_order: 2,
                    external_hand_id: "ft-b".to_string(),
                    hand_started_at_local: "2026/03/16 10:05:00".to_string(),
                    played_ft_hand: true,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: Some(5),
                    entered_boundary_zone: false,
                    boundary_resolution_state: "exact".to_string(),
                    hero_starting_stack: Some(15000),
                    big_blind: 400,
                },
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(3),
                    tournament_hand_order: 3,
                    external_hand_id: "ft-c".to_string(),
                    hand_started_at_local: "2026/03/16 10:10:00".to_string(),
                    played_ft_hand: true,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: Some(2),
                    entered_boundary_zone: false,
                    boundary_resolution_state: "exact".to_string(),
                    hero_starting_stack: Some(25000),
                    big_blind: 800,
                },
            ],
        );

        assert!(helper.reached_ft_exact);
        assert_eq!(helper.first_ft_table_size, Some(9));
        assert_eq!(helper.ft_started_incomplete, Some(false));
        assert_eq!(helper.deepest_ft_size_reached, Some(2));
        assert_eq!(helper.hero_ft_entry_stack_chips, Some(10000));
    }

    #[test]
    fn builds_ft_helper_from_committed_fixture_tournament() {
        let canonical_hands =
            all_hands_from_fixture("GG20260316-0344 - Mystery Battle Royale 25.txt");
        let normalized_hands = canonical_hands
            .iter()
            .map(normalize_hand)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let stage_facts = canonical_hands
            .iter()
            .zip(normalized_hands.iter())
            .map(|(hand, normalized_hand)| StageHandFact {
                hand_id: hand.header.hand_id.clone(),
                played_at: hand.header.played_at.clone(),
                max_players: hand.header.max_players,
                seat_count: hand.seats.len(),
                exact_hero_boundary_ko_share: exact_hero_boundary_ko_share(hand, normalized_hand),
            })
            .collect::<Vec<_>>();
        let stage_rows = build_mbr_stage_resolutions_from_facts(Uuid::nil(), &stage_facts);
        let mut helper_source_hands = canonical_hands
            .iter()
            .enumerate()
            .map(|(index, hand)| {
                build_tournament_ft_helper_source_hand(
                    Uuid::from_u128(index as u128 + 1),
                    hand,
                    stage_rows.get(&hand.header.hand_id).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        helper_source_hands.sort_by(|left, right| {
            left.hand_started_at_local
                .cmp(&right.hand_started_at_local)
                .then_with(|| left.external_hand_id.cmp(&right.external_hand_id))
        });
        for (index, source_hand) in helper_source_hands.iter_mut().enumerate() {
            source_hand.tournament_hand_order = index as i32 + 1;
        }

        let helper_row =
            build_mbr_tournament_ft_helper_row(Uuid::nil(), Uuid::nil(), &helper_source_hands);
        let first_ft_hand = canonical_hands
            .iter()
            .find(|hand| hand.header.hand_id == FIRST_FT_HAND_ID)
            .unwrap();
        let expected_first_ft_hand_id = helper_source_hands
            .iter()
            .find(|hand| hand.external_hand_id == FIRST_FT_HAND_ID)
            .unwrap()
            .hand_id;
        let hero_name = first_ft_hand.hero_name.as_deref().unwrap();
        let expected_hero_stack = first_ft_hand
            .seats
            .iter()
            .find(|seat| seat.player_name == hero_name)
            .unwrap()
            .starting_stack;
        let expected_bb = format!(
            "{:.6}",
            expected_hero_stack as f64 / f64::from(first_ft_hand.header.big_blind)
        );

        assert!(helper_row.reached_ft_exact);
        assert_eq!(helper_row.first_ft_hand_id, Some(expected_first_ft_hand_id));
        assert_eq!(
            helper_row.first_ft_hand_started_local.as_deref(),
            Some(first_ft_hand.header.played_at.as_str())
        );
        assert_eq!(helper_row.first_ft_table_size, Some(9));
        assert_eq!(helper_row.ft_started_incomplete, Some(false));
        assert_eq!(helper_row.deepest_ft_size_reached, Some(2));
        assert_eq!(
            helper_row.hero_ft_entry_stack_chips,
            Some(expected_hero_stack)
        );
        assert_eq!(
            helper_row.hero_ft_entry_stack_bb.as_deref(),
            Some(expected_bb.as_str())
        );
        assert!(helper_row.entered_boundary_zone);
        assert_eq!(helper_row.boundary_resolution_state, "exact");
    }

    #[test]
    fn builds_ft_helper_row_when_no_exact_ft_hand_exists() {
        let helper_row = build_mbr_tournament_ft_helper_row(
            Uuid::nil(),
            Uuid::nil(),
            &[
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(1),
                    tournament_hand_order: 1,
                    external_hand_id: "rush-1".to_string(),
                    hand_started_at_local: "2026/03/16 10:00:00".to_string(),
                    played_ft_hand: false,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: None,
                    entered_boundary_zone: false,
                    boundary_resolution_state: "uncertain".to_string(),
                    hero_starting_stack: Some(1_200),
                    big_blind: 100,
                },
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(2),
                    tournament_hand_order: 2,
                    external_hand_id: "rush-2".to_string(),
                    hand_started_at_local: "2026/03/16 10:01:00".to_string(),
                    played_ft_hand: false,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: None,
                    entered_boundary_zone: true,
                    boundary_resolution_state: "uncertain".to_string(),
                    hero_starting_stack: Some(900),
                    big_blind: 100,
                },
            ],
        );

        assert!(!helper_row.reached_ft_exact);
        assert_eq!(helper_row.first_ft_hand_id, None);
        assert_eq!(helper_row.first_ft_hand_started_local, None);
        assert_eq!(helper_row.first_ft_table_size, None);
        assert_eq!(helper_row.ft_started_incomplete, None);
        assert_eq!(helper_row.deepest_ft_size_reached, None);
        assert_eq!(helper_row.hero_ft_entry_stack_chips, None);
        assert_eq!(helper_row.hero_ft_entry_stack_bb, None);
        assert!(helper_row.entered_boundary_zone);
        assert_eq!(helper_row.boundary_resolution_state, "uncertain");
    }

    #[test]
    fn marks_incomplete_ft_in_ft_helper_when_first_exact_ft_hand_has_fewer_than_nine_players() {
        let helper_row = build_mbr_tournament_ft_helper_row(
            Uuid::nil(),
            Uuid::nil(),
            &[
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(1),
                    tournament_hand_order: 1,
                    external_hand_id: "rush".to_string(),
                    hand_started_at_local: "2026/03/16 10:00:00".to_string(),
                    played_ft_hand: false,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: None,
                    entered_boundary_zone: true,
                    boundary_resolution_state: "exact".to_string(),
                    hero_starting_stack: Some(4_000),
                    big_blind: 100,
                },
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(2),
                    tournament_hand_order: 2,
                    external_hand_id: "ft-6".to_string(),
                    hand_started_at_local: "2026/03/16 10:01:00".to_string(),
                    played_ft_hand: true,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: Some(6),
                    entered_boundary_zone: false,
                    boundary_resolution_state: "exact".to_string(),
                    hero_starting_stack: Some(3_600),
                    big_blind: 100,
                },
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(3),
                    tournament_hand_order: 3,
                    external_hand_id: "ft-3".to_string(),
                    hand_started_at_local: "2026/03/16 10:02:00".to_string(),
                    played_ft_hand: true,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: Some(3),
                    entered_boundary_zone: false,
                    boundary_resolution_state: "exact".to_string(),
                    hero_starting_stack: Some(2_100),
                    big_blind: 100,
                },
            ],
        );

        assert!(helper_row.reached_ft_exact);
        assert_eq!(helper_row.first_ft_table_size, Some(6));
        assert_eq!(helper_row.ft_started_incomplete, Some(true));
        assert_eq!(helper_row.deepest_ft_size_reached, Some(3));
        assert_eq!(helper_row.hero_ft_entry_stack_chips, Some(3_600));
        assert_eq!(
            helper_row.hero_ft_entry_stack_bb.as_deref(),
            Some("36.000000")
        );
    }

    #[test]
    fn keeps_uncertain_boundary_state_in_ft_helper_row() {
        let helper_row = build_mbr_tournament_ft_helper_row(
            Uuid::nil(),
            Uuid::nil(),
            &[
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(1),
                    tournament_hand_order: 1,
                    external_hand_id: "rush-a".to_string(),
                    hand_started_at_local: "2026/03/16 10:00:00".to_string(),
                    played_ft_hand: false,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: None,
                    entered_boundary_zone: true,
                    boundary_resolution_state: "uncertain".to_string(),
                    hero_starting_stack: Some(2_000),
                    big_blind: 100,
                },
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(2),
                    tournament_hand_order: 2,
                    external_hand_id: "rush-b".to_string(),
                    hand_started_at_local: "2026/03/16 10:00:00".to_string(),
                    played_ft_hand: false,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: None,
                    entered_boundary_zone: true,
                    boundary_resolution_state: "uncertain".to_string(),
                    hero_starting_stack: Some(1_900),
                    big_blind: 100,
                },
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(3),
                    tournament_hand_order: 3,
                    external_hand_id: "ft".to_string(),
                    hand_started_at_local: "2026/03/16 10:01:00".to_string(),
                    played_ft_hand: true,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: Some(9),
                    entered_boundary_zone: false,
                    boundary_resolution_state: "uncertain".to_string(),
                    hero_starting_stack: Some(1_800),
                    big_blind: 100,
                },
            ],
        );

        assert!(helper_row.reached_ft_exact);
        assert!(helper_row.entered_boundary_zone);
        assert_eq!(helper_row.boundary_resolution_state, "uncertain");
        assert_eq!(helper_row.first_ft_table_size, Some(9));
    }

    #[test]
    fn ft_helper_prefers_tournament_hand_order_over_local_timestamp() {
        let helper_row = build_mbr_tournament_ft_helper_row(
            Uuid::nil(),
            Uuid::nil(),
            &[
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(1),
                    tournament_hand_order: 2,
                    external_hand_id: "ft-late-clock".to_string(),
                    hand_started_at_local: "2026/03/16 10:01:00".to_string(),
                    played_ft_hand: true,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: Some(9),
                    entered_boundary_zone: false,
                    boundary_resolution_state: "exact".to_string(),
                    hero_starting_stack: Some(4_000),
                    big_blind: 100,
                },
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(2),
                    tournament_hand_order: 1,
                    external_hand_id: "ft-early-order".to_string(),
                    hand_started_at_local: "2026/03/16 10:05:00".to_string(),
                    played_ft_hand: true,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: Some(8),
                    entered_boundary_zone: false,
                    boundary_resolution_state: "exact".to_string(),
                    hero_starting_stack: Some(3_200),
                    big_blind: 100,
                },
            ],
        );

        assert_eq!(helper_row.first_ft_hand_id, Some(Uuid::from_u128(2)));
        assert_eq!(helper_row.first_ft_table_size, Some(8));
        assert_eq!(
            helper_row.first_ft_hand_started_local.as_deref(),
            Some("2026/03/16 10:05:00")
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0004_adds_schema_v2_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let table_contract_rows = client
            .query(
                "SELECT table_schema, table_name
                 FROM information_schema.tables
                 WHERE (table_schema, table_name) IN (
                     ('core', 'player_aliases'),
                     ('import', 'source_file_members'),
                     ('import', 'job_attempts'),
                     ('analytics', 'feature_catalog'),
                     ('analytics', 'stat_catalog'),
                     ('analytics', 'stat_dependencies'),
                     ('analytics', 'materialization_policies')
                 )
                 ORDER BY table_schema, table_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        assert_eq!(
            table_contract_rows,
            vec![
                ("analytics".to_string(), "feature_catalog".to_string()),
                (
                    "analytics".to_string(),
                    "materialization_policies".to_string()
                ),
                ("analytics".to_string(), "stat_catalog".to_string()),
                ("analytics".to_string(), "stat_dependencies".to_string()),
                ("core".to_string(), "player_aliases".to_string()),
                ("import".to_string(), "job_attempts".to_string()),
                ("import".to_string(), "source_file_members".to_string()),
            ]
        );

        let time_columns = client
            .query(
                "SELECT table_schema, table_name, column_name
                 FROM information_schema.columns
                 WHERE (table_schema, table_name, column_name) IN (
                     ('core', 'tournaments', 'started_at_raw'),
                     ('core', 'tournaments', 'started_at_local'),
                     ('core', 'tournaments', 'started_at_tz_provenance'),
                     ('core', 'hands', 'hand_started_at_raw'),
                     ('core', 'hands', 'hand_started_at_local'),
                     ('core', 'hands', 'hand_started_at_tz_provenance')
                 )
                 ORDER BY table_schema, table_name, column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, String>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            time_columns,
            vec![
                (
                    "core".to_string(),
                    "hands".to_string(),
                    "hand_started_at_local".to_string()
                ),
                (
                    "core".to_string(),
                    "hands".to_string(),
                    "hand_started_at_raw".to_string()
                ),
                (
                    "core".to_string(),
                    "hands".to_string(),
                    "hand_started_at_tz_provenance".to_string(),
                ),
                (
                    "core".to_string(),
                    "tournaments".to_string(),
                    "started_at_local".to_string()
                ),
                (
                    "core".to_string(),
                    "tournaments".to_string(),
                    "started_at_raw".to_string()
                ),
                (
                    "core".to_string(),
                    "tournaments".to_string(),
                    "started_at_tz_provenance".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn migration_filenames_use_unique_numeric_prefixes() {
        let migration_dir = fixture_path("../../migrations");
        let mut prefixes = BTreeSet::new();
        let mut duplicates = Vec::new();

        for entry in fs::read_dir(migration_dir).unwrap() {
            let entry = entry.unwrap();
            let file_name = entry.file_name().into_string().unwrap();
            let Some((prefix, _rest)) = file_name.split_once('_') else {
                continue;
            };
            if !prefixes.insert(prefix.to_string()) {
                duplicates.push(file_name);
            }
        }

        assert!(
            duplicates.is_empty(),
            "migration numeric prefixes must be unique, duplicates: {duplicates:?}"
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0011_adds_boundary_resolution_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let columns = client
            .query(
                "SELECT column_name
                 FROM information_schema.columns
                 WHERE table_schema = 'derived'
                   AND table_name = 'mbr_stage_resolution'
                   AND column_name IN (
                       'boundary_resolution_state',
                       'boundary_candidate_count',
                       'boundary_resolution_method',
                       'boundary_confidence_class'
                   )
                 ORDER BY column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert_eq!(
            columns,
            vec![
                "boundary_candidate_count".to_string(),
                "boundary_confidence_class".to_string(),
                "boundary_resolution_method".to_string(),
                "boundary_resolution_state".to_string(),
            ]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0015_adds_ko_event_vs_money_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let columns = client
            .query(
                "SELECT column_name
                 FROM information_schema.columns
                 WHERE table_schema = 'derived'
                   AND table_name = 'hand_eliminations'
                   AND column_name IN (
                       'ko_pot_resolution_type',
                       'money_share_model_state',
                       'money_share_exact_fraction',
                       'money_share_estimated_min_fraction',
                       'money_share_estimated_ev_fraction',
                       'money_share_estimated_max_fraction'
                   )
                 ORDER BY column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert_eq!(
            columns,
            vec![
                "ko_pot_resolution_type".to_string(),
                "money_share_estimated_ev_fraction".to_string(),
                "money_share_estimated_max_fraction".to_string(),
                "money_share_estimated_min_fraction".to_string(),
                "money_share_exact_fraction".to_string(),
                "money_share_model_state".to_string(),
            ]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0019_adds_unified_settlement_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let columns = client
            .query(
                "SELECT column_name
                 FROM information_schema.columns
                 WHERE table_schema = 'derived'
                   AND table_name = 'hand_state_resolutions'
                   AND column_name IN (
                       'settlement_state',
                       'settlement',
                       'invariant_issues',
                       'invariant_errors',
                       'uncertain_reason_codes'
                   )
                 ORDER BY column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert_eq!(
            columns,
            vec![
                "invariant_issues".to_string(),
                "settlement".to_string(),
                "settlement_state".to_string(),
            ]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0020_adds_hand_eliminations_v2_contract() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let columns = client
            .query(
                "SELECT column_name
                 FROM information_schema.columns
                 WHERE table_schema = 'derived'
                   AND table_name = 'hand_eliminations'
                   AND column_name IN (
                       'pots_participated_by_busted',
                       'pots_causing_bust',
                       'last_busting_pot_no',
                       'ko_winner_set',
                       'ko_share_fraction_by_winner',
                       'elimination_certainty_state',
                       'ko_certainty_state'
                   )
                 ORDER BY column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert_eq!(
            columns,
            vec![
                "elimination_certainty_state".to_string(),
                "ko_certainty_state".to_string(),
                "ko_share_fraction_by_winner".to_string(),
                "ko_winner_set".to_string(),
                "last_busting_pot_no".to_string(),
                "pots_causing_bust".to_string(),
                "pots_participated_by_busted".to_string(),
            ]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0021_adds_ingest_runtime_runner_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let table_contract_rows = client
            .query(
                "SELECT table_schema, table_name
                 FROM information_schema.tables
                 WHERE (table_schema, table_name) IN (
                     ('import', 'ingest_bundles'),
                     ('import', 'ingest_bundle_files')
                 )
                 ORDER BY table_schema, table_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        assert_eq!(
            table_contract_rows,
            vec![
                ("import".to_string(), "ingest_bundle_files".to_string()),
                ("import".to_string(), "ingest_bundles".to_string()),
            ]
        );

        let import_job_columns = client
            .query(
                "SELECT column_name, is_nullable
                 FROM information_schema.columns
                 WHERE table_schema = 'import'
                   AND table_name = 'import_jobs'
                   AND column_name IN ('bundle_id', 'bundle_file_id', 'job_kind', 'source_file_id')
                 ORDER BY column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        assert_eq!(
            import_job_columns,
            vec![
                ("bundle_file_id".to_string(), "YES".to_string()),
                ("bundle_id".to_string(), "YES".to_string()),
                ("job_kind".to_string(), "NO".to_string()),
                ("source_file_id".to_string(), "YES".to_string()),
            ]
        );

        let status_stage_constraints = client
            .query(
                "SELECT c.conname, pg_get_constraintdef(c.oid)
                 FROM pg_constraint c
                 INNER JOIN pg_class t ON t.oid = c.conrelid
                 INNER JOIN pg_namespace n ON n.oid = t.relnamespace
                 WHERE n.nspname = 'import'
                   AND t.relname IN ('import_jobs', 'job_attempts')
                   AND c.contype = 'c'
                   AND (
                       c.conname LIKE '%status%'
                       OR c.conname LIKE '%stage%'
                       OR c.conname LIKE '%job_kind%'
                   )
                 ORDER BY c.conname",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        let joined_constraint_defs = status_stage_constraints
            .iter()
            .map(|(_, definition)| definition.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            joined_constraint_defs.contains("failed_retriable")
                && joined_constraint_defs.contains("failed_terminal")
                && joined_constraint_defs.contains("bundle_finalize")
                && joined_constraint_defs.contains("materialize_refresh"),
            "missing expected ingest runner constraint values: {joined_constraint_defs}"
        );

        let indexes = client
            .query(
                "SELECT indexname
                 FROM pg_indexes
                 WHERE schemaname = 'import'
                   AND indexname IN (
                       'idx_import_jobs_bundle_status',
                       'idx_import_jobs_claim',
                       'uniq_import_jobs_bundle_finalize',
                       'uniq_import_jobs_bundle_file_ingest'
                   )
                 ORDER BY indexname",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert_eq!(
            indexes,
            vec![
                "idx_import_jobs_bundle_status".to_string(),
                "idx_import_jobs_claim".to_string(),
                "uniq_import_jobs_bundle_file_ingest".to_string(),
                "uniq_import_jobs_bundle_finalize".to_string(),
            ]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0027_adds_hand_ko_attempt_and_opportunity_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let tables = client
            .query(
                "SELECT table_name
                 FROM information_schema.tables
                 WHERE table_schema = 'derived'
                   AND table_name IN ('hand_ko_attempts', 'hand_ko_opportunities')
                 ORDER BY table_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert_eq!(
            tables,
            vec![
                "hand_ko_attempts".to_string(),
                "hand_ko_opportunities".to_string(),
            ]
        );

        let columns = client
            .query(
                "SELECT table_name, column_name
                 FROM information_schema.columns
                 WHERE table_schema = 'derived'
                   AND (
                        (table_name = 'hand_ko_attempts' AND column_name IN (
                            'hand_id',
                            'player_profile_id',
                            'hero_seat_no',
                            'target_seat_no',
                            'target_player_name',
                            'attempt_kind',
                            'street',
                            'source_sequence_no',
                            'is_forced_all_in'
                        ))
                        OR
                        (table_name = 'hand_ko_opportunities' AND column_name IN (
                            'hand_id',
                            'player_profile_id',
                            'hero_seat_no',
                            'target_seat_no',
                            'target_player_name',
                            'opportunity_kind',
                            'street',
                            'source_sequence_no',
                            'is_forced_all_in'
                        ))
                   )
                 ORDER BY table_name, column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        assert_eq!(
            columns,
            vec![
                ("hand_ko_attempts".to_string(), "attempt_kind".to_string()),
                ("hand_ko_attempts".to_string(), "hand_id".to_string()),
                ("hand_ko_attempts".to_string(), "hero_seat_no".to_string()),
                (
                    "hand_ko_attempts".to_string(),
                    "is_forced_all_in".to_string()
                ),
                (
                    "hand_ko_attempts".to_string(),
                    "player_profile_id".to_string()
                ),
                (
                    "hand_ko_attempts".to_string(),
                    "source_sequence_no".to_string()
                ),
                ("hand_ko_attempts".to_string(), "street".to_string()),
                (
                    "hand_ko_attempts".to_string(),
                    "target_player_name".to_string()
                ),
                ("hand_ko_attempts".to_string(), "target_seat_no".to_string()),
                ("hand_ko_opportunities".to_string(), "hand_id".to_string()),
                (
                    "hand_ko_opportunities".to_string(),
                    "hero_seat_no".to_string()
                ),
                (
                    "hand_ko_opportunities".to_string(),
                    "is_forced_all_in".to_string()
                ),
                (
                    "hand_ko_opportunities".to_string(),
                    "opportunity_kind".to_string()
                ),
                (
                    "hand_ko_opportunities".to_string(),
                    "player_profile_id".to_string()
                ),
                (
                    "hand_ko_opportunities".to_string(),
                    "source_sequence_no".to_string()
                ),
                ("hand_ko_opportunities".to_string(), "street".to_string()),
                (
                    "hand_ko_opportunities".to_string(),
                    "target_player_name".to_string()
                ),
                (
                    "hand_ko_opportunities".to_string(),
                    "target_seat_no".to_string()
                ),
            ]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0028_adds_pair_aware_ingest_queue_contract() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let columns = client
            .query(
                "SELECT column_name, is_nullable
                 FROM information_schema.columns
                 WHERE table_schema = 'import'
                   AND table_name = 'import_jobs'
                   AND column_name = 'depends_on_job_id'",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        assert_eq!(
            columns,
            vec![("depends_on_job_id".to_string(), "YES".to_string())]
        );

        let indexes = client
            .query(
                "SELECT indexname
                 FROM pg_indexes
                 WHERE schemaname = 'import'
                   AND indexname = 'idx_import_jobs_claim_dependency'",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert_eq!(
            indexes,
            vec!["idx_import_jobs_claim_dependency".to_string()]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0029_removes_legacy_file_fragment_source_uniqueness() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let indexes = client
            .query(
                "SELECT indexname
                 FROM pg_indexes
                 WHERE schemaname = 'import'
                   AND tablename = 'file_fragments'
                   AND indexname IN (
                       'idx_file_fragments_source_fragment_unique',
                       'uniq_file_fragments_member_fragment'
                   )
                 ORDER BY indexname",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert_eq!(
            indexes,
            vec!["uniq_file_fragments_member_fragment".to_string()]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0022_adds_web_upload_member_ingest_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let table_contract_rows = client
            .query(
                "SELECT table_schema, table_name
                 FROM information_schema.tables
                 WHERE (table_schema, table_name) IN (
                     ('import', 'ingest_events')
                 )
                 ORDER BY table_schema, table_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        assert_eq!(
            table_contract_rows,
            vec![("import".to_string(), "ingest_events".to_string())]
        );

        let ingest_bundle_file_columns = client
            .query(
                "SELECT column_name, is_nullable
                 FROM information_schema.columns
                 WHERE table_schema = 'import'
                   AND table_name = 'ingest_bundle_files'
                   AND column_name IN ('source_file_id', 'source_file_member_id')
                 ORDER BY column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        assert_eq!(
            ingest_bundle_file_columns,
            vec![
                ("source_file_id".to_string(), "NO".to_string()),
                ("source_file_member_id".to_string(), "NO".to_string()),
            ]
        );

        let import_job_columns = client
            .query(
                "SELECT column_name, is_nullable
                 FROM information_schema.columns
                 WHERE table_schema = 'import'
                   AND table_name = 'import_jobs'
                   AND column_name IN ('bundle_file_id', 'job_kind', 'source_file_id', 'source_file_member_id')
                 ORDER BY column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        assert_eq!(
            import_job_columns,
            vec![
                ("bundle_file_id".to_string(), "YES".to_string()),
                ("job_kind".to_string(), "NO".to_string()),
                ("source_file_id".to_string(), "YES".to_string()),
                ("source_file_member_id".to_string(), "YES".to_string()),
            ]
        );

        let file_fragment_columns = client
            .query(
                "SELECT column_name, is_nullable
                 FROM information_schema.columns
                 WHERE table_schema = 'import'
                   AND table_name = 'file_fragments'
                   AND column_name IN ('source_file_id', 'source_file_member_id')
                 ORDER BY column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        assert_eq!(
            file_fragment_columns,
            vec![
                ("source_file_id".to_string(), "NO".to_string()),
                ("source_file_member_id".to_string(), "NO".to_string()),
            ]
        );

        let ingest_event_columns = client
            .query(
                "SELECT column_name, is_nullable
                 FROM information_schema.columns
                 WHERE table_schema = 'import'
                   AND table_name = 'ingest_events'
                   AND column_name IN (
                       'bundle_id',
                       'bundle_file_id',
                       'event_kind',
                       'message',
                       'payload',
                       'sequence_no'
                   )
                 ORDER BY column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>();

        assert_eq!(
            ingest_event_columns,
            vec![
                ("bundle_file_id".to_string(), "YES".to_string()),
                ("bundle_id".to_string(), "NO".to_string()),
                ("event_kind".to_string(), "NO".to_string()),
                ("message".to_string(), "NO".to_string()),
                ("payload".to_string(), "NO".to_string()),
                ("sequence_no".to_string(), "NO".to_string()),
            ]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0024_adds_user_timezone_and_gg_time_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let timezone_columns = client
            .query(
                "SELECT table_schema, table_name, column_name, is_nullable
                 FROM information_schema.columns
                 WHERE (table_schema, table_name, column_name) IN (
                     ('auth', 'users', 'timezone_name'),
                     ('core', 'tournaments', 'started_at_tz_provenance'),
                     ('core', 'hands', 'hand_started_at_tz_provenance')
                 )
                 ORDER BY table_schema, table_name, column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, String>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                    row.get::<_, String>(3),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            timezone_columns,
            vec![
                (
                    "auth".to_string(),
                    "users".to_string(),
                    "timezone_name".to_string(),
                    "YES".to_string(),
                ),
                (
                    "core".to_string(),
                    "hands".to_string(),
                    "hand_started_at_tz_provenance".to_string(),
                    "YES".to_string(),
                ),
                (
                    "core".to_string(),
                    "tournaments".to_string(),
                    "started_at_tz_provenance".to_string(),
                    "YES".to_string(),
                ),
            ]
        );

        let constraint_defs = client
            .query(
                "SELECT conname, pg_get_constraintdef(oid)
                 FROM pg_constraint
                 WHERE conrelid IN ('auth.users'::regclass, 'core.tournaments'::regclass, 'core.hands'::regclass)
                 ORDER BY conname",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| format!("{} {}", row.get::<_, String>(0), row.get::<_, String>(1)))
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            constraint_defs.contains("auth_users_timezone_name_not_blank"),
            "expected auth.users timezone constraint, got:\n{constraint_defs}"
        );
        assert!(
            constraint_defs.contains("gg_user_timezone"),
            "expected gg_user_timezone provenance constraint, got:\n{constraint_defs}"
        );
        assert!(
            constraint_defs.contains("gg_user_timezone_missing"),
            "expected gg_user_timezone_missing provenance constraint, got:\n{constraint_defs}"
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0014_adds_stage_predicate_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        let columns = client
            .query(
                "SELECT column_name
                 FROM information_schema.columns
                 WHERE table_schema = 'derived'
                   AND table_name = 'mbr_stage_resolution'
                   AND column_name IN (
                       'is_ft_hand',
                       'ft_players_remaining_exact',
                       'is_stage_2',
                       'is_stage_3_4',
                       'is_stage_4_5',
                       'is_stage_5_6',
                       'is_stage_6_9',
                       'is_boundary_hand'
                   )
                 ORDER BY column_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert_eq!(
            columns,
            vec![
                "ft_players_remaining_exact".to_string(),
                "is_boundary_hand".to_string(),
                "is_ft_hand".to_string(),
                "is_stage_2".to_string(),
                "is_stage_3_4".to_string(),
                "is_stage_4_5".to_string(),
                "is_stage_5_6".to_string(),
                "is_stage_6_9".to_string(),
            ]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0004_adds_composite_integrity_constraints() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);

        client
            .batch_execute(
                "BEGIN;
                 INSERT INTO org.organizations (id, name) VALUES ('00000000-0000-0000-0000-000000000001', 'schema-test-org') ON CONFLICT (id) DO NOTHING;
                 INSERT INTO auth.users (id, email, auth_provider, status) VALUES ('00000000-0000-0000-0000-000000000002', 'schema-test@example.com', 'seed', 'active') ON CONFLICT (id) DO NOTHING;
                 INSERT INTO core.rooms (id, code, name) VALUES ('00000000-0000-0000-0000-000000000003', 'gg-schema-test', 'GG Schema Test') ON CONFLICT (id) DO NOTHING;
                 INSERT INTO core.formats (id, room_id, code, name, max_players) VALUES ('00000000-0000-0000-0000-000000000004', '00000000-0000-0000-0000-000000000003', 'mbr-schema-test', 'MBR Schema Test', 18) ON CONFLICT (id) DO NOTHING;
                 INSERT INTO core.player_profiles (id, organization_id, owner_user_id, room, network, screen_name) VALUES ('00000000-0000-0000-0000-000000000005', '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000002', 'gg', 'gg', 'SchemaHero') ON CONFLICT (id) DO NOTHING;
                 INSERT INTO import.source_files (id, organization_id, uploaded_by_user_id, owner_user_id, player_profile_id, room, file_kind, sha256, original_filename, byte_size, storage_uri)
                 VALUES ('00000000-0000-0000-0000-000000000006', '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000002', '00000000-0000-0000-0000-000000000002', '00000000-0000-0000-0000-000000000005', 'gg', 'hh', repeat('a', 64), 'schema-test.txt', 1, 'local://schema-test.txt') ON CONFLICT DO NOTHING;
                 INSERT INTO core.tournaments (id, organization_id, player_profile_id, room_id, format_id, external_tournament_id, buyin_total, buyin_prize_component, buyin_bounty_component, fee_component, currency, max_players, source_summary_file_id)
                 VALUES ('00000000-0000-0000-0000-000000000007', '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000005', '00000000-0000-0000-0000-000000000003', '00000000-0000-0000-0000-000000000004', 'schema-tournament', 25.00, 12.50, 10.50, 2.00, 'USD', 18, '00000000-0000-0000-0000-000000000006') ON CONFLICT DO NOTHING;
                 INSERT INTO import.file_fragments (id, source_file_id, fragment_index, external_hand_id, kind, raw_text, sha256)
                 VALUES ('00000000-0000-0000-0000-000000000008', '00000000-0000-0000-0000-000000000006', 0, 'schema-hand', 'hand', 'raw', repeat('b', 64)) ON CONFLICT DO NOTHING;
                 INSERT INTO core.hands (id, organization_id, player_profile_id, tournament_id, source_file_id, external_hand_id, table_name, table_max_seats, dealer_seat_no, small_blind, big_blind, ante, currency, raw_fragment_id)
                 VALUES ('00000000-0000-0000-0000-000000000009', '00000000-0000-0000-0000-000000000001', '00000000-0000-0000-0000-000000000005', '00000000-0000-0000-0000-000000000007', '00000000-0000-0000-0000-000000000006', 'schema-hand', '1', 9, 1, 100, 200, 25, 'USD', '00000000-0000-0000-0000-000000000008') ON CONFLICT DO NOTHING;
                 INSERT INTO core.hand_seats (hand_id, seat_no, player_name, starting_stack, is_hero, is_button)
                 VALUES ('00000000-0000-0000-0000-000000000009', 1, 'SchemaHero', 10000, true, true) ON CONFLICT DO NOTHING;
                 INSERT INTO core.hand_pots (hand_id, pot_no, pot_type, amount)
                 VALUES ('00000000-0000-0000-0000-000000000009', 1, 'main', 300) ON CONFLICT DO NOTHING;
                 COMMIT;",
            )
            .unwrap();

        let seat_fk_error = client
            .execute(
                "INSERT INTO core.hand_showdowns (
                    hand_id,
                    seat_no,
                    shown_cards,
                    best5_cards,
                    hand_rank_class,
                    hand_rank_value
                )
                 VALUES ($1, $2, ARRAY['As', 'Ah'], ARRAY['As', 'Ah', 'Kd', 'Qc', 'Jd'], 'pair', 1)",
                &[&Uuid::parse_str("00000000-0000-0000-0000-000000000009").unwrap(), &2_i32],
            )
            .unwrap_err();
        assert_eq!(
            seat_fk_error.code(),
            Some(&postgres::error::SqlState::FOREIGN_KEY_VIOLATION)
        );
        assert_eq!(
            seat_fk_error
                .as_db_error()
                .and_then(|error| error.constraint()),
            Some("fk_hand_showdowns_hand_seat")
        );

        let pot_fk_error = client
            .execute(
                "INSERT INTO core.hand_pot_winners (
                    hand_id,
                    pot_no,
                    seat_no,
                    share_amount
                 )
                 VALUES ($1, $2, $3, $4)",
                &[
                    &Uuid::parse_str("00000000-0000-0000-0000-000000000009").unwrap(),
                    &2_i32,
                    &1_i32,
                    &300_i64,
                ],
            )
            .unwrap_err();
        assert_eq!(
            pot_fk_error.code(),
            Some(&postgres::error::SqlState::FOREIGN_KEY_VIOLATION)
        );
        assert_eq!(
            pot_fk_error
                .as_db_error()
                .and_then(|error| error.constraint()),
            Some("fk_hand_pot_winners_hand_pot")
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn seed_populates_runtime_catalog_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);
        apply_sql_file(
            &mut client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let feature_catalog = client
            .query(
                "SELECT feature_key, feature_version, table_family, value_kind
                 FROM analytics.feature_catalog
                 ORDER BY feature_key",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, String>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                    row.get::<_, String>(3),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            feature_catalog,
            vec![
                (
                    "best_hand_class".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string()
                ),
                (
                    "best_hand_rank_value".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "num".to_string(),
                    "double".to_string()
                ),
                (
                    "certainty_state".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string()
                ),
                (
                    "draw_category".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string()
                ),
                (
                    "ft_players_remaining_exact".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "num".to_string(),
                    "double".to_string()
                ),
                (
                    "ft_stage_bucket".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string()
                ),
                (
                    "ft_table_size".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "num".to_string(),
                    "double".to_string()
                ),
                (
                    "has_air".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "has_exact_ko_event".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "has_sidepot_ko_event".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "has_split_ko_event".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "hero_exact_ko_event_count".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "num".to_string(),
                    "double".to_string()
                ),
                (
                    "hero_sidepot_ko_event_count".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "num".to_string(),
                    "double".to_string()
                ),
                (
                    "hero_split_ko_event_count".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "num".to_string(),
                    "double".to_string()
                ),
                (
                    "is_boundary_hand".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "is_ft_hand".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "is_stage_2".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "is_stage_3_4".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "is_stage_4_5".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "is_stage_5_6".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "is_stage_6_9".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "made_hand_category".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string()
                ),
                (
                    "missed_flush_draw".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "missed_straight_draw".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "overcards_count".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "num".to_string(),
                    "double".to_string()
                ),
                (
                    "played_ft_hand".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string()
                ),
                (
                    "starter_hand_class".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string()
                ),
            ]
        );

        let stat_catalog = client
            .query(
                "SELECT stat_key, stat_family, exactness_class
                 FROM analytics.stat_catalog
                 ORDER BY stat_key",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, String>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            stat_catalog,
            vec![
                (
                    "avg_finish_place".to_string(),
                    "seed_snapshot".to_string(),
                    "exact".to_string()
                ),
                (
                    "avg_finish_place_ft".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "avg_finish_place_no_ft".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "avg_ft_initial_stack_bb".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "avg_ft_initial_stack_chips".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "avg_ko_attempts_per_ft".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "avg_ko_event_per_tournament".to_string(),
                    "seed_snapshot".to_string(),
                    "exact".to_string()
                ),
                (
                    "big_ko_x1_5_count".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "big_ko_x10_count".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "big_ko_x100_count".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "big_ko_x1000_count".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "big_ko_x10000_count".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "big_ko_x2_count".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "deep_ft_avg_stack_bb".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "deep_ft_avg_stack_chips".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "deep_ft_reach_percent".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "deep_ft_roi_pct".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "early_ft_bust_count".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "early_ft_bust_per_tournament".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "early_ft_ko_event_count".to_string(),
                    "seed_snapshot".to_string(),
                    "exact".to_string()
                ),
                (
                    "early_ft_ko_event_per_tournament".to_string(),
                    "seed_snapshot".to_string(),
                    "exact".to_string()
                ),
                (
                    "final_table_reach_percent".to_string(),
                    "seed_snapshot".to_string(),
                    "exact".to_string()
                ),
                (
                    "ft_stack_conversion".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ft_stack_conversion_3_4".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ft_stack_conversion_3_4_attempts".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ft_stack_conversion_5_6".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ft_stack_conversion_5_6_attempts".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ft_stack_conversion_7_9".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ft_stack_conversion_7_9_attempts".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "incomplete_ft_percent".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "itm_percent".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_attempts_success_rate".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_contribution_adjusted_percent".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "ko_contribution_percent".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_luck_money_delta".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "ko_stage_2_3_attempts_per_tournament".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_2_3_event_count".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_2_3_money_total".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "ko_stage_3_4_attempts_per_tournament".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_3_4_event_count".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_3_4_money_total".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "ko_stage_4_5_attempts_per_tournament".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_4_5_event_count".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_4_5_money_total".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "ko_stage_5_6_attempts_per_tournament".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_5_6_event_count".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_5_6_money_total".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "ko_stage_6_9_event_count".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_6_9_money_total".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "ko_stage_7_9_attempts_per_tournament".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_7_9_event_count".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "ko_stage_7_9_money_total".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "pre_ft_chipev".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "pre_ft_ko_count".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "roi_adj_pct".to_string(),
                    "canonical_query_time".to_string(),
                    "estimated".to_string()
                ),
                (
                    "roi_on_ft_pct".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "roi_pct".to_string(),
                    "seed_snapshot".to_string(),
                    "exact".to_string()
                ),
                (
                    "total_ko_event_count".to_string(),
                    "seed_snapshot".to_string(),
                    "exact".to_string()
                ),
                (
                    "winnings_from_itm".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
                (
                    "winnings_from_ko_total".to_string(),
                    "canonical_query_time".to_string(),
                    "exact".to_string()
                ),
            ]
        );

        let dependency_count: i64 = client
            .query_one("SELECT COUNT(*) FROM analytics.stat_dependencies", &[])
            .unwrap()
            .get(0);
        let policy_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM analytics.materialization_policies",
                &[],
            )
            .unwrap()
            .get(0);

        assert!(dependency_count >= 5);
        assert!(policy_count >= 18);
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn seed_and_migrations_populate_street_runtime_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_core_schema_migrations(&mut client);
        apply_sql_file(
            &mut client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let street_tables = client
            .query(
                "SELECT table_name
                 FROM information_schema.tables
                 WHERE table_schema = 'analytics'
                   AND table_name IN (
                       'player_street_bool_features',
                       'player_street_num_features',
                       'player_street_enum_features'
                   )
                 ORDER BY table_name",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();
        assert_eq!(
            street_tables,
            vec![
                "player_street_bool_features".to_string(),
                "player_street_enum_features".to_string(),
                "player_street_num_features".to_string(),
            ]
        );

        let street_catalog = client
            .query(
                "SELECT feature_key, feature_version, table_family, value_kind
                 FROM analytics.feature_catalog
                 WHERE feature_key IN (
                     'best_hand_class',
                     'best_hand_rank_value',
                     'made_hand_category',
                     'draw_category',
                     'overcards_count',
                     'starter_hand_class',
                     'has_air',
                     'missed_flush_draw',
                     'missed_straight_draw',
                     'certainty_state'
                 )
                 ORDER BY feature_key",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, String>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                    row.get::<_, String>(3),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            street_catalog,
            vec![
                (
                    "best_hand_class".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string(),
                ),
                (
                    "best_hand_rank_value".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "num".to_string(),
                    "double".to_string(),
                ),
                (
                    "certainty_state".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string(),
                ),
                (
                    "draw_category".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string(),
                ),
                (
                    "has_air".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string(),
                ),
                (
                    "made_hand_category".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string(),
                ),
                (
                    "missed_flush_draw".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string(),
                ),
                (
                    "missed_straight_draw".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "bool".to_string(),
                    "bool".to_string(),
                ),
                (
                    "overcards_count".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "num".to_string(),
                    "double".to_string(),
                ),
                (
                    "starter_hand_class".to_string(),
                    "mbr_runtime_v1".to_string(),
                    "enum".to_string(),
                    "enum".to_string(),
                ),
            ]
        );

        let street_policy_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM analytics.materialization_policies
                 WHERE target_kind = 'feature'
                   AND target_version = 'mbr_runtime_v1'
                   AND target_key IN (
                       'best_hand_class',
                       'best_hand_rank_value',
                       'made_hand_category',
                       'draw_category',
                       'overcards_count',
                       'has_air',
                       'missed_flush_draw',
                       'missed_straight_draw',
                       'certainty_state'
                   )",
                &[],
            )
            .unwrap()
            .get(0);
        assert_eq!(street_policy_count, 9);
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_persists_time_provenance_members_and_alias_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        drop(setup_client);

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");

        let ts_report = import_path(&ts_path).unwrap();
        let hh_report = import_path(&hh_path).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let player_profile_id = dev_player_profile_id(&mut client);

        let tournament_time = client
            .query_one(
                "SELECT
                    started_at::text,
                    started_at_raw,
                    started_at_local::text,
                    started_at_tz_provenance
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap();

        assert_eq!(tournament_time.get::<_, Option<String>>(0), None);
        assert_eq!(
            tournament_time.get::<_, Option<String>>(1).as_deref(),
            Some("2026/03/16 10:44:11")
        );
        assert_eq!(
            tournament_time.get::<_, Option<String>>(2).as_deref(),
            Some("2026-03-16 10:44:11")
        );
        assert_eq!(
            tournament_time.get::<_, Option<String>>(3).as_deref(),
            Some("gg_user_timezone_missing")
        );

        let hand_time = client
            .query_one(
                "SELECT
                    hand_started_at::text,
                    hand_started_at_raw,
                    hand_started_at_local::text,
                    hand_started_at_tz_provenance
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&hh_report.source_file_id, &FT_HAND_ID],
            )
            .unwrap();

        assert_eq!(hand_time.get::<_, Option<String>>(0), None);
        assert_eq!(
            hand_time.get::<_, Option<String>>(1).as_deref(),
            Some("2026/03/16 11:07:34")
        );
        assert_eq!(
            hand_time.get::<_, Option<String>>(2).as_deref(),
            Some("2026-03-16 11:07:34")
        );
        assert_eq!(
            hand_time.get::<_, Option<String>>(3).as_deref(),
            Some("gg_user_timezone_missing")
        );

        let source_file_members = client
            .query(
                "SELECT source_file_id, member_index, member_path, member_kind
                 FROM import.source_file_members
                 WHERE source_file_id IN ($1, $2)
                 ORDER BY member_kind, source_file_id",
                &[&ts_report.source_file_id, &hh_report.source_file_id],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, Uuid>(0),
                    row.get::<_, i32>(1),
                    row.get::<_, String>(2),
                    row.get::<_, String>(3),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            source_file_members,
            vec![
                (
                    hh_report.source_file_id,
                    0,
                    "GG20260316-0344 - Mystery Battle Royale 25.txt".to_string(),
                    "hh".to_string(),
                ),
                (
                    ts_report.source_file_id,
                    0,
                    "GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt".to_string(),
                    "ts".to_string(),
                ),
            ]
        );

        let job_attempts = client
            .query(
                "SELECT attempt_no, status, stage
                 FROM import.job_attempts
                 WHERE import_job_id IN ($1, $2)
                 ORDER BY import_job_id",
                &[&ts_report.import_job_id, &hh_report.import_job_id],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, i32>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            job_attempts,
            vec![
                (1, "done".to_string(), "done".to_string()),
                (1, "done".to_string(), "done".to_string()),
            ]
        );

        let alias_row = client
            .query_one(
                "SELECT alias, is_primary
                 FROM core.player_aliases
                 WHERE player_profile_id = $1
                 ORDER BY created_at
                 LIMIT 1",
                &[&player_profile_id],
            )
            .unwrap();
        assert_eq!(alias_row.get::<_, String>(0), DEV_PLAYER_NAME);
        assert!(alias_row.get::<_, bool>(1));

        let hero_seat = client
            .query_one(
                "SELECT player_name, player_profile_id
                 FROM core.hand_seats
                 WHERE hand_id = (
                     SELECT id
                     FROM core.hands
                     WHERE source_file_id = $1
                       AND external_hand_id = $2
                 )
                   AND is_hero = TRUE",
                &[&hh_report.source_file_id, &FT_HAND_ID],
            )
            .unwrap();
        assert_eq!(hero_seat.get::<_, String>(0), DEV_PLAYER_NAME);
        assert_eq!(hero_seat.get::<_, Option<Uuid>>(1), Some(player_profile_id));
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_uses_explicit_profile_aliases_without_creating_dev_context() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        let (_organization_id, _user_id, player_profile_id) = seed_import_actor(
            &mut setup_client,
            "P2-03 Explicit Org",
            "p203-explicit@example.com",
            "TableHero",
            Some("Hero"),
            None,
        )
        .unwrap();
        drop(setup_client);

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");

        let ts_report =
            super::import_path_with_database_url(&database_url, &ts_path, player_profile_id)
                .unwrap();
        let hh_report =
            super::import_path_with_database_url(&database_url, &hh_path, player_profile_id)
                .unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let hero_seat = client
            .query_one(
                "SELECT player_name, player_profile_id
                 FROM core.hand_seats
                 WHERE hand_id = (
                     SELECT id
                     FROM core.hands
                     WHERE source_file_id = $1
                       AND external_hand_id = $2
                 )
                   AND player_name = 'Hero'",
                &[&hh_report.source_file_id, &FT_HAND_ID],
            )
            .unwrap();
        assert_eq!(hero_seat.get::<_, String>(0), "Hero");
        assert_eq!(hero_seat.get::<_, Option<Uuid>>(1), Some(player_profile_id));

        let dev_artifacts: i64 = client
            .query_one(
                "SELECT
                    (SELECT COUNT(*) FROM org.organizations WHERE name = $1)
                  + (SELECT COUNT(*) FROM auth.users WHERE email = $2)
                  + (SELECT COUNT(*) FROM core.player_profiles WHERE screen_name = $3 AND owner_user_id IN (
                        SELECT id FROM auth.users WHERE email = $2
                    ))",
                &[&DEV_ORG_NAME, &DEV_USER_EMAIL, &DEV_PLAYER_NAME],
            )
            .unwrap()
            .get(0);
        assert_eq!(dev_artifacts, 0);

        let tournament_profile_id: Uuid = client
            .query_one(
                "SELECT player_profile_id
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(tournament_profile_id, player_profile_id);
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_with_user_timezone_populates_canonical_utc() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        let (_organization_id, _user_id, player_profile_id) = seed_import_actor(
            &mut setup_client,
            "P2-03 Timezone Org",
            "p203-timezone@example.com",
            "Hero",
            Some("Hero"),
            Some("Asia/Krasnoyarsk"),
        )
        .unwrap();
        drop(setup_client);

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");

        let ts_report =
            super::import_path_with_database_url(&database_url, &ts_path, player_profile_id)
                .unwrap();
        let hh_report =
            super::import_path_with_database_url(&database_url, &hh_path, player_profile_id)
                .unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let tournament_time = client
            .query_one(
                "SELECT
                    timezone('UTC', started_at)::text,
                    started_at_tz_provenance
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap();
        assert_eq!(
            tournament_time.get::<_, Option<String>>(0).as_deref(),
            Some("2026-03-16 03:44:11")
        );
        assert_eq!(
            tournament_time.get::<_, Option<String>>(1).as_deref(),
            Some("gg_user_timezone")
        );

        let hand_time = client
            .query_one(
                "SELECT
                    timezone('UTC', hand_started_at)::text,
                    hand_started_at_tz_provenance
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&hh_report.source_file_id, &FT_HAND_ID],
            )
            .unwrap();
        assert_eq!(
            hand_time.get::<_, Option<String>>(0).as_deref(),
            Some("2026-03-16 04:07:34")
        );
        assert_eq!(
            hand_time.get::<_, Option<String>>(1).as_deref(),
            Some("gg_user_timezone")
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn set_and_clear_user_timezone_recompute_historical_gg_timestamps() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        let (_organization_id, user_id, player_profile_id) = seed_import_actor(
            &mut setup_client,
            "P2-03 Recompute Org",
            "p203-recompute@example.com",
            "Hero",
            Some("Hero"),
            None,
        )
        .unwrap();
        drop(setup_client);

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");
        let ts_report =
            super::import_path_with_database_url(&database_url, &ts_path, player_profile_id)
                .unwrap();
        let hh_report =
            super::import_path_with_database_url(&database_url, &hh_path, player_profile_id)
                .unwrap();

        let set_report = super::set_user_timezone(user_id, "Asia/Krasnoyarsk").unwrap();
        assert_eq!(set_report.user_id, user_id);
        assert_eq!(
            set_report.timezone_name.as_deref(),
            Some("Asia/Krasnoyarsk")
        );
        assert_eq!(set_report.affected_profiles, 1);
        assert!(set_report.tournaments_recomputed >= 1);
        assert!(set_report.hands_recomputed >= 1);

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let tournament_after_set = client
            .query_one(
                "SELECT timezone('UTC', started_at)::text, started_at_tz_provenance
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap();
        assert_eq!(
            tournament_after_set.get::<_, Option<String>>(0).as_deref(),
            Some("2026-03-16 03:44:11")
        );
        assert_eq!(
            tournament_after_set.get::<_, Option<String>>(1).as_deref(),
            Some("gg_user_timezone")
        );

        let hand_after_set = client
            .query_one(
                "SELECT timezone('UTC', hand_started_at)::text, hand_started_at_tz_provenance
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&hh_report.source_file_id, &FT_HAND_ID],
            )
            .unwrap();
        assert_eq!(
            hand_after_set.get::<_, Option<String>>(0).as_deref(),
            Some("2026-03-16 04:07:34")
        );
        assert_eq!(
            hand_after_set.get::<_, Option<String>>(1).as_deref(),
            Some("gg_user_timezone")
        );

        let clear_report = super::clear_user_timezone(user_id).unwrap();
        assert_eq!(clear_report.user_id, user_id);
        assert_eq!(clear_report.timezone_name, None);

        let tournament_after_clear = client
            .query_one(
                "SELECT started_at::text, started_at_tz_provenance
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap();
        assert_eq!(tournament_after_clear.get::<_, Option<String>>(0), None);
        assert_eq!(
            tournament_after_clear
                .get::<_, Option<String>>(1)
                .as_deref(),
            Some("gg_user_timezone_missing")
        );

        let hand_after_clear = client
            .query_one(
                "SELECT hand_started_at::text, hand_started_at_tz_provenance
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&hh_report.source_file_id, &FT_HAND_ID],
            )
            .unwrap();
        assert_eq!(hand_after_clear.get::<_, Option<String>>(0), None);
        assert_eq!(
            hand_after_clear.get::<_, Option<String>>(1).as_deref(),
            Some("gg_user_timezone_missing")
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_persists_tournament_summary_tail_conflicts_as_parse_issues() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        drop(setup_client);

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260325 - Tournament #271770266 - Tail conflict.txt",
        );
        let report = import_path(&ts_path).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let tournament_entry = client
            .query_one(
                "SELECT finish_place, total_payout_money::text, mystery_money_total::text
                 FROM core.tournament_entries
                 WHERE tournament_id = $1",
                &[&report.tournament_id],
            )
            .unwrap();
        let parse_issues = client
            .query(
                "SELECT severity, code, message
                 FROM core.parse_issues
                 WHERE source_file_id = $1
                   AND hand_id IS NULL
                 ORDER BY code",
                &[&report.source_file_id],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, String>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(tournament_entry.get::<_, Option<i32>>(0), Some(1));
        assert_eq!(
            tournament_entry.get::<_, Option<String>>(1).as_deref(),
            Some("205.00")
        );
        assert_eq!(
            tournament_entry.get::<_, Option<String>>(2).as_deref(),
            Some("105.00")
        );
        assert_eq!(
            parse_issues,
            vec![
                (
                    "warning".to_string(),
                    "ts_tail_finish_place_mismatch".to_string(),
                    "result line finish_place=1 conflicts with tail finish_place=2".to_string(),
                ),
                (
                    "warning".to_string(),
                    "ts_tail_total_received_mismatch".to_string(),
                    "result line payout_cents=20500 conflicts with tail payout_cents=20400"
                        .to_string(),
                ),
            ]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_reimport_of_conflicting_tournament_summary_keeps_parse_issues_idempotent() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        drop(setup_client);

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260325 - Tournament #271770266 - Tail conflict.txt",
        );

        let first_report = import_path(&ts_path).unwrap();
        let second_report = import_path(&ts_path).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let parse_issues = client
            .query(
                "SELECT code
                 FROM core.parse_issues
                 WHERE source_file_id = $1
                   AND hand_id IS NULL
                 ORDER BY code",
                &[&second_report.source_file_id],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert_eq!(first_report.source_file_id, second_report.source_file_id);
        assert_eq!(
            parse_issues,
            vec![
                "ts_tail_finish_place_mismatch".to_string(),
                "ts_tail_total_received_mismatch".to_string(),
            ]
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_reuses_source_files_and_members_on_repeat_import() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");

        let first_ts_report = import_path(&ts_path).unwrap();
        let first_hh_report = import_path(&hh_path).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let player_profile_id = dev_player_profile_id(&mut client);

        let source_file_count_after_first: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM import.source_files
                 WHERE player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let member_count_after_first: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM import.source_file_members members
                 JOIN import.source_files files ON files.id = members.source_file_id
                 WHERE files.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let import_job_count_after_first: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM import.import_jobs
                 WHERE source_file_id IN ($1, $2)",
                &[
                    &first_ts_report.source_file_id,
                    &first_hh_report.source_file_id,
                ],
            )
            .unwrap()
            .get(0);
        let attempt_count_after_first: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM import.job_attempts attempts
                 JOIN import.import_jobs jobs ON jobs.id = attempts.import_job_id
                 WHERE jobs.source_file_id IN ($1, $2)",
                &[
                    &first_ts_report.source_file_id,
                    &first_hh_report.source_file_id,
                ],
            )
            .unwrap()
            .get(0);

        let second_ts_report = import_path(&ts_path).unwrap();
        let second_hh_report = import_path(&hh_path).unwrap();

        assert_eq!(
            first_ts_report.source_file_id,
            second_ts_report.source_file_id
        );
        assert_eq!(
            first_hh_report.source_file_id,
            second_hh_report.source_file_id
        );

        let source_file_count_after_second: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM import.source_files
                 WHERE player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let member_count_after_second: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM import.source_file_members members
                 JOIN import.source_files files ON files.id = members.source_file_id
                 WHERE files.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let import_job_count_after_second: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM import.import_jobs
                 WHERE source_file_id IN ($1, $2)",
                &[
                    &first_ts_report.source_file_id,
                    &first_hh_report.source_file_id,
                ],
            )
            .unwrap()
            .get(0);
        let attempt_count_after_second: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM import.job_attempts attempts
                 JOIN import.import_jobs jobs ON jobs.id = attempts.import_job_id
                 WHERE jobs.source_file_id IN ($1, $2)",
                &[
                    &first_ts_report.source_file_id,
                    &first_hh_report.source_file_id,
                ],
            )
            .unwrap()
            .get(0);

        assert_eq!(
            source_file_count_after_first,
            source_file_count_after_second
        );
        assert_eq!(member_count_after_first, member_count_after_second);
        assert_eq!(
            import_job_count_after_first + 2,
            import_job_count_after_second
        );
        assert_eq!(attempt_count_after_first + 2, attempt_count_after_second);
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn upsert_hand_row_reports_fresh_insert_vs_reimport_conflict() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");
        import_path(&ts_path).unwrap();

        let hh_input = fs::read_to_string(&hh_path).unwrap();
        let split_hands = split_hand_history(&hh_input).unwrap();
        let first_split_hand = split_hands.first().expect("fixture must contain hands");
        let canonical_hand = parse_canonical_hand(&first_split_hand.raw_text).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let player_profile_id = dev_player_profile_id(&mut client);
        let mut tx = client.transaction().unwrap();
        let context = load_import_context(&mut tx, player_profile_id).unwrap();
        let tournament_id: Uuid = tx
            .query_one(
                "SELECT id
                 FROM core.tournaments
                 WHERE player_profile_id = $1
                   AND room_id = $2
                   AND external_tournament_id = $3",
                &[
                    &context.player_profile_id,
                    &context.room_id,
                    &canonical_hand.header.tournament_id.to_string(),
                ],
            )
            .unwrap()
            .get(0);
        let source_file_id =
            insert_source_file(&mut tx, &context, &hh_path, &hh_input, "hh").unwrap();
        let source_file_member_id =
            insert_source_file_member(&mut tx, source_file_id, &hh_path, "hh", &hh_input).unwrap();
        let fragment_id = insert_file_fragment(
            &mut tx,
            source_file_id,
            source_file_member_id,
            0,
            Some(canonical_hand.header.hand_id.as_str()),
            "hand",
            &first_split_hand.raw_text,
        )
        .unwrap();

        let (first_hand_id, first_is_new) = upsert_hand_row(
            &mut tx,
            &context,
            tournament_id,
            source_file_id,
            fragment_id,
            &canonical_hand,
        )
        .unwrap();
        let (second_hand_id, second_is_new) = upsert_hand_row(
            &mut tx,
            &context,
            tournament_id,
            source_file_id,
            fragment_id,
            &canonical_hand,
        )
        .unwrap();

        assert_eq!(first_hand_id, second_hand_id);
        assert!(first_is_new);
        assert!(!second_is_new);

        tx.rollback().unwrap();
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_persists_canonical_hand_layer_to_postgres() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");

        import_path(&ts_path).unwrap();
        let report = import_path(&hh_path).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let hand_id: Uuid = client
            .query_one(
                "SELECT id
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&report.source_file_id, &FT_HAND_ID],
            )
            .unwrap()
            .get(0);

        let seat_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_seats WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let position_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_positions WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let hole_cards_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_hole_cards WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let action_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_actions WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let showdown_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_showdowns WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let parse_issue_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.parse_issues
                 WHERE source_file_id = $1
                   AND hand_id = $2",
                &[&report.source_file_id, &hand_id],
            )
            .unwrap()
            .get(0);

        assert_eq!(seat_count, 2);
        assert_eq!(position_count, 2);
        assert_eq!(hole_cards_count, 2);
        assert_eq!(action_count, 9);
        assert_eq!(showdown_count, 2);
        assert_eq!(parse_issue_count, 0);

        let board = client
            .query_one(
                "SELECT flop1, flop2, flop3, turn, river
                 FROM core.hand_boards
                 WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap();

        assert_eq!(board.get::<_, Option<String>>(0).as_deref(), Some("7d"));
        assert_eq!(board.get::<_, Option<String>>(1).as_deref(), Some("2s"));
        assert_eq!(board.get::<_, Option<String>>(2).as_deref(), Some("8h"));
        assert_eq!(board.get::<_, Option<String>>(3).as_deref(), Some("2c"));
        assert_eq!(board.get::<_, Option<String>>(4).as_deref(), Some("Kh"));

        let raise_action = client
            .query_one(
                "SELECT seat_no, action_type, raw_amount, to_amount, is_all_in
                 FROM core.hand_actions
                 WHERE hand_id = $1
                   AND sequence_no = 4",
                &[&hand_id],
            )
            .unwrap();

        assert_eq!(raise_action.get::<_, Option<i32>>(0), Some(3));
        assert_eq!(raise_action.get::<_, String>(1), "raise_to");
        assert_eq!(raise_action.get::<_, Option<i64>>(2), Some(1_512));
        assert_eq!(raise_action.get::<_, Option<i64>>(3), Some(1_912));
        assert!(raise_action.get::<_, bool>(4));

        let resolution = client
            .query_one(
                "SELECT
                    chip_conservation_ok,
                    pot_conservation_ok,
                    settlement_state,
                    rake_amount,
                    final_stacks->>'Hero',
                    final_stacks->>'f02e54a6',
                    invariant_issues::text,
                    settlement->>'certainty_state',
                    (settlement->'issues')::text
                 FROM derived.hand_state_resolutions
                 WHERE hand_id = $1
                   AND resolution_version = $2",
                &[&hand_id, &HAND_RESOLUTION_VERSION],
            )
            .unwrap();

        assert!(resolution.get::<_, bool>(0));
        assert!(resolution.get::<_, bool>(1));
        assert_eq!(resolution.get::<_, String>(2), "exact");
        assert_eq!(resolution.get::<_, i64>(3), 0);
        assert_eq!(
            resolution.get::<_, Option<String>>(4).as_deref(),
            Some("18000")
        );
        assert_eq!(resolution.get::<_, Option<String>>(5).as_deref(), Some("0"));
        assert_eq!(resolution.get::<_, String>(6), "[]");
        assert_eq!(resolution.get::<_, String>(7), "exact");
        assert_eq!(resolution.get::<_, String>(8), "[]");

        let mbr_stage = client
            .query_one(
                "SELECT
                    played_ft_hand,
                    played_ft_hand_state,
                    is_ft_hand,
                    ft_players_remaining_exact,
                    is_stage_2,
                    is_stage_3_4,
                    is_stage_4_5,
                    is_stage_5_6,
                    is_stage_6_9,
                    is_boundary_hand,
                    entered_boundary_zone,
                    entered_boundary_zone_state,
                    boundary_resolution_state,
                    boundary_candidate_count,
                    ft_table_size,
                    boundary_ko_ev::text,
                    boundary_ko_state
                 FROM derived.mbr_stage_resolution
                 WHERE hand_id = $1
                   AND player_profile_id = (
                       SELECT player_profile_id FROM core.hands WHERE id = $1
                   )",
                &[&hand_id],
            )
            .unwrap();

        assert!(mbr_stage.get::<_, bool>(0));
        assert_eq!(mbr_stage.get::<_, String>(1), "exact");
        assert!(mbr_stage.get::<_, bool>(2));
        assert_eq!(mbr_stage.get::<_, Option<i32>>(3), Some(2));
        assert!(mbr_stage.get::<_, bool>(4));
        assert!(!mbr_stage.get::<_, bool>(5));
        assert!(!mbr_stage.get::<_, bool>(6));
        assert!(!mbr_stage.get::<_, bool>(7));
        assert!(!mbr_stage.get::<_, bool>(8));
        assert!(!mbr_stage.get::<_, bool>(9));
        assert!(!mbr_stage.get::<_, bool>(10));
        assert_eq!(mbr_stage.get::<_, String>(11), "exact");
        assert_eq!(mbr_stage.get::<_, String>(12), "exact");
        assert_eq!(mbr_stage.get::<_, i32>(13), 1);
        assert_eq!(mbr_stage.get::<_, Option<i32>>(14), Some(2));
        assert_eq!(mbr_stage.get::<_, Option<String>>(15), None);
        assert_eq!(mbr_stage.get::<_, String>(16), "uncertain");

        let boundary_hand_id: Uuid = client
            .query_one(
                "SELECT id
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&report.source_file_id, &BOUNDARY_RUSH_HAND_ID],
            )
            .unwrap()
            .get(0);

        let boundary_stage = client
            .query_one(
                "SELECT
                    played_ft_hand,
                    is_ft_hand,
                    ft_players_remaining_exact,
                    is_stage_2,
                    is_stage_3_4,
                    is_stage_4_5,
                    is_stage_5_6,
                    is_stage_6_9,
                    is_boundary_hand,
                    entered_boundary_zone,
                    entered_boundary_zone_state,
                    boundary_resolution_state,
                    boundary_candidate_count,
                    ft_table_size,
                    boundary_ko_ev::text,
                    boundary_ko_state
                 FROM derived.mbr_stage_resolution
                 WHERE hand_id = $1
                   AND player_profile_id = (
                       SELECT player_profile_id FROM core.hands WHERE id = $1
                   )",
                &[&boundary_hand_id],
            )
            .unwrap();

        assert!(!boundary_stage.get::<_, bool>(0));
        assert!(!boundary_stage.get::<_, bool>(1));
        assert_eq!(boundary_stage.get::<_, Option<i32>>(2), None);
        assert!(!boundary_stage.get::<_, bool>(3));
        assert!(!boundary_stage.get::<_, bool>(4));
        assert!(!boundary_stage.get::<_, bool>(5));
        assert!(!boundary_stage.get::<_, bool>(6));
        assert!(!boundary_stage.get::<_, bool>(7));
        assert!(boundary_stage.get::<_, bool>(8));
        assert!(boundary_stage.get::<_, bool>(9));
        assert_eq!(boundary_stage.get::<_, String>(10), "exact");
        assert_eq!(boundary_stage.get::<_, String>(11), "exact");
        assert_eq!(boundary_stage.get::<_, i32>(12), 1);
        assert_eq!(boundary_stage.get::<_, Option<i32>>(13), None);
        assert_eq!(boundary_stage.get::<_, Option<String>>(14).as_deref(), None);
        assert_eq!(boundary_stage.get::<_, String>(15), "uncertain");

        let player_profile_id = dev_player_profile_id(&mut client);
        let stage_2_feature = client
            .query_one(
                "SELECT value
                 FROM analytics.player_hand_bool_features
                 WHERE player_profile_id = $1
                   AND hand_id = $2
                   AND feature_key = 'is_stage_2'",
                &[&player_profile_id, &hand_id],
            )
            .unwrap();
        assert!(stage_2_feature.get::<_, bool>(0));

        let boundary_feature = client
            .query_one(
                "SELECT value
                 FROM analytics.player_hand_bool_features
                 WHERE player_profile_id = $1
                   AND hand_id = $2
                   AND feature_key = 'is_boundary_hand'",
                &[&player_profile_id, &boundary_hand_id],
            )
            .unwrap();
        assert!(boundary_feature.get::<_, bool>(0));

        let ft_players_remaining_exact = client
            .query_one(
                "SELECT value::text
                 FROM analytics.player_hand_num_features
                 WHERE player_profile_id = $1
                   AND hand_id = $2
                   AND feature_key = 'ft_players_remaining_exact'",
                &[&player_profile_id, &hand_id],
            )
            .unwrap();
        assert_eq!(
            ft_players_remaining_exact
                .get::<_, Option<String>>(0)
                .as_deref(),
            Some("2.000000")
        );
        let ft_helper_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.mbr_tournament_ft_helper
                 WHERE tournament_id = $1
                   AND player_profile_id = $2",
                &[&report.tournament_id, &player_profile_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(ft_helper_count, 1);

        let ft_helper = client
            .query_one(
                "SELECT
                    reached_ft_exact,
                    first_ft_hand_id,
                    first_ft_hand_started_local::text,
                    first_ft_table_size,
                    ft_started_incomplete,
                    deepest_ft_size_reached,
                    hero_ft_entry_stack_chips,
                    hero_ft_entry_stack_bb::text,
                    entered_boundary_zone,
                    boundary_resolution_state
                 FROM derived.mbr_tournament_ft_helper
                 WHERE tournament_id = $1
                   AND player_profile_id = $2",
                &[&report.tournament_id, &player_profile_id],
            )
            .unwrap();

        assert!(ft_helper.get::<_, bool>(0));
        let first_ft_hand_id: Uuid = ft_helper.get(1);
        let first_ft_external_hand_id = client
            .query_one(
                "SELECT external_hand_id
                 FROM core.hands
                 WHERE id = $1",
                &[&first_ft_hand_id],
            )
            .unwrap()
            .get::<_, String>(0);
        assert_eq!(first_ft_external_hand_id, FIRST_FT_HAND_ID);
        assert_eq!(
            ft_helper.get::<_, Option<String>>(2).as_deref(),
            Some("2026-03-16 10:54:02")
        );
        assert_eq!(ft_helper.get::<_, Option<i32>>(3), Some(9));
        assert_eq!(ft_helper.get::<_, Option<bool>>(4), Some(false));
        assert_eq!(ft_helper.get::<_, Option<i32>>(5), Some(2));
        assert_eq!(ft_helper.get::<_, Option<i64>>(6), Some(1_866));
        assert_eq!(
            ft_helper.get::<_, Option<String>>(7).as_deref(),
            Some("18.660000")
        );
        assert!(ft_helper.get::<_, bool>(8));
        assert_eq!(ft_helper.get::<_, String>(9), "exact");

        let elimination = client
            .query_one(
                "SELECT
                    eliminated_seat_no,
                    eliminated_player_name,
                    pots_participated_by_busted::text,
                    pots_causing_bust::text,
                    last_busting_pot_no,
                    ko_winner_set::text,
                    ko_share_fraction_by_winner #>> '{0,seat_no}',
                    ko_share_fraction_by_winner #>> '{0,player_name}',
                    ko_share_fraction_by_winner #>> '{0,share_fraction}',
                    elimination_certainty_state,
                    ko_certainty_state
                 FROM derived.hand_eliminations
                 WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap();

        assert_eq!(elimination.get::<_, i32>(0), 3);
        assert_eq!(elimination.get::<_, String>(1), "f02e54a6");
        assert_eq!(elimination.get::<_, String>(2), "{1}");
        assert_eq!(elimination.get::<_, String>(3), "{1}");
        assert_eq!(elimination.get::<_, Option<i32>>(4), Some(1));
        assert_eq!(elimination.get::<_, String>(5), "{Hero}");
        assert_eq!(
            elimination.get::<_, Option<String>>(6).as_deref(),
            Some("7")
        );
        assert_eq!(
            elimination.get::<_, Option<String>>(7).as_deref(),
            Some("Hero")
        );
        assert_eq!(
            elimination.get::<_, Option<String>>(8).as_deref(),
            Some("1.000000")
        );
        assert_eq!(elimination.get::<_, String>(9), "exact");
        assert_eq!(elimination.get::<_, String>(10), "exact");

        let street_strength_columns = client
            .query(
                "SELECT column_name
                 FROM information_schema.columns
                 WHERE table_schema = 'derived'
                   AND table_name = 'street_hand_strength'
                 ORDER BY ordinal_position",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| row.get::<_, String>(0))
            .collect::<Vec<_>>();

        assert!(street_strength_columns.contains(&"made_hand_category".to_string()));
        assert!(street_strength_columns.contains(&"draw_category".to_string()));
        assert!(street_strength_columns.contains(&"overcards_count".to_string()));
        assert!(street_strength_columns.contains(&"has_air".to_string()));
        assert!(street_strength_columns.contains(&"missed_flush_draw".to_string()));
        assert!(street_strength_columns.contains(&"missed_straight_draw".to_string()));
        assert!(!street_strength_columns.contains(&"pair_strength".to_string()));
        assert!(!street_strength_columns.contains(&"has_missed_draw_by_river".to_string()));
        assert!(!street_strength_columns.contains(&"descriptor_version".to_string()));

        let street_strength_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.street_hand_strength
                 WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let hero_street_strength_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.street_hand_strength
                 WHERE hand_id = $1
                   AND seat_no = 7",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let villain_street_strength_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.street_hand_strength
                 WHERE hand_id = $1
                   AND seat_no = 3",
                &[&hand_id],
            )
            .unwrap()
            .get(0);

        assert_eq!(street_strength_count, 6);
        assert_eq!(hero_street_strength_count, 3);
        assert_eq!(villain_street_strength_count, 3);

        let hero_flop_street_strength = client
            .query_one(
                "SELECT
                    best_hand_class,
                    best_hand_rank_value,
                    made_hand_category,
                    draw_category,
                    overcards_count,
                    has_air,
                    missed_flush_draw,
                    missed_straight_draw,
                    is_nut_hand,
                    is_nut_draw,
                    certainty_state
                 FROM derived.street_hand_strength
                 WHERE hand_id = $1
                   AND seat_no = 7
                   AND street = 'flop'",
                &[&hand_id],
            )
            .unwrap();

        assert_eq!(hero_flop_street_strength.get::<_, String>(0), "pair");
        assert_eq!(hero_flop_street_strength.get::<_, String>(2), "overpair");
        assert_eq!(hero_flop_street_strength.get::<_, String>(3), "none");
        assert_eq!(hero_flop_street_strength.get::<_, i32>(4), 0);
        assert!(!hero_flop_street_strength.get::<_, bool>(5));
        assert!(!hero_flop_street_strength.get::<_, bool>(6));
        assert!(!hero_flop_street_strength.get::<_, bool>(7));
        assert_eq!(
            hero_flop_street_strength.get::<_, Option<bool>>(8),
            Some(false)
        );
        assert_eq!(
            hero_flop_street_strength.get::<_, Option<bool>>(9),
            Some(false)
        );
        assert_eq!(hero_flop_street_strength.get::<_, String>(10), "exact");

        let pot_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_pots WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let contribution_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_pot_contributions WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let eligibility_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_pot_eligibility WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let winner_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_pot_winners WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let return_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_returns WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);

        assert_eq!(pot_count, 1);
        assert_eq!(eligibility_count, 2);
        assert_eq!(contribution_count, 2);
        assert_eq!(winner_count, 1);
        assert_eq!(return_count, 0);

        let multi_collect_hand_id: Uuid = client
            .query_one(
                "SELECT id
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&report.source_file_id, &MULTI_COLLECT_HAND_ID],
            )
            .unwrap()
            .get(0);

        let multi_collect_resolution = client
            .query_one(
                "SELECT
                    pot_conservation_ok,
                    final_stacks->>'aaab99dd',
                    final_stacks->>'4bdabfc',
                    final_stacks->>'b35710b1'
                 FROM derived.hand_state_resolutions
                 WHERE hand_id = $1
                   AND resolution_version = $2",
                &[&multi_collect_hand_id, &HAND_RESOLUTION_VERSION],
            )
            .unwrap();

        assert!(multi_collect_resolution.get::<_, bool>(0));
        assert_eq!(
            multi_collect_resolution
                .get::<_, Option<String>>(1)
                .as_deref(),
            Some("7572")
        );
        assert_eq!(
            multi_collect_resolution
                .get::<_, Option<String>>(2)
                .as_deref(),
            Some("0")
        );
        assert_eq!(
            multi_collect_resolution
                .get::<_, Option<String>>(3)
                .as_deref(),
            Some("0")
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn runtime_reads_canonical_attempt_tables_even_after_hand_actions_are_deleted() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");

        let ts_report = import_path(&ts_path).unwrap();
        let _hh_report = import_path(&hh_path).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let organization_id = dev_org_id(&mut client);
        let player_profile_id = dev_player_profile_id(&mut client);
        let hand_with_attempt = client
            .query_one(
                "SELECT hand_id, COUNT(*)::bigint
                 FROM derived.hand_ko_attempts
                 WHERE player_profile_id = $1
                 GROUP BY hand_id
                 ORDER BY COUNT(*) DESC, hand_id
                 LIMIT 1",
                &[&player_profile_id],
            )
            .unwrap();
        let first_ft_hand_id: Uuid = hand_with_attempt.get(0);
        let expected_hand_attempt_count: i64 = hand_with_attempt.get(1);
        assert!(expected_hand_attempt_count > 0);

        let expected_exact_ft_attempt_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.hand_ko_attempts attempts
                 INNER JOIN derived.mbr_stage_resolution msr
                   ON msr.hand_id = attempts.hand_id
                  AND msr.player_profile_id = attempts.player_profile_id
                 WHERE attempts.player_profile_id = $1
                   AND msr.ft_players_remaining_exact IS NOT NULL",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);

        let expected_transition_attempt_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.hand_ko_attempts attempts
                 INNER JOIN derived.mbr_stage_resolution msr
                   ON msr.hand_id = attempts.hand_id
                  AND msr.player_profile_id = attempts.player_profile_id
                 INNER JOIN core.hands h
                   ON h.id = attempts.hand_id
                 INNER JOIN derived.mbr_tournament_ft_helper helper
                   ON helper.tournament_id = h.tournament_id
                  AND helper.player_profile_id = attempts.player_profile_id
                 WHERE attempts.player_profile_id = $1
                   AND helper.boundary_resolution_state = 'exact'
                   AND COALESCE(msr.is_boundary_hand, FALSE) IS TRUE",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);

        let first_ft_players: Option<i32> = client
            .query_one(
                "SELECT first_ft_table_size
                 FROM derived.mbr_tournament_ft_helper
                 WHERE tournament_id = $1
                   AND player_profile_id = $2",
                &[&ts_report.tournament_id, &player_profile_id],
            )
            .unwrap()
            .get(0);

        client
            .execute(
                "DELETE FROM core.hand_actions
                 WHERE hand_id IN (
                     SELECT id FROM core.hands WHERE player_profile_id = $1
                 )",
                &[&player_profile_id],
            )
            .unwrap();

        materialize_player_hand_features(&mut client, organization_id, player_profile_id).unwrap();
        let canonical_stats = query_canonical_stats(
            &mut client,
            SeedStatsFilters {
                organization_id,
                player_profile_id,
                buyin_total_cents: Some(vec![2_500]),
            },
        )
        .unwrap();

        let hand_attempt_feature = client
            .query_one(
                "SELECT value::text
                 FROM analytics.player_hand_num_features
                 WHERE player_profile_id = $1
                   AND hand_id = $2
                   AND feature_key = 'hero_ko_attempt_count'",
                &[&player_profile_id, &first_ft_hand_id],
            )
            .unwrap();

        assert_eq!(
            hand_attempt_feature.get::<_, Option<String>>(0).as_deref(),
            Some(format!("{expected_hand_attempt_count}.000000").as_str())
        );
        assert_eq!(
            canonical_stats.values["avg_ko_attempts_per_ft"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_exact_ft_attempt_count as f64,
            ))
        );
        assert_eq!(
            canonical_stats.values["pre_ft_attempts"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_transition_attempt_count as f64
                    * transition_stage_weight(first_ft_players),
            ))
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn ft_dashboard_reads_canonical_attempt_tables_even_after_hand_actions_are_deleted() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");

        import_path(&ts_path).unwrap();
        import_path(&hh_path).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let organization_id = dev_org_id(&mut client);
        let player_profile_id = dev_player_profile_id(&mut client);

        client
            .execute(
                "DELETE FROM core.hand_actions
                 WHERE hand_id IN (
                     SELECT id FROM core.hands WHERE player_profile_id = $1
                 )",
                &[&player_profile_id],
            )
            .unwrap();

        materialize_player_hand_features(&mut client, organization_id, player_profile_id).unwrap();
        let snapshot = query_ft_dashboard(
            &mut client,
            FtDashboardFilters {
                organization_id,
                player_profile_id,
                buyin_total_cents: Some(vec![2_500]),
                bundle_id: None,
                date_from_local: None,
                date_to_local: None,
                timezone_name: "Asia/Krasnoyarsk".to_string(),
            },
        )
        .unwrap();

        assert_eq!(snapshot.data_state, FtDashboardDataState::Ready);
        assert_eq!(snapshot.charts["ko_attempts"].state, FtValueState::Ready);
        assert!(
            snapshot.charts["ko_attempts"]
                .variants
                .values()
                .flat_map(|variant| variant.bars.iter())
                .any(|bar| bar.sample_size > 0)
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_persists_summary_seat_results_to_postgres() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271769484 - Mystery Battle Royale 25.txt",
        );
        import_path(&ts_path).unwrap();

        let source_hand = split_hand_history(
            &fs::read_to_string(fixture_path(
                "../../fixtures/mbr/hh/GG20260316-0338 - Mystery Battle Royale 25.txt",
            ))
            .unwrap(),
        )
        .unwrap()
        .into_iter()
        .find(|hand| hand.header.hand_id == "BR1064995351")
        .unwrap()
        .raw_text;

        let temp_dir = std::env::temp_dir();
        let unique_suffix = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let hh_path = temp_dir.join(format!("cm-summary-outcome-{unique_suffix}.txt"));
        fs::write(
            &hh_path,
            format!("{source_hand}\nSeat 9: VillainX (button) ???"),
        )
        .unwrap();

        let report = import_path(hh_path.to_str().unwrap()).unwrap();
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let hand_id: Uuid = client
            .query_one(
                "SELECT id
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&report.source_file_id, &"BR1064995351"],
            )
            .unwrap()
            .get(0);

        let summary_row_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.hand_summary_results
                 WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let malformed_summary_issue_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.parse_issues
                 WHERE hand_id = $1
                   AND code = 'unparsed_summary_seat_tail'",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let hero_summary_row = client
            .query_one(
                "SELECT
                    seat_no,
                    player_name,
                    position_marker,
                    outcome_kind,
                    folded_street,
                    won_amount,
                    hand_class
                 FROM core.hand_summary_results
                 WHERE hand_id = $1
                   AND seat_no = 4",
                &[&hand_id],
            )
            .unwrap();

        assert_eq!(summary_row_count, 6);
        assert_eq!(malformed_summary_issue_count, 1);
        assert_eq!(hero_summary_row.get::<_, i32>(0), 4);
        assert_eq!(hero_summary_row.get::<_, String>(1).as_str(), "Hero");
        assert_eq!(
            hero_summary_row.get::<_, Option<String>>(2).as_deref(),
            Some("button")
        );
        assert_eq!(hero_summary_row.get::<_, String>(3), "showed_lost");
        assert_eq!(
            hero_summary_row.get::<_, Option<String>>(4).as_deref(),
            None
        );
        assert_eq!(hero_summary_row.get::<_, Option<i64>>(5), None);
        assert_eq!(
            hero_summary_row.get::<_, Option<String>>(6).as_deref(),
            Some("a pair of Kings")
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_persists_position_facts_to_postgres() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        import_path(&ts_path).unwrap();

        let source_hand = first_ft_hand_text();

        let temp_dir = std::env::temp_dir();
        let unique_suffix = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let hh_path = temp_dir.join(format!("cm-position-facts-{unique_suffix}.txt"));
        fs::write(&hh_path, source_hand).unwrap();

        let report = import_path(hh_path.to_str().unwrap()).unwrap();
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let hand_id: Uuid = client
            .query_one(
                "SELECT id
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&report.source_file_id, &"BR1064987693"],
            )
            .unwrap()
            .get(0);

        let position_count: i64 = client
            .query_one(
                "SELECT COUNT(*) FROM core.hand_positions WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let position_rows = client
            .query(
                "SELECT
                     seat_no,
                     position_index,
                     position_label,
                     preflop_act_order_index,
                     postflop_act_order_index
                 FROM core.hand_positions
                 WHERE hand_id = $1
                 ORDER BY seat_no",
                &[&hand_id],
            )
            .unwrap();

        assert_eq!(position_count, 2);
        assert_eq!(position_rows.len(), 2);
        assert_eq!(position_rows[0].get::<_, i32>(0), 3);
        assert_eq!(position_rows[0].get::<_, i32>(1), 1);
        assert_eq!(position_rows[0].get::<_, String>(2), "BTN");
        assert_eq!(position_rows[0].get::<_, i32>(3), 1);
        assert_eq!(position_rows[0].get::<_, i32>(4), 2);
        assert_eq!(position_rows[1].get::<_, i32>(0), 7);
        assert_eq!(position_rows[1].get::<_, i32>(1), 2);
        assert_eq!(position_rows[1].get::<_, String>(2), "BB");
        assert_eq!(position_rows[1].get::<_, i32>(3), 2);
        assert_eq!(position_rows[1].get::<_, i32>(4), 1);
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_materializes_street_runtime_features_for_hero_and_known_showdown() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        import_path(&ts_path).unwrap();

        let temp_dir = std::env::temp_dir();
        let unique_suffix = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        let known_showdown_hh_path =
            temp_dir.join(format!("cm-street-runtime-known-{unique_suffix}.txt"));
        fs::write(&known_showdown_hh_path, first_ft_hand_text()).unwrap();

        let known_report = import_path(known_showdown_hh_path.to_str().unwrap()).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let known_hand_id: Uuid = client
            .query_one(
                "SELECT id
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&known_report.source_file_id, &"BR1064987693"],
            )
            .unwrap()
            .get(0);

        let known_best_hand_rows = client
            .query(
                "SELECT seat_no, street, value
                 FROM analytics.player_street_enum_features
                 WHERE hand_id = $1
                   AND feature_key = 'best_hand_class'
                 ORDER BY seat_no, street",
                &[&known_hand_id],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, i32>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(known_best_hand_rows.len(), 6);
        assert_eq!(
            known_best_hand_rows
                .iter()
                .map(|(seat_no, _, _)| *seat_no)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([3_i32, 7_i32])
        );

        let hero_flop_exact_values = client
            .query_one(
                "SELECT
                    enum_made.value,
                    num_overcards.value::text,
                    bool_air.value
                 FROM analytics.player_street_enum_features enum_made
                 INNER JOIN analytics.player_street_num_features num_overcards
                   ON num_overcards.organization_id = enum_made.organization_id
                  AND num_overcards.player_profile_id = enum_made.player_profile_id
                  AND num_overcards.hand_id = enum_made.hand_id
                  AND num_overcards.seat_no = enum_made.seat_no
                  AND num_overcards.street = enum_made.street
                  AND num_overcards.feature_version = enum_made.feature_version
                  AND num_overcards.feature_key = 'overcards_count'
                 INNER JOIN analytics.player_street_bool_features bool_air
                   ON bool_air.organization_id = enum_made.organization_id
                  AND bool_air.player_profile_id = enum_made.player_profile_id
                  AND bool_air.hand_id = enum_made.hand_id
                  AND bool_air.seat_no = enum_made.seat_no
                  AND bool_air.street = enum_made.street
                  AND bool_air.feature_version = enum_made.feature_version
                  AND bool_air.feature_key = 'has_air'
                 WHERE enum_made.hand_id = $1
                   AND enum_made.seat_no = 7
                   AND enum_made.street = 'flop'
                   AND enum_made.feature_key = 'made_hand_category'",
                &[&known_hand_id],
            )
            .unwrap();
        assert_eq!(hero_flop_exact_values.get::<_, String>(0), "overpair");
        assert_eq!(
            hero_flop_exact_values
                .get::<_, Option<String>>(1)
                .as_deref(),
            Some("0.000000")
        );
        assert!(!hero_flop_exact_values.get::<_, bool>(2));
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_materializes_preflop_matrix_rows_and_runtime_filters() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let ts_report = import_path(&ts_path).unwrap();
        let organization_id: Uuid = setup_client
            .query_one(
                "SELECT organization_id
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap()
            .get(0);
        let player_profile_id: Uuid = setup_client
            .query_one(
                "SELECT player_profile_id
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap()
            .get(0);
        drop(setup_client);

        let cm06_path = std::env::temp_dir().join(format!(
            "cm-preflop-matrix-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &cm06_path,
            cm06_joint_ko_hand_text().replace("Tournament #999060", "Tournament #271770266"),
        )
        .unwrap();
        let cm06_report = import_path(cm06_path.to_str().unwrap()).unwrap();

        let cm05_path = std::env::temp_dir().join(format!(
            "cm-preflop-unknown-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &cm05_path,
            cm05_hidden_showdown_hand_text()
                .replace("Tournament #999051", "Tournament #271770266")
                .replace(
                    "Seat 1: ShortyA mucked",
                    "Seat 1: ShortyA folded before Flop",
                )
                .replace(
                    "Seat 2: ShortyB mucked",
                    "Seat 2: ShortyB folded before Flop",
                ),
        )
        .unwrap();
        let cm05_report = import_path(cm05_path.to_str().unwrap()).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let known_hand_id: Uuid = client
            .query_one(
                "SELECT id
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&cm06_report.source_file_id, &"BRCM0601"],
            )
            .unwrap()
            .get(0);
        let unknown_hand_id: Uuid = client
            .query_one(
                "SELECT id
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&cm05_report.source_file_id, &"BRCM0502"],
            )
            .unwrap()
            .get(0);

        let derived_rows = client
            .query(
                "SELECT seat_no, starter_hand_class, certainty_state
                 FROM derived.preflop_starting_hands
                 WHERE hand_id = $1
                 ORDER BY seat_no",
                &[&known_hand_id],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, i32>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            derived_rows,
            vec![
                (1, "AA".to_string(), "exact".to_string()),
                (2, "22".to_string(), "exact".to_string()),
                (3, "KQs".to_string(), "exact".to_string()),
            ]
        );

        let unknown_rows_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.preflop_starting_hands
                 WHERE hand_id = $1",
                &[&unknown_hand_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(unknown_rows_count, 1);

        let analytics_rows = client
            .query(
                "SELECT seat_no, street, value
                 FROM analytics.player_street_enum_features
                 WHERE hand_id = $1
                   AND feature_key = 'starter_hand_class'
                 ORDER BY seat_no, street",
                &[&known_hand_id],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, i32>(0),
                    row.get::<_, String>(1),
                    row.get::<_, String>(2),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            analytics_rows,
            vec![
                (1, "preflop".to_string(), "AA".to_string()),
                (2, "preflop".to_string(), "22".to_string()),
                (3, "preflop".to_string(), "KQs".to_string()),
            ]
        );

        let hero_matches = query_matching_hand_ids(
            &mut client,
            &hand_query_request(
                organization_id,
                player_profile_id,
                vec![FilterCondition {
                    feature: FeatureRef::Street {
                        street: "preflop".to_string(),
                        feature_key: "starter_hand_class".to_string(),
                    },
                    operator: FilterOperator::In,
                    value: FilterValue::EnumList(vec!["AA".to_string()]),
                }],
                vec![],
            ),
        )
        .unwrap()
        .hand_ids;
        assert_eq!(hero_matches, vec![known_hand_id, unknown_hand_id]);
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_persists_cm06_joint_ko_fields_to_postgres() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        import_path(&ts_path).unwrap();

        let source_hand =
            cm06_joint_ko_hand_text().replace("Tournament #999060", "Tournament #271770266");

        let temp_dir = std::env::temp_dir();
        let unique_suffix = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let hh_path = temp_dir.join(format!("cm06-joint-ko-{unique_suffix}.txt"));
        fs::write(&hh_path, source_hand).unwrap();

        let report = import_path(hh_path.to_str().unwrap()).unwrap();
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let hand_id: Uuid = client
            .query_one(
                "SELECT id
                 FROM core.hands
                 WHERE source_file_id = $1
                   AND external_hand_id = $2",
                &[&report.source_file_id, &"BRCM0601"],
            )
            .unwrap()
            .get(0);

        let elimination = client
            .query_one(
                "SELECT
                    eliminated_seat_no,
                    eliminated_player_name,
                    pots_participated_by_busted::text,
                    pots_causing_bust::text,
                    last_busting_pot_no,
                    ko_winner_set::text,
                    ko_share_fraction_by_winner #>> '{0,seat_no}',
                    ko_share_fraction_by_winner #>> '{0,player_name}',
                    ko_share_fraction_by_winner #>> '{0,share_fraction}',
                    elimination_certainty_state,
                    ko_certainty_state
                 FROM derived.hand_eliminations
                 WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap();

        assert_eq!(elimination.get::<_, i32>(0), 3);
        assert_eq!(elimination.get::<_, String>(1), "Medium");
        assert_eq!(elimination.get::<_, String>(2), "{1,2}");
        assert_eq!(elimination.get::<_, String>(3), "{2}");
        assert_eq!(elimination.get::<_, Option<i32>>(4), Some(2));
        assert_eq!(elimination.get::<_, String>(5), "{Hero}");
        assert_eq!(
            elimination.get::<_, Option<String>>(6).as_deref(),
            Some("1")
        );
        assert_eq!(
            elimination.get::<_, Option<String>>(7).as_deref(),
            Some("Hero")
        );
        assert_eq!(
            elimination.get::<_, Option<String>>(8).as_deref(),
            Some("1.000000")
        );
        assert_eq!(elimination.get::<_, String>(9), "exact");
        assert_eq!(elimination.get::<_, String>(10), "exact");
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_refreshes_analytics_features_and_seed_stats() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");

        let ts_report = import_path(&ts_path).unwrap();
        let hh_report = import_path(&hh_path).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let player_profile_id: Uuid = client
            .query_one(
                "SELECT player_profile_id
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap()
            .get(0);
        let organization_id: Uuid = client
            .query_one(
                "SELECT organization_id
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap()
            .get(0);
        let economics = client
            .query_one(
                "SELECT
                    regular_prize_money::text,
                    total_payout_money::text,
                    mystery_money_total::text
                 FROM core.tournament_entries
                 WHERE tournament_id = $1
                   AND player_profile_id = $2",
                &[&ts_report.tournament_id, &player_profile_id],
            )
            .unwrap();
        let buyin_config_count: i64 = client
            .query_one("SELECT COUNT(*) FROM ref.mbr_buyin_configs", &[])
            .unwrap()
            .get(0);
        let regular_prize_count: i64 = client
            .query_one("SELECT COUNT(*) FROM ref.mbr_regular_prizes", &[])
            .unwrap()
            .get(0);
        let mystery_envelope_count: i64 = client
            .query_one("SELECT COUNT(*) FROM ref.mbr_mystery_envelopes", &[])
            .unwrap()
            .get(0);
        let regular_prize_rows = client
            .query(
                "SELECT
                    cfg.buyin_total::text,
                    prize.finish_place,
                    prize.regular_prize_money::text
                 FROM ref.mbr_regular_prizes AS prize
                 INNER JOIN ref.mbr_buyin_configs AS cfg
                    ON cfg.id = prize.buyin_config_id
                 ORDER BY cfg.buyin_total, prize.finish_place",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, String>(0),
                    row.get::<_, i32>(1),
                    row.get::<_, String>(2),
                )
            })
            .collect::<Vec<_>>();
        let mystery_envelope_edges = client
            .query(
                "SELECT
                    cfg.buyin_total::text,
                    envelope.sort_order,
                    envelope.payout_money::text,
                    envelope.frequency_per_100m
                 FROM ref.mbr_mystery_envelopes AS envelope
                 INNER JOIN ref.mbr_buyin_configs AS cfg
                    ON cfg.id = envelope.buyin_config_id
                 WHERE envelope.sort_order IN (1, 10)
                 ORDER BY cfg.buyin_total, envelope.sort_order",
                &[],
            )
            .unwrap()
            .into_iter()
            .map(|row| {
                (
                    row.get::<_, String>(0),
                    row.get::<_, i32>(1),
                    row.get::<_, String>(2),
                    row.get::<_, i64>(3),
                )
            })
            .collect::<Vec<_>>();

        let bool_feature_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM analytics.player_hand_bool_features
                 WHERE player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let num_feature_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM analytics.player_hand_num_features
                 WHERE player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let enum_feature_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM analytics.player_hand_enum_features
                 WHERE player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);

        assert_eq!(
            economics.get::<_, Option<String>>(0).as_deref(),
            Some("100.00")
        );
        assert_eq!(
            economics.get::<_, Option<String>>(1).as_deref(),
            Some("205.00")
        );
        assert_eq!(
            economics.get::<_, Option<String>>(2).as_deref(),
            Some("105.00")
        );
        assert_eq!(buyin_config_count, 5);
        assert_eq!(regular_prize_count, 15);
        assert_eq!(mystery_envelope_count, 50);
        assert_eq!(
            regular_prize_rows,
            vec![
                ("0.25".to_string(), 1, "1.00".to_string()),
                ("0.25".to_string(), 2, "0.75".to_string()),
                ("0.25".to_string(), 3, "0.50".to_string()),
                ("1.00".to_string(), 1, "4.00".to_string()),
                ("1.00".to_string(), 2, "3.00".to_string()),
                ("1.00".to_string(), 3, "2.00".to_string()),
                ("3.00".to_string(), 1, "12.00".to_string()),
                ("3.00".to_string(), 2, "9.00".to_string()),
                ("3.00".to_string(), 3, "6.00".to_string()),
                ("10.00".to_string(), 1, "40.00".to_string()),
                ("10.00".to_string(), 2, "30.00".to_string()),
                ("10.00".to_string(), 3, "20.00".to_string()),
                ("25.00".to_string(), 1, "100.00".to_string()),
                ("25.00".to_string(), 2, "75.00".to_string()),
                ("25.00".to_string(), 3, "50.00".to_string()),
            ]
        );
        assert_eq!(
            mystery_envelope_edges,
            vec![
                ("0.25".to_string(), 1, "5000.00".to_string(), 30),
                ("0.25".to_string(), 10, "0.06".to_string(), 27048920),
                ("1.00".to_string(), 1, "10000.00".to_string(), 60),
                ("1.00".to_string(), 10, "0.25".to_string(), 28391080),
                ("3.00".to_string(), 1, "30000.00".to_string(), 80),
                ("3.00".to_string(), 10, "0.75".to_string(), 29191040),
                ("10.00".to_string(), 1, "100000.00".to_string(), 100),
                ("10.00".to_string(), 10, "2.50".to_string(), 29991000),
                ("25.00".to_string(), 1, "250000.00".to_string(), 100),
                ("25.00".to_string(), 10, "6.00".to_string(), 28477360),
            ]
        );
        assert!(bool_feature_count > 0);
        assert!(num_feature_count > 0);
        assert!(enum_feature_count > 0);

        let played_ft_hand = client
            .query_one(
                "SELECT value
                 FROM analytics.player_hand_bool_features
                 WHERE player_profile_id = $1
                   AND hand_id = (
                       SELECT id
                       FROM core.hands
                       WHERE source_file_id = $2
                         AND external_hand_id = $3
                   )
                   AND feature_key = 'played_ft_hand'",
                &[
                    &player_profile_id,
                    &hh_report.source_file_id,
                    &FIRST_FT_HAND_ID,
                ],
            )
            .unwrap();
        assert!(played_ft_hand.get::<_, bool>(0));

        let seed_stats = query_seed_stats(
            &mut client,
            SeedStatsFilters {
                organization_id,
                player_profile_id,
                buyin_total_cents: Some(vec![2_500]),
            },
        )
        .unwrap();

        assert_eq!(seed_stats.coverage.summary_tournament_count, 1);
        assert_eq!(seed_stats.coverage.hand_tournament_count, 1);
        assert_eq!(seed_stats.roi_pct, Some(720.0));
        assert_eq!(seed_stats.avg_finish_place, Some(1.0));
        assert_eq!(seed_stats.final_table_reach_percent, Some(100.0));
        assert!(seed_stats.total_ko_event_count >= 1);
        assert_eq!(seed_stats.early_ft_ko_event_count, 1);
        assert_eq!(seed_stats.early_ft_ko_event_per_tournament, Some(1.0));

        let canonical_stats = query_canonical_stats(
            &mut client,
            SeedStatsFilters {
                organization_id,
                player_profile_id,
                buyin_total_cents: Some(vec![2_500]),
            },
        )
        .unwrap();
        let ft_helper = client
            .query_one(
                "SELECT
                    hero_ft_entry_stack_chips::double precision,
                    hero_ft_entry_stack_bb::double precision,
                    ft_started_incomplete
                 FROM derived.mbr_tournament_ft_helper
                 WHERE tournament_id = $1
                   AND player_profile_id = $2",
                &[&ts_report.tournament_id, &player_profile_id],
            )
            .unwrap();
        let deep_ft_entry = client
            .query_one(
                "SELECT
                    hs.starting_stack::double precision,
                    hs.starting_stack::double precision / h.big_blind::double precision
                 FROM core.hands h
                 INNER JOIN derived.mbr_stage_resolution msr
                   ON msr.hand_id = h.id
                  AND msr.player_profile_id = h.player_profile_id
                 INNER JOIN core.hand_seats hs
                   ON hs.hand_id = h.id
                  AND hs.is_hero IS TRUE
                 WHERE h.tournament_id = $1
                   AND h.player_profile_id = $2
                   AND msr.ft_players_remaining_exact IS NOT NULL
                   AND msr.ft_players_remaining_exact <= 5
                 ORDER BY
                    h.tournament_hand_order NULLS LAST,
                    h.id
                 LIMIT 1",
                &[&ts_report.tournament_id, &player_profile_id],
            )
            .unwrap();
        let stage_event_counts = client
            .query_one(
                "SELECT
                    COUNT(*) FILTER (
                        WHERE he.elimination_certainty_state = 'exact'
                          AND eliminated_seat.is_hero IS TRUE
                          AND msr.is_stage_6_9
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE hero_winner.hand_id IS NOT NULL
                          AND he.ko_certainty_state = 'exact'
                          AND msr.ft_players_remaining_exact IN (2, 3)
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE hero_winner.hand_id IS NOT NULL
                          AND he.ko_certainty_state = 'exact'
                          AND msr.is_stage_3_4
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE hero_winner.hand_id IS NOT NULL
                          AND he.ko_certainty_state = 'exact'
                          AND msr.is_stage_4_5
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE hero_winner.hand_id IS NOT NULL
                          AND he.ko_certainty_state = 'exact'
                          AND msr.is_stage_5_6
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE hero_winner.hand_id IS NOT NULL
                          AND he.ko_certainty_state = 'exact'
                          AND msr.is_stage_6_9
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE hero_winner.hand_id IS NOT NULL
                          AND he.ko_certainty_state = 'exact'
                          AND msr.ft_players_remaining_exact IN (7, 8, 9)
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE hero_winner.hand_id IS NOT NULL
                          AND he.ko_certainty_state = 'exact'
                          AND helper.first_ft_hand_id IS NOT NULL
                          AND h.tournament_hand_order IS NOT NULL
                          AND COALESCE(msr.is_boundary_hand, FALSE) IS FALSE
                          AND h.tournament_hand_order < (
                              SELECT fh.tournament_hand_order
                              FROM core.hands fh
                              WHERE fh.id = helper.first_ft_hand_id
                          )
                    )::bigint
                 FROM core.hands h
                 LEFT JOIN derived.mbr_stage_resolution msr
                   ON msr.hand_id = h.id
                  AND msr.player_profile_id = h.player_profile_id
                 LEFT JOIN derived.hand_eliminations he
                   ON he.hand_id = h.id
                 LEFT JOIN core.hand_seats eliminated_seat
                   ON eliminated_seat.hand_id = he.hand_id
                  AND eliminated_seat.seat_no = he.eliminated_seat_no
                 LEFT JOIN core.hand_seats hero_winner
                   ON hero_winner.hand_id = he.hand_id
                  AND hero_winner.is_hero IS TRUE
                  AND hero_winner.player_name = ANY(he.ko_winner_set)
                 LEFT JOIN derived.mbr_tournament_ft_helper helper
                   ON helper.tournament_id = h.tournament_id
                  AND helper.player_profile_id = h.player_profile_id
                 WHERE h.tournament_id = $1
                   AND h.player_profile_id = $2",
                &[&ts_report.tournament_id, &player_profile_id],
            )
            .unwrap();
        let _stage_attempt_counts = client
            .query_one(
                "WITH attempt_targets AS (
                    SELECT DISTINCT
                        h.id AS hand_id,
                        target.seat_no AS target_seat_no
                     FROM core.hands h
                     INNER JOIN core.hand_seats hero_seat
                       ON hero_seat.hand_id = h.id
                      AND hero_seat.is_hero IS TRUE
                     INNER JOIN core.hand_seats target
                       ON target.hand_id = h.id
                      AND target.is_hero IS FALSE
                      AND target.starting_stack > 0
                      AND hero_seat.starting_stack >= target.starting_stack
                     WHERE h.tournament_id = $1
                       AND h.player_profile_id = $2
                       AND EXISTS (
                           SELECT 1
                           FROM core.hand_actions target_action
                           WHERE target_action.hand_id = h.id
                             AND target_action.seat_no = target.seat_no
                             AND target_action.is_all_in IS TRUE
                       )
                       AND EXISTS (
                           SELECT 1
                           FROM core.hand_pot_eligibility hero_pe
                           INNER JOIN core.hand_pot_eligibility target_pe
                             ON target_pe.hand_id = hero_pe.hand_id
                            AND target_pe.pot_no = hero_pe.pot_no
                           WHERE hero_pe.hand_id = h.id
                             AND hero_pe.seat_no = hero_seat.seat_no
                             AND target_pe.seat_no = target.seat_no
                       )
                 )
                 SELECT
                    COUNT(*) FILTER (
                        WHERE msr.ft_players_remaining_exact IN (2, 3)
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE msr.is_stage_3_4
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE msr.is_stage_4_5
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE msr.is_stage_5_6
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE msr.is_stage_6_9
                    )::bigint,
                    COUNT(*) FILTER (
                        WHERE msr.ft_players_remaining_exact IN (7, 8, 9)
                    )::bigint
                 FROM attempt_targets attempts
                 INNER JOIN derived.mbr_stage_resolution msr
                   ON msr.hand_id = attempts.hand_id
                  AND msr.player_profile_id = $2",
                &[&ts_report.tournament_id, &player_profile_id],
            )
            .unwrap();
        let _stage_entry_values = client
            .query_one(
                "SELECT
                    helper.reached_ft_exact,
                    EXISTS (
                        SELECT 1
                        FROM core.hands h
                        INNER JOIN derived.mbr_stage_resolution msr
                          ON msr.hand_id = h.id
                         AND msr.player_profile_id = h.player_profile_id
                        WHERE h.tournament_id = helper.tournament_id
                          AND h.player_profile_id = helper.player_profile_id
                          AND msr.ft_players_remaining_exact IN (2, 3)
                    ) AS reached_stage_2_3,
                    EXISTS (
                        SELECT 1
                        FROM core.hands h
                        INNER JOIN derived.mbr_stage_resolution msr
                          ON msr.hand_id = h.id
                         AND msr.player_profile_id = h.player_profile_id
                        WHERE h.tournament_id = helper.tournament_id
                          AND h.player_profile_id = helper.player_profile_id
                          AND msr.is_stage_4_5
                    ) AS reached_stage_4_5,
                    EXISTS (
                        SELECT 1
                        FROM core.hands h
                        INNER JOIN derived.mbr_stage_resolution msr
                          ON msr.hand_id = h.id
                         AND msr.player_profile_id = h.player_profile_id
                        WHERE h.tournament_id = helper.tournament_id
                          AND h.player_profile_id = helper.player_profile_id
                          AND msr.ft_players_remaining_exact IN (7, 8, 9)
                    ) AS reached_stage_7_9,
                    helper.hero_ft_entry_stack_bb::double precision,
                    (
                        SELECT hs.starting_stack::double precision / h.big_blind::double precision
                        FROM core.hands h
                        INNER JOIN derived.mbr_stage_resolution msr
                          ON msr.hand_id = h.id
                         AND msr.player_profile_id = h.player_profile_id
                        INNER JOIN core.hand_seats hs
                          ON hs.hand_id = h.id
                         AND hs.is_hero IS TRUE
                        WHERE h.tournament_id = helper.tournament_id
                          AND h.player_profile_id = helper.player_profile_id
                          AND msr.is_stage_5_6
                        ORDER BY
                            h.tournament_hand_order NULLS LAST,
                            h.id
                        LIMIT 1
                    ) AS hero_stage_5_6_stack_bb,
                    (
                        SELECT hs.starting_stack::double precision / h.big_blind::double precision
                        FROM core.hands h
                        INNER JOIN derived.mbr_stage_resolution msr
                          ON msr.hand_id = h.id
                         AND msr.player_profile_id = h.player_profile_id
                        INNER JOIN core.hand_seats hs
                          ON hs.hand_id = h.id
                         AND hs.is_hero IS TRUE
                        WHERE h.tournament_id = helper.tournament_id
                          AND h.player_profile_id = helper.player_profile_id
                          AND msr.is_stage_3_4
                        ORDER BY
                            h.tournament_hand_order NULLS LAST,
                            h.id
                        LIMIT 1
                    ) AS hero_stage_3_4_stack_bb
                 FROM derived.mbr_tournament_ft_helper helper
                 WHERE helper.tournament_id = $1
                   AND helper.player_profile_id = $2",
                &[&ts_report.tournament_id, &player_profile_id],
            )
            .unwrap();
        let tournament_buyin_cents: i64 = client
            .query_one(
                "SELECT (buyin_total * 100)::bigint
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap()
            .get(0);
        let ko_money_events = client
            .query(
                "SELECT
                    (hero_share.hero_share_fraction * 1000000)::bigint,
                    COALESCE(msr.ft_players_remaining_exact IN (2, 3), FALSE),
                    COALESCE(msr.is_stage_3_4, FALSE),
                    COALESCE(msr.is_stage_4_5, FALSE),
                    COALESCE(msr.is_stage_5_6, FALSE),
                    COALESCE(msr.is_stage_6_9, FALSE),
                    COALESCE(msr.ft_players_remaining_exact IN (7, 8, 9), FALSE)
                 FROM core.hands h
                 INNER JOIN derived.hand_eliminations he
                   ON he.hand_id = h.id
                 INNER JOIN core.hand_seats hero_seat
                   ON hero_seat.hand_id = h.id
                  AND hero_seat.is_hero IS TRUE
                 INNER JOIN LATERAL (
                    SELECT (share->>'share_fraction')::numeric AS hero_share_fraction
                    FROM jsonb_array_elements(he.ko_share_fraction_by_winner) share
                    WHERE (share->>'seat_no')::int = hero_seat.seat_no
                    LIMIT 1
                 ) hero_share
                   ON TRUE
                 LEFT JOIN derived.mbr_stage_resolution msr
                   ON msr.hand_id = h.id
                  AND msr.player_profile_id = h.player_profile_id
                 WHERE h.tournament_id = $1
                   AND h.player_profile_id = $2
                   AND he.ko_certainty_state = 'exact'
                   AND hero_share.hero_share_fraction > 0",
                &[&ts_report.tournament_id, &player_profile_id],
            )
            .unwrap();
        let mystery_envelopes = client
            .query(
                "SELECT
                    envelope.sort_order,
                    (envelope.payout_money * 100)::bigint,
                    envelope.frequency_per_100m
                 FROM ref.mbr_mystery_envelopes envelope
                 INNER JOIN ref.mbr_buyin_configs cfg
                   ON cfg.id = envelope.buyin_config_id
                 WHERE (cfg.buyin_total * 100)::bigint = $1
                 ORDER BY envelope.sort_order",
                &[&tournament_buyin_cents],
            )
            .unwrap()
            .into_iter()
            .map(|row| MysteryEnvelope {
                sort_order: row.get(0),
                payout_cents: row.get(1),
                frequency_per_100m: row.get(2),
            })
            .collect::<Vec<_>>();
        let mut expected_ko_money_total = 0.0;
        let mut expected_ko_stage_2_3_money_total = 0.0;
        let mut expected_ko_stage_3_4_money_total = 0.0;
        let mut expected_ko_stage_4_5_money_total = 0.0;
        let mut expected_ko_stage_5_6_money_total = 0.0;
        let mut expected_ko_stage_6_9_money_total = 0.0;
        let mut expected_ko_stage_7_9_money_total = 0.0;
        for row in ko_money_events {
            let expected_cents =
                expected_hero_mystery_cents(row.get::<_, i64>(0), &mystery_envelopes).unwrap();
            let expected_money = expected_cents / 100.0;
            expected_ko_money_total += expected_money;
            if row.get::<_, bool>(1) {
                expected_ko_stage_2_3_money_total += expected_money;
            }
            if row.get::<_, bool>(2) {
                expected_ko_stage_3_4_money_total += expected_money;
            }
            if row.get::<_, bool>(3) {
                expected_ko_stage_4_5_money_total += expected_money;
            }
            if row.get::<_, bool>(4) {
                expected_ko_stage_5_6_money_total += expected_money;
            }
            if row.get::<_, bool>(5) {
                expected_ko_stage_6_9_money_total += expected_money;
            }
            if row.get::<_, bool>(6) {
                expected_ko_stage_7_9_money_total += expected_money;
            }
        }
        let pre_ft_chip_delta: f64 = client
            .query_one(
                "SELECT
                    helper.hero_ft_entry_stack_chips::double precision - 1000::double precision
                 FROM derived.mbr_tournament_ft_helper helper
                 WHERE helper.tournament_id = $1
                   AND helper.player_profile_id = $2
                   AND (
                       helper.first_ft_hand_started_local IS NULL
                       OR helper.boundary_resolution_state = 'exact'
                   )",
                &[&ts_report.tournament_id, &player_profile_id],
            )
            .unwrap()
            .get::<_, f64>(0);
        let regular_prize_money = economics
            .get::<_, Option<String>>(0)
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let _total_payout_money = economics
            .get::<_, Option<String>>(1)
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let mystery_money_total = economics
            .get::<_, Option<String>>(2)
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let buyin_total_money = cents_to_f64(tournament_buyin_cents);

        assert_eq!(canonical_stats.coverage.summary_tournament_count, 1);
        assert_eq!(canonical_stats.coverage.hand_tournament_count, 1);
        assert_eq!(
            canonical_stats.values["avg_finish_place_ft"].state,
            CanonicalStatState::Value
        );
        assert_eq!(
            canonical_stats.values["avg_finish_place_ft"].value,
            Some(CanonicalStatNumericValue::Float(1.0))
        );
        assert_eq!(
            canonical_stats.values["avg_finish_place_no_ft"].state,
            CanonicalStatState::Null
        );
        assert_eq!(
            canonical_stats.values["avg_ft_initial_stack_chips"].value,
            Some(CanonicalStatNumericValue::Float(ft_helper.get::<_, f64>(0)))
        );
        assert_eq!(
            canonical_stats.values["avg_ft_initial_stack_bb"].value,
            Some(CanonicalStatNumericValue::Float(ft_helper.get::<_, f64>(1)))
        );
        assert_eq!(
            canonical_stats.values["incomplete_ft_percent"].value,
            Some(CanonicalStatNumericValue::Float(
                if ft_helper.get::<_, Option<bool>>(2) == Some(true) {
                    100.0
                } else {
                    0.0
                }
            ))
        );
        assert_eq!(
            canonical_stats.values["itm_percent"].value,
            Some(CanonicalStatNumericValue::Float(100.0))
        );
        assert_eq!(
            canonical_stats.values["roi_on_ft_pct"].value,
            Some(CanonicalStatNumericValue::Float(720.0))
        );
        assert_eq!(
            canonical_stats.values["winnings_from_itm"].value,
            Some(CanonicalStatNumericValue::Float(100.0))
        );
        assert_eq!(
            canonical_stats.values["winnings_from_ko"].value,
            Some(CanonicalStatNumericValue::Float(
                economics
                    .get::<_, Option<String>>(2)
                    .unwrap()
                    .parse::<f64>()
                    .unwrap(),
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_contribution"].value,
            Some(CanonicalStatNumericValue::Float(
                economics
                    .get::<_, Option<String>>(2)
                    .unwrap()
                    .parse::<f64>()
                    .unwrap()
                    / economics
                        .get::<_, Option<String>>(1)
                        .unwrap()
                        .parse::<f64>()
                        .unwrap()
                    * 100.0,
            ))
        );
        assert_canonical_float_close(
            &canonical_stats.values["ko_contribution_adj"].value,
            expected_ko_money_total / (regular_prize_money + expected_ko_money_total) * 100.0,
            "ko_contribution_adj",
        );
        assert_canonical_float_close(
            &canonical_stats.values["ko_luck"].value,
            mystery_money_total - expected_ko_money_total,
            "ko_luck",
        );
        assert_canonical_float_close(
            &canonical_stats.values["roi_adj"].value,
            ((regular_prize_money + expected_ko_money_total - buyin_total_money)
                / buyin_total_money)
                * 100.0,
            "roi_adj",
        );
        assert_eq!(
            canonical_stats.values["deep_ft_reach_percent"].value,
            Some(CanonicalStatNumericValue::Float(100.0))
        );
        assert_eq!(
            canonical_stats.values["deep_ft_avg_stack_chips"].value,
            Some(CanonicalStatNumericValue::Float(
                deep_ft_entry.get::<_, f64>(0)
            ))
        );
        assert_eq!(
            canonical_stats.values["deep_ft_avg_stack_bb"].value,
            Some(CanonicalStatNumericValue::Float(
                deep_ft_entry.get::<_, f64>(1)
            ))
        );
        assert_eq!(
            canonical_stats.values["deep_ft_roi_pct"].value,
            Some(CanonicalStatNumericValue::Float(720.0))
        );
        assert_eq!(
            canonical_stats.values["early_ft_bust_count"].value,
            Some(CanonicalStatNumericValue::Integer(
                stage_event_counts.get::<_, i64>(0) as u64
            ))
        );
        assert_eq!(
            canonical_stats.values["early_ft_bust_per_tournament"].value,
            Some(CanonicalStatNumericValue::Float(
                stage_event_counts.get::<_, i64>(0) as f64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_2_3"].value,
            Some(CanonicalStatNumericValue::Float(
                stage_event_counts.get::<_, i64>(1) as f64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_2_3_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_2_3_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_3_4"].value,
            Some(CanonicalStatNumericValue::Float(
                stage_event_counts.get::<_, i64>(2) as f64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_3_4_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_3_4_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_4_5"].value,
            Some(CanonicalStatNumericValue::Float(
                stage_event_counts.get::<_, i64>(3) as f64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_4_5_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_4_5_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_5_6"].value,
            Some(CanonicalStatNumericValue::Float(
                stage_event_counts.get::<_, i64>(4) as f64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_5_6_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_5_6_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_6_9"].value,
            Some(CanonicalStatNumericValue::Float(
                stage_event_counts.get::<_, i64>(5) as f64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_6_9_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_6_9_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_7_9"].value,
            Some(CanonicalStatNumericValue::Float(
                stage_event_counts.get::<_, i64>(6) as f64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_7_9_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_7_9_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["pre_ft_ko"].value,
            Some(CanonicalStatNumericValue::Float(
                stage_event_counts.get::<_, i64>(7) as f64
            ))
        );
        assert_eq!(
            canonical_stats.values["pre_ft_chips"].value,
            Some(CanonicalStatNumericValue::Float(pre_ft_chip_delta))
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_keeps_early_ft_ko_seed_stats_exact_without_proxy_hand_features() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let hh_path =
            fixture_path("../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt");

        let ts_report = import_path(&ts_path).unwrap();
        import_path(&hh_path).unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let player_profile_id: Uuid = client
            .query_one(
                "SELECT player_profile_id
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap()
            .get(0);
        let organization_id: Uuid = client
            .query_one(
                "SELECT organization_id
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap()
            .get(0);

        client
            .execute(
                "DELETE FROM analytics.player_hand_bool_features
                 WHERE organization_id = $1
                   AND player_profile_id = $2
                   AND feature_key = 'is_stage_6_9'",
                &[&organization_id, &player_profile_id],
            )
            .unwrap();
        client
            .execute(
                "DELETE FROM analytics.player_hand_num_features
                 WHERE organization_id = $1
                   AND player_profile_id = $2
                   AND feature_key = 'hero_exact_ko_event_count'",
                &[&organization_id, &player_profile_id],
            )
            .unwrap();

        let seed_stats = query_seed_stats(
            &mut client,
            SeedStatsFilters {
                organization_id,
                player_profile_id,
                buyin_total_cents: Some(vec![2_500]),
            },
        )
        .unwrap();

        assert_eq!(seed_stats.early_ft_ko_event_count, 1);
        assert_eq!(seed_stats.early_ft_ko_event_per_tournament, Some(1.0));
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_exposes_exact_core_descriptors_to_runtime_filters() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        let ts_path = fixture_path(
            "../../fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        );
        let ts_report = import_path(&ts_path).unwrap();
        let organization_id: Uuid = setup_client
            .query_one(
                "SELECT organization_id
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap()
            .get(0);
        let player_profile_id: Uuid = setup_client
            .query_one(
                "SELECT player_profile_id
                 FROM core.tournaments
                 WHERE id = $1",
                &[&ts_report.tournament_id],
            )
            .unwrap()
            .get(0);
        drop(setup_client);

        let temp_dir = std::env::temp_dir();
        let unique_suffix = format!(
            "{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let write_hand = |prefix: &str, contents: String| {
            let path = temp_dir.join(format!("{prefix}-{unique_suffix}.txt"));
            fs::write(&path, contents).unwrap();
            path
        };

        import_path(
            write_hand("cm10-ft", first_ft_hand_text())
                .to_str()
                .unwrap(),
        )
        .unwrap();
        import_path(
            write_hand(
                "cm10-cm05",
                cm05_hidden_showdown_hand_text()
                    .replace("Tournament #999051", "Tournament #271770266")
                    .replace(
                        "Seat 1: ShortyA mucked",
                        "Seat 1: ShortyA folded before Flop",
                    )
                    .replace(
                        "Seat 2: ShortyB mucked",
                        "Seat 2: ShortyB folded before Flop",
                    ),
            )
            .to_str()
            .unwrap(),
        )
        .unwrap();
        import_path(
            write_hand(
                "cm10-cm06",
                cm06_joint_ko_hand_text().replace("Tournament #999060", "Tournament #271770266"),
            )
            .to_str()
            .unwrap(),
        )
        .unwrap();
        import_path(
            write_hand(
                "cm10-illegal",
                cm10_illegal_actor_order_hand_text()
                    .replace("Tournament #999006", "Tournament #271770266"),
            )
            .to_str()
            .unwrap(),
        )
        .unwrap();

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let expected_hand_ids = [
            ("BR1064987693", "ft_exact_ko"),
            ("BR1064987693", "ft_summary_position"),
            ("BRCM0502", "uncertain_reason"),
            ("BRCM0601", "position_all_in"),
            ("BRCM0601", "joint_ko_participant"),
            ("BRLEGAL2", "legality_issue"),
        ]
        .into_iter()
        .map(|(external_hand_id, label)| {
            let hand_id: Uuid = client
                .query_one(
                    "SELECT id
                     FROM core.hands
                     WHERE player_profile_id = $1
                       AND external_hand_id = $2",
                    &[&player_profile_id, &external_hand_id],
                )
                .unwrap()
                .get(0);
            (label, hand_id)
        })
        .collect::<BTreeMap<_, _>>();

        let uncertainty_matches = query_matching_hand_ids(
            &mut client,
            &hand_query_request(
                organization_id,
                player_profile_id,
                vec![FilterCondition {
                    feature: FeatureRef::Hand {
                        feature_key:
                            "has_uncertain_reason_code:pot_settlement_ambiguous_hidden_showdown"
                                .to_string(),
                    },
                    operator: FilterOperator::Eq,
                    value: FilterValue::Bool(true),
                }],
                vec![],
            ),
        )
        .unwrap()
        .hand_ids;
        assert_eq!(
            uncertainty_matches,
            vec![expected_hand_ids["uncertain_reason"]]
        );

        let legality_matches = query_matching_hand_ids(
            &mut client,
            &hand_query_request(
                organization_id,
                player_profile_id,
                vec![FilterCondition {
                    feature: FeatureRef::Hand {
                        feature_key: "has_action_legality_issue:illegal_actor_order".to_string(),
                    },
                    operator: FilterOperator::Eq,
                    value: FilterValue::Bool(true),
                }],
                vec![],
            ),
        )
        .unwrap()
        .hand_ids;
        assert_eq!(legality_matches, vec![expected_hand_ids["legality_issue"]]);

        let position_all_in_matches = query_matching_hand_ids(
            &mut client,
            &hand_query_request(
                organization_id,
                player_profile_id,
                vec![],
                vec![
                    FilterCondition {
                        feature: FeatureRef::Street {
                            street: "seat".to_string(),
                            feature_key: "position_index".to_string(),
                        },
                        operator: FilterOperator::Eq,
                        value: FilterValue::Num(2.0),
                    },
                    FilterCondition {
                        feature: FeatureRef::Street {
                            street: "seat".to_string(),
                            feature_key: "has_all_in_reason:call_exhausted".to_string(),
                        },
                        operator: FilterOperator::Eq,
                        value: FilterValue::Bool(true),
                    },
                    FilterCondition {
                        feature: FeatureRef::Street {
                            street: "seat".to_string(),
                            feature_key: "summary_outcome_kind".to_string(),
                        },
                        operator: FilterOperator::Eq,
                        value: FilterValue::Enum("showed_won".to_string()),
                    },
                ],
            ),
        )
        .unwrap()
        .hand_ids;
        assert_eq!(
            position_all_in_matches,
            vec![expected_hand_ids["position_all_in"]]
        );

        let summary_position_matches = query_matching_hand_ids(
            &mut client,
            &hand_query_request(
                organization_id,
                player_profile_id,
                vec![],
                vec![
                    FilterCondition {
                        feature: FeatureRef::Street {
                            street: "seat".to_string(),
                            feature_key: "position_label".to_string(),
                        },
                        operator: FilterOperator::Eq,
                        value: FilterValue::Enum("BTN".to_string()),
                    },
                    FilterCondition {
                        feature: FeatureRef::Street {
                            street: "seat".to_string(),
                            feature_key: "summary_outcome_kind".to_string(),
                        },
                        operator: FilterOperator::Eq,
                        value: FilterValue::Enum("showed_lost".to_string()),
                    },
                ],
            ),
        )
        .unwrap()
        .hand_ids;
        assert_eq!(
            summary_position_matches,
            vec![expected_hand_ids["ft_summary_position"]]
        );

        let exact_ko_participant_matches = query_matching_hand_ids(
            &mut client,
            &hand_query_request(
                organization_id,
                player_profile_id,
                vec![FilterCondition {
                    feature: FeatureRef::Street {
                        street: "seat".to_string(),
                        feature_key: "is_exact_ko_participant".to_string(),
                    },
                    operator: FilterOperator::Eq,
                    value: FilterValue::Bool(true),
                }],
                vec![],
            ),
        )
        .unwrap()
        .hand_ids;
        let mut expected_exact_ko_participant_matches = vec![
            expected_hand_ids["ft_exact_ko"],
            expected_hand_ids["joint_ko_participant"],
        ];
        expected_exact_ko_participant_matches.sort_unstable();
        assert_eq!(
            exact_ko_participant_matches,
            expected_exact_ko_participant_matches
        );
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_full_pack_smoke_is_clean() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_core_schema_migrations(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );
        drop(setup_client);

        for (ts_fixture, _) in FULL_PACK_FIXTURE_PAIRS {
            let ts_path = fixture_path(&format!("../../fixtures/mbr/ts/{ts_fixture}"));
            let tournament_summary =
                parse_tournament_summary(&fs::read_to_string(&ts_path).unwrap()).unwrap();
            import_path_with_database_url(&database_url, &ts_path).unwrap();

            let mut visibility_client = Client::connect(&database_url, NoTls).unwrap();
            let player_profile_id = dev_player_profile_id(&mut visibility_client);
            let room_id: Uuid = visibility_client
                .query_one("SELECT id FROM core.rooms WHERE code = 'gg'", &[])
                .unwrap()
                .get(0);
            let persisted_tournament_count: i64 = visibility_client
                .query_one(
                    "SELECT COUNT(*)
                     FROM core.tournaments
                     WHERE player_profile_id = $1
                       AND room_id = $2
                       AND external_tournament_id = $3",
                    &[
                        &player_profile_id,
                        &room_id,
                        &tournament_summary.tournament_id.to_string(),
                    ],
                )
                .unwrap()
                .get(0);
            assert_eq!(
                persisted_tournament_count, 1,
                "TS fixture `{ts_fixture}` did not persist tournament {}",
                tournament_summary.tournament_id
            );
            drop(visibility_client);
        }

        let mut visibility_client = Client::connect(&database_url, NoTls).unwrap();
        let player_profile_id = dev_player_profile_id(&mut visibility_client);
        let committed_tournament_count: i64 = visibility_client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.tournaments
                 WHERE player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(
            committed_tournament_count,
            FULL_PACK_FIXTURE_PAIRS.len() as i64
        );
        drop(visibility_client);

        for (_, hh_fixture) in FULL_PACK_FIXTURE_PAIRS {
            let hh_path = fixture_path(&format!("../../fixtures/mbr/hh/{hh_fixture}"));
            import_path_with_database_url(&database_url, &hh_path).unwrap_or_else(|error| {
                panic!("HH fixture `{hh_fixture}` failed after committed TS preload: {error:#}")
            });
        }

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let player_profile_id = dev_player_profile_id(&mut client);
        let imported_tournament_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.tournaments
                 WHERE player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(
            imported_tournament_count,
            FULL_PACK_FIXTURE_PAIRS.len() as i64
        );

        let unexpected_parse_issues = client
            .query(
                "SELECT pi.code, pi.message
                 FROM core.parse_issues pi
                 JOIN import.source_files sf ON sf.id = pi.source_file_id
                 WHERE sf.player_profile_id = $1
                 ORDER BY sf.original_filename, pi.code, pi.message",
                &[&player_profile_id],
            )
            .unwrap()
            .into_iter()
            .map(|row| (row.get::<_, String>(0), row.get::<_, String>(1)))
            .filter(|(code, message)| !is_expected_committed_parse_issue(code, message))
            .collect::<Vec<_>>();
        assert!(unexpected_parse_issues.is_empty());

        let uncertain_resolution_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.hand_state_resolutions hs
                 JOIN core.hands h ON h.id = hs.hand_id
                 WHERE h.player_profile_id = $1
                   AND hs.settlement_state <> 'exact'",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(uncertain_resolution_count, 0);

        let invariant_mismatch_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.hand_state_resolutions hs
                 JOIN core.hands h ON h.id = hs.hand_id
                 WHERE h.player_profile_id = $1
                   AND (
                       NOT hs.chip_conservation_ok
                       OR NOT hs.pot_conservation_ok
                       OR jsonb_array_length(hs.invariant_issues) > 0
                   )",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(invariant_mismatch_count, 0);

        let non_exact_elimination_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.hand_eliminations e
                 JOIN core.hands h ON h.id = e.hand_id
                 WHERE h.player_profile_id = $1
                   AND (
                       e.elimination_certainty_state <> 'exact'
                       OR e.ko_certainty_state <> 'exact'
                   )",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(non_exact_elimination_count, 0);
    }

    fn first_ft_hand_text() -> String {
        let content = fs::read_to_string(fixture_path(
            "../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt",
        ))
        .unwrap();
        content.split("\n\n").next().unwrap().trim().to_string()
    }

    fn summary_outcome_hand_text() -> String {
        r#"Poker Hand #BRSUMMARY1: Tournament #999101, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:30:00
Table '1' 8-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: VillainA (1,000 in chips)
Seat 3: VillainB (1,000 in chips)
Seat 4: VillainC (1,000 in chips)
Seat 5: VillainD (1,000 in chips)
Seat 6: VillainE (1,000 in chips)
Seat 7: VillainF (1,000 in chips)
Seat 8: VillainG (1,000 in chips)
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
*** SHOWDOWN ***
Hero collected 110 from pot
*** SUMMARY ***
Total pot 3,454 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) won (110)
Seat 2: VillainA (small blind) folded before Flop
Seat 2: Hero lost
Seat 3: VillainB (big blind) folded on the Flop
Seat 4: VillainC showed [Qh Kh] and lost with a pair of Kings
Seat 5: VillainD showed [2s 6c] and won (1,944) with two pair, Sixes and Twos
Seat 6: VillainE lost
Seat 7: VillainF mucked
Seat 8: VillainG collected (200)
Seat 9: VillainX (button) ???"#.to_string()
    }

    fn cm04_import_surface_hand_text() -> String {
        r#"Poker Hand #BRCM0408: Tournament #999208, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 13:35:00
Table '15' 5-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: Sitout (1,000 in chips) is sitting out
Seat 3: ShortBlind (50 in chips)
Seat 4: VillainDead (1,000 in chips)
Seat 5: VillainNoShow (1,000 in chips)
ShortBlind: posts small blind 50
VillainDead: posts dead 100
VillainNoShow: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Hero: folds
*** SHOWDOWN ***
VillainDead: shows [5d]
VillainNoShow: doesn't show hand
VillainDead collected 250 from pot
*** SUMMARY ***
Total pot 250 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Seat 1: Hero folded before Flop
Seat 3: ShortBlind (small blind) lost
Seat 4: VillainDead showed [5d] and won (250)
Seat 5: VillainNoShow (big blind) lost"#.to_string()
    }

    fn cm05_hidden_showdown_hand_text() -> String {
        r#"Poker Hand #BRCM0502: Tournament #999051, Mystery Battle Royale $25 Hold'em No Limit - Level1(0/0(100)) - 2026/03/16 13:45:00
Table '16' 4-max Seat #1 is the button
Seat 1: ShortyA (100 in chips)
Seat 2: ShortyB (100 in chips)
Seat 3: Hero (300 in chips)
Seat 4: Villain (300 in chips)
ShortyA: posts the ante 100
ShortyB: posts the ante 100
Hero: posts the ante 100
Villain: posts the ante 100
*** HOLE CARDS ***
Dealt to ShortyA
Dealt to ShortyB
Dealt to Hero [Ah Ad]
Dealt to Villain
Hero: bets 200 and is all-in
Villain: calls 200 and is all-in
Hero: shows [Ah Ad]
*** SHOWDOWN ***
Hero collected 400 from pot
Villain collected 400 from pot
*** SUMMARY ***
Total pot 800 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: ShortyA mucked
Seat 2: ShortyB mucked
Seat 3: Hero showed [Ah Ad] and collected (400)
Seat 4: Villain collected (400)"#
            .to_string()
    }

    fn cm06_joint_ko_hand_text() -> String {
        r#"Poker Hand #BRCM0601: Tournament #999060, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 14:00:00
Table '18' 3-max Seat #1 is the button
Seat 1: Hero (1,500 in chips)
Seat 2: Shorty (500 in chips)
Seat 3: Medium (1,000 in chips)
Shorty: posts small blind 50
Medium: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [Ah Ad]
Dealt to Shorty
Dealt to Medium
Hero: raises 400 to 500
Shorty: calls 450 and is all-in
Medium: calls 400
*** FLOP *** [2c 7d 9h]
Medium: bets 500 and is all-in
Hero: calls 500
*** TURN *** [2c 7d 9h] [Qs]
*** RIVER *** [2c 7d 9h Qs] [3c]
*** SHOWDOWN ***
Hero: shows [Ah Ad]
Shorty: shows [2h 2d]
Medium: shows [Kc Qc]
Shorty collected 1,500 from pot
Hero collected 1,000 from pot
*** SUMMARY ***
Total pot 2,500 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Qs 3c]
Seat 1: Hero (button) showed [Ah Ad] and collected (1,000)
Seat 2: Shorty (small blind) showed [2h 2d] and collected (1,500)
Seat 3: Medium (big blind) showed [Kc Qc] and lost"#
            .to_string()
    }

    fn cm10_illegal_actor_order_hand_text() -> String {
        r#"Poker Hand #BRLEGAL2: Tournament #999006, Mystery Battle Royale $25 Hold'em No Limit - Level1(50/100(0)) - 2026/03/16 12:25:00
Table '5' 2-max Seat #1 is the button
Seat 1: Hero (1,000 in chips)
Seat 2: Villain (1,000 in chips)
Hero: posts small blind 50
Villain: posts big blind 100
*** HOLE CARDS ***
Dealt to Hero [As Ac]
Dealt to Villain
Villain: checks
Hero: calls 50
*** FLOP *** [2c 7d 9h]
Villain: checks
Hero: checks
*** TURN *** [2c 7d 9h] [Js]
Villain: checks
Hero: checks
*** RIVER *** [2c 7d 9h Js] [3c]
Villain: checks
Hero: checks
*** SHOWDOWN ***
Hero: shows [As Ac]
Villain: shows [Kd Kh]
Hero collected 200 from pot
*** SUMMARY ***
Total pot 200 | Rake 0 | Jackpot 0 | Bingo 0 | Fortune 0 | Tax 0
Board [2c 7d 9h Js 3c]
Seat 1: Hero (button) showed [As Ac] and won (200)
Seat 2: Villain (big blind) showed [Kd Kh] and lost"#
            .to_string()
    }

    fn is_expected_committed_parse_issue(code: &str, message: &str) -> bool {
        code == "partial_reveal_show_line"
            && message == "partial_reveal_show_line: 43b06066: shows [5d] (a pair of Fives)"
    }

    fn second_ft_hand_text() -> String {
        let content = fs::read_to_string(fixture_path(
            "../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt",
        ))
        .unwrap();
        content.split("\n\n").nth(1).unwrap().trim().to_string()
    }

    fn all_hands_from_fixture(filename: &str) -> Vec<CanonicalParsedHand> {
        let content =
            fs::read_to_string(fixture_path(&format!("../../fixtures/mbr/hh/{filename}"))).unwrap();

        split_hand_history(&content)
            .unwrap()
            .iter()
            .map(|hand| parse_canonical_hand(&hand.raw_text).unwrap())
            .collect()
    }

    fn manual_attempt_test_hand(
        seats: Vec<(&str, u8, i64)>,
        actions: Vec<tracker_parser_core::models::HandActionEvent>,
    ) -> CanonicalParsedHand {
        CanonicalParsedHand {
            header: tracker_parser_core::models::HandHeader {
                hand_id: "TEST-KO-ATTEMPT".to_string(),
                tournament_id: 42,
                game_name: "Mystery Battle Royale".to_string(),
                level_name: "Level1".to_string(),
                small_blind: 50,
                big_blind: 100,
                ante: 0,
                played_at: "2026/03/28 12:00:00".to_string(),
                table_name: "1".to_string(),
                max_players: seats.len() as u8,
                button_seat: 1,
            },
            hero_name: Some("Hero".to_string()),
            seats: seats
                .into_iter()
                .map(|(player_name, seat_no, starting_stack)| {
                    tracker_parser_core::models::ParsedHandSeat {
                        seat_no,
                        player_name: player_name.to_string(),
                        starting_stack,
                        is_sitting_out: false,
                    }
                })
                .collect(),
            actions,
            board_final: vec![],
            summary_total_pot: None,
            summary_rake_amount: None,
            summary_board: vec![],
            hero_hole_cards: None,
            showdown_hands: BTreeMap::new(),
            summary_seat_outcomes: vec![],
            collected_amounts: BTreeMap::new(),
            raw_hand_text: String::new(),
            parse_issues: vec![],
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn manual_action(
        seq: usize,
        street: Street,
        player_name: Option<&str>,
        action_type: ActionType,
        amount: Option<i64>,
        to_amount: Option<i64>,
        is_all_in: bool,
        all_in_reason: Option<tracker_parser_core::models::AllInReason>,
        forced_all_in_preflop: bool,
        raw_line: &str,
    ) -> tracker_parser_core::models::HandActionEvent {
        tracker_parser_core::models::HandActionEvent {
            seq,
            street,
            player_name: player_name.map(str::to_string),
            action_type,
            is_forced: false,
            is_all_in,
            all_in_reason,
            forced_all_in_preflop,
            amount,
            to_amount,
            cards: None,
            raw_line: raw_line.to_string(),
        }
    }

    fn fixture_path(relative_from_crate: &str) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(relative_from_crate)
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    fn apply_sql_file(client: &mut Client, path: &str) {
        let sql = fs::read_to_string(path).unwrap();
        client.batch_execute(&sql).unwrap();
    }

    fn apply_core_schema_migrations(client: &mut Client) {
        let schema_ready: bool = client
            .query_one(
                "SELECT
                    to_regclass('import.ingest_bundles') IS NOT NULL
                    AND to_regclass('derived.preflop_starting_hands') IS NOT NULL
                    AND to_regclass('derived.hand_ko_attempts') IS NOT NULL",
                &[],
            )
            .unwrap()
            .get(0);
        if schema_ready {
            return;
        }

        apply_sql_file(
            client,
            &fixture_path("../../migrations/0001_init_source_of_truth.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0002_exact_pot_ko_core.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0003_mbr_stage_economics.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0004_exact_core_schema_v2.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0005_hand_summary_results.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0006_hand_positions.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0007_hand_action_all_in_metadata.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0016_ko_credit_pot_no.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0017_tournament_hand_order.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0018_hand_positions_v2.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0011_mbr_boundary_resolution_v2.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0012_mbr_tournament_ft_helper.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0008_street_hand_strength_canonical_contract.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0009_hand_pot_eligibility_and_uncertain_codes.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0010_hand_eliminations_ko_v2.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0013_player_street_features.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0014_mbr_stage_predicates_v1.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0015_ko_event_money_contracts.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0019_unified_settlement_contract.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0020_hand_eliminations_v2.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0021_ingest_runtime_runner.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0022_web_upload_member_ingest.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0023_file_fragments_member_uniqueness.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0024_user_timezone_and_gg_timestamp_contract.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0025_ingest_bundle_queue_order.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0026_preflop_matrix_filters.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0027_mbr_ko_attempts.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0028_pair_aware_ingest_queue.sql"),
        );
        apply_sql_file(
            client,
            &fixture_path("../../migrations/0029_file_fragments_source_uniqueness_cleanup.sql"),
        );
    }

    fn dev_player_profile_id(client: &mut Client) -> Uuid {
        client
            .query_one(
                "SELECT id
                 FROM core.player_profiles
                 WHERE organization_id = (
                     SELECT id FROM org.organizations WHERE name = $1
                 )
                   AND room = 'gg'
                   AND screen_name = $2",
                &[&DEV_ORG_NAME, &DEV_PLAYER_NAME],
            )
            .unwrap()
            .get(0)
    }

    fn dev_org_id(client: &mut Client) -> Uuid {
        client
            .query_one(
                "SELECT id
                 FROM org.organizations
                 WHERE name = $1",
                &[&DEV_ORG_NAME],
            )
            .unwrap()
            .get(0)
    }

    fn transition_stage_weight(first_ft_table_size: Option<i32>) -> f64 {
        match first_ft_table_size {
            Some(8) => 0.40,
            Some(7) => 0.50,
            Some(6) | Some(5) => 0.60,
            Some(4) => 0.65,
            Some(3) | Some(2) => 0.70,
            _ => 0.0,
        }
    }

    fn delete_analytics_rows_for_player(client: &mut Client, player_profile_id: Uuid) {
        client
            .execute(
                "DELETE FROM analytics.player_hand_bool_features
                 WHERE player_profile_id = $1
                    OR hand_id IN (
                        SELECT id FROM core.hands WHERE player_profile_id = $1
                    )",
                &[&player_profile_id],
            )
            .unwrap();
        client
            .execute(
                "DELETE FROM analytics.player_hand_num_features
                 WHERE player_profile_id = $1
                    OR hand_id IN (
                        SELECT id FROM core.hands WHERE player_profile_id = $1
                    )",
                &[&player_profile_id],
            )
            .unwrap();
        client
            .execute(
                "DELETE FROM analytics.player_hand_enum_features
                 WHERE player_profile_id = $1
                    OR hand_id IN (
                        SELECT id FROM core.hands WHERE player_profile_id = $1
                    )",
                &[&player_profile_id],
            )
            .unwrap();
        if client
            .query_one(
                "SELECT to_regclass('analytics.player_street_bool_features') IS NOT NULL",
                &[],
            )
            .unwrap()
            .get::<_, bool>(0)
        {
            client
                .execute(
                    "DELETE FROM analytics.player_street_bool_features
                     WHERE player_profile_id = $1
                        OR hand_id IN (
                            SELECT id FROM core.hands WHERE player_profile_id = $1
                        )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM analytics.player_street_num_features
                     WHERE player_profile_id = $1
                        OR hand_id IN (
                            SELECT id FROM core.hands WHERE player_profile_id = $1
                        )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM analytics.player_street_enum_features
                     WHERE player_profile_id = $1
                        OR hand_id IN (
                            SELECT id FROM core.hands WHERE player_profile_id = $1
                        )",
                    &[&player_profile_id],
                )
                .unwrap();
        }
    }

    fn reset_dev_player_data(client: &mut Client) {
        let player_profile_id = client
            .query_opt(
                "SELECT id
                 FROM core.player_profiles
                 WHERE organization_id = (
                     SELECT id FROM org.organizations WHERE name = $1
                 )
                   AND room = 'gg'
                   AND screen_name = $2",
                &[&DEV_ORG_NAME, &DEV_PLAYER_NAME],
            )
            .unwrap()
            .map(|row| row.get::<_, Uuid>(0));

        if let Some(player_profile_id) = player_profile_id {
            delete_analytics_rows_for_player(client, player_profile_id);
            if client
                .query_one(
                    "SELECT to_regclass('derived.hand_ko_attempts') IS NOT NULL",
                    &[],
                )
                .unwrap()
                .get::<_, bool>(0)
            {
                client
                    .execute(
                        "DELETE FROM derived.hand_ko_attempts
                         WHERE hand_id IN (
                             SELECT id FROM core.hands WHERE player_profile_id = $1
                         )",
                        &[&player_profile_id],
                    )
                    .unwrap();
            }
            if client
                .query_one(
                    "SELECT to_regclass('derived.hand_ko_opportunities') IS NOT NULL",
                    &[],
                )
                .unwrap()
                .get::<_, bool>(0)
            {
                client
                    .execute(
                        "DELETE FROM derived.hand_ko_opportunities
                         WHERE hand_id IN (
                             SELECT id FROM core.hands WHERE player_profile_id = $1
                         )",
                        &[&player_profile_id],
                    )
                    .unwrap();
            }
            client
                .execute(
                    "DELETE FROM derived.mbr_stage_resolution
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM derived.hand_eliminations
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM derived.hand_state_resolutions
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM derived.street_hand_strength
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            if client
                .query_one(
                    "SELECT to_regclass('derived.preflop_starting_hands') IS NOT NULL",
                    &[],
                )
                .unwrap()
                .get::<_, bool>(0)
            {
                client
                    .execute(
                        "DELETE FROM derived.preflop_starting_hands
                         WHERE hand_id IN (
                             SELECT id FROM core.hands WHERE player_profile_id = $1
                         )",
                        &[&player_profile_id],
                    )
                    .unwrap();
            }
            if client
                .query_one(
                    "SELECT to_regclass('derived.mbr_tournament_ft_helper') IS NOT NULL",
                    &[],
                )
                .unwrap()
                .get::<_, bool>(0)
            {
                client
                    .execute(
                        "DELETE FROM derived.mbr_tournament_ft_helper
                         WHERE player_profile_id = $1",
                        &[&player_profile_id],
                    )
                    .unwrap();
            }
            client
                .execute(
                    "DELETE FROM core.parse_issues
                     WHERE source_file_id IN (
                         SELECT id FROM import.source_files WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.hand_returns
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.hand_pot_winners
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.hand_pot_eligibility
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.hand_pot_contributions
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.hand_pots
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.hand_showdowns
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            if client
                .query_one(
                    "SELECT to_regclass('core.hand_summary_results') IS NOT NULL",
                    &[],
                )
                .unwrap()
                .get::<_, bool>(0)
            {
                client
                    .execute(
                        "DELETE FROM core.hand_summary_results
                         WHERE hand_id IN (
                             SELECT id FROM core.hands WHERE player_profile_id = $1
                         )",
                        &[&player_profile_id],
                    )
                    .unwrap();
            }
            client
                .execute(
                    "DELETE FROM core.hand_hole_cards
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.hand_actions
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.hand_boards
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.hand_seats
                     WHERE hand_id IN (
                         SELECT id FROM core.hands WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            delete_analytics_rows_for_player(client, player_profile_id);
            client
                .execute(
                    "DELETE FROM core.hands WHERE player_profile_id = $1",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.tournament_entries WHERE player_profile_id = $1",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.tournaments WHERE player_profile_id = $1",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM import.file_fragments
                     WHERE source_file_id IN (
                         SELECT id FROM import.source_files WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM import.job_attempts
                     WHERE import_job_id IN (
                         SELECT id
                         FROM import.import_jobs
                         WHERE source_file_id IN (
                             SELECT id FROM import.source_files WHERE player_profile_id = $1
                         )
                            OR bundle_id IN (
                                SELECT id FROM import.ingest_bundles WHERE player_profile_id = $1
                            )
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM import.import_jobs
                     WHERE source_file_id IN (
                         SELECT id FROM import.source_files WHERE player_profile_id = $1
                     )
                        OR bundle_id IN (
                            SELECT id FROM import.ingest_bundles WHERE player_profile_id = $1
                        )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM import.ingest_bundle_files
                     WHERE bundle_id IN (
                         SELECT id FROM import.ingest_bundles WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM import.source_file_members
                     WHERE source_file_id IN (
                         SELECT id FROM import.source_files WHERE player_profile_id = $1
                     )",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM import.ingest_bundles WHERE player_profile_id = $1",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.player_aliases WHERE player_profile_id = $1",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM import.source_files WHERE player_profile_id = $1",
                    &[&player_profile_id],
                )
                .unwrap();
            client
                .execute(
                    "DELETE FROM core.player_profiles WHERE id = $1",
                    &[&player_profile_id],
                )
                .unwrap();
        }
        client
            .execute(
                "DELETE FROM org.organization_memberships
                 WHERE organization_id = (
                     SELECT id FROM org.organizations WHERE name = $1
                 )
                   AND user_id = (
                     SELECT id FROM auth.users WHERE email = $2
                 )",
                &[&DEV_ORG_NAME, &DEV_USER_EMAIL],
            )
            .unwrap();
        client
            .execute(
                "DELETE FROM auth.users WHERE email = $1",
                &[&DEV_USER_EMAIL],
            )
            .unwrap();
        client
            .execute(
                "DELETE FROM org.organizations WHERE name = $1",
                &[&DEV_ORG_NAME],
            )
            .unwrap();
    }
