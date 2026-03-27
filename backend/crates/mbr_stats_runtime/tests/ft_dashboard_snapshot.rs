use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use mbr_stats_runtime::{FtDashboardDataState, FtDashboardFilters, query_ft_dashboard};
use parser_worker::local_import::run_ingest_runner_until_idle;
use postgres::{Client, NoTls};
use tracker_ingest_runtime::{FileKind, IngestBundleInput, IngestFileInput, enqueue_bundle};
use uuid::Uuid;

fn backend_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("backend root must exist")
        .to_path_buf()
}

fn migrations_dir() -> PathBuf {
    backend_root().join("migrations")
}

fn seed_path() -> PathBuf {
    backend_root().join("seeds").join("0001_reference_data.sql")
}

fn fixture_path(relative: &str) -> PathBuf {
    backend_root().join(relative)
}

fn db_url() -> String {
    env::var("CHECK_MATE_DATABASE_URL")
        .expect("CHECK_MATE_DATABASE_URL must exist for mbr_stats_runtime DB tests")
}

fn db_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn apply_all_migrations(client: &mut Client) {
    let mut paths = fs::read_dir(migrations_dir())
        .expect("migrations dir must exist")
        .map(|entry| entry.expect("entry must load").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("sql"))
        .collect::<Vec<_>>();
    paths.sort();

    for path in paths {
        let sql = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read migration {}: {error}", path.display()));
        client
            .batch_execute(&sql)
            .unwrap_or_else(|error| panic!("failed to apply {}: {error}", path.display()));
    }

    let seed_sql = fs::read_to_string(seed_path()).expect("reference seed must exist");
    client
        .batch_execute(&seed_sql)
        .expect("reference seed must apply");
}

fn seed_actor_shell(client: &mut impl postgres::GenericClient, timezone_name: &str) -> (Uuid, Uuid, Uuid) {
    let organization_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let player_profile_id = Uuid::new_v4();

    client
        .execute(
            "INSERT INTO org.organizations (id, name) VALUES ($1, $2)",
            &[&organization_id, &format!("org-{organization_id}")],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO auth.users (id, email, auth_provider, status, timezone_name)
             VALUES ($1, $2, 'seed', 'active', $3)",
            &[&user_id, &format!("{user_id}@example.com"), &timezone_name],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO org.organization_memberships (organization_id, user_id, role)
             VALUES ($1, $2, 'student')",
            &[&organization_id, &user_id],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO core.player_profiles (id, organization_id, owner_user_id, room, network, screen_name)
             VALUES ($1, $2, $3, 'gg', 'gg', $4)",
            &[&player_profile_id, &organization_id, &user_id, &format!("Hero-{player_profile_id}")],
        )
        .unwrap();

    (organization_id, user_id, player_profile_id)
}

fn build_flat_ingest_file_input(path: &Path) -> IngestFileInput {
    let input = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read fixture {}: {error}", path.display()));
    let original_filename = path.file_name().unwrap().to_string_lossy().to_string();
    let file_kind = if original_filename.contains("Tournament #") {
        FileKind::TournamentSummary
    } else {
        FileKind::HandHistory
    };

    IngestFileInput {
        room: "gg".to_string(),
        file_kind,
        sha256: Uuid::new_v4().simple().to_string().repeat(2),
        original_filename,
        byte_size: input.len() as i64,
        storage_uri: format!("local://{}", path.display()),
        members: vec![],
        diagnostics: vec![],
    }
}

fn enqueue_fixture_bundle(
    database_url: &str,
    organization_id: Uuid,
    user_id: Uuid,
    player_profile_id: Uuid,
    fixture_paths: &[PathBuf],
) -> Uuid {
    let mut client = Client::connect(database_url, NoTls).expect("database connection");
    let mut tx = client.transaction().expect("bundle enqueue transaction");
    let bundle = enqueue_bundle(
        &mut tx,
        &IngestBundleInput {
            organization_id,
            player_profile_id,
            created_by_user_id: user_id,
            files: fixture_paths
                .iter()
                .map(|path| build_flat_ingest_file_input(path))
                .collect(),
        },
    )
    .expect("bundle enqueue");
    tx.commit().expect("bundle enqueue commit");

    let processed_jobs =
        run_ingest_runner_until_idle(database_url, "mbr_ft_dashboard_test", 3).expect("runner");
    assert!(processed_jobs > 0, "runner must process at least one job");

    bundle.bundle_id
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn ft_dashboard_snapshot_respects_live_bundle_and_date_filters() {
    let _guard = db_test_guard();
    let database_url = db_url();
    let mut client = Client::connect(&database_url, NoTls).unwrap();
    apply_all_migrations(&mut client);

    let (organization_id, user_id, player_profile_id) =
        seed_actor_shell(&mut client, "Asia/Krasnoyarsk");
    drop(client);

    let first_bundle_id = enqueue_fixture_bundle(
        &database_url,
        organization_id,
        user_id,
        player_profile_id,
        &[
            fixture_path("fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"),
            fixture_path("fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"),
        ],
    );
    let second_bundle_id = enqueue_fixture_bundle(
        &database_url,
        organization_id,
        user_id,
        player_profile_id,
        &[
            fixture_path("fixtures/mbr/ts/GG20260316 - Tournament #271769772 - Mystery Battle Royale 25.txt"),
            fixture_path("fixtures/mbr/hh/GG20260316-0342 - Mystery Battle Royale 25.txt"),
        ],
    );

    let mut check_client = Client::connect(&database_url, NoTls).unwrap();
    let snapshot = query_ft_dashboard(
        &mut check_client,
        FtDashboardFilters {
            organization_id,
            player_profile_id,
            buyin_total_cents: None,
            bundle_id: None,
            date_from_local: None,
            date_to_local: None,
            timezone_name: "Asia/Krasnoyarsk".to_string(),
        },
    )
    .expect("ft dashboard snapshot");

    assert_eq!(snapshot.data_state, FtDashboardDataState::Ready);
    assert_eq!(snapshot.coverage.summary_tournament_count, 2);
    assert_eq!(snapshot.coverage.hand_tournament_count, 2);
    assert_eq!(
        snapshot
            .filter_options
            .bundle_options
            .iter()
            .map(|option| option.bundle_id)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([first_bundle_id, second_bundle_id])
    );
    assert_eq!(
        snapshot
            .charts
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "all".to_string(),
            "avg_ko_by_early_ft_stack".to_string(),
            "avg_ko_by_ft_stack".to_string(),
            "avg_ko_by_position".to_string(),
            "ft".to_string(),
            "ft_stack".to_string(),
            "ft_stack_conv".to_string(),
            "ft_stack_conv_5_6".to_string(),
            "ft_stack_conv_7_9".to_string(),
            "ft_stack_roi".to_string(),
            "ft_stack_roi_0_800".to_string(),
            "ko_attempts".to_string(),
            "pre_ft".to_string(),
        ])
    );

    let first_bundle_snapshot = query_ft_dashboard(
        &mut check_client,
        FtDashboardFilters {
            organization_id,
            player_profile_id,
            buyin_total_cents: None,
            bundle_id: Some(first_bundle_id),
            date_from_local: None,
            date_to_local: None,
            timezone_name: "Asia/Krasnoyarsk".to_string(),
        },
    )
    .expect("bundle filtered snapshot");
    assert_eq!(first_bundle_snapshot.coverage.summary_tournament_count, 1);
    assert_eq!(first_bundle_snapshot.coverage.hand_tournament_count, 1);

    let empty_buyin_snapshot = query_ft_dashboard(
        &mut check_client,
        FtDashboardFilters {
            organization_id,
            player_profile_id,
            buyin_total_cents: Some(vec![1_000]),
            bundle_id: None,
            date_from_local: None,
            date_to_local: None,
            timezone_name: "Asia/Krasnoyarsk".to_string(),
        },
    )
    .expect("empty buyin snapshot");
    assert_eq!(empty_buyin_snapshot.data_state, FtDashboardDataState::Empty);

    let empty_date_snapshot = query_ft_dashboard(
        &mut check_client,
        FtDashboardFilters {
            organization_id,
            player_profile_id,
            buyin_total_cents: None,
            bundle_id: None,
            date_from_local: Some("2026-03-17T00:00".to_string()),
            date_to_local: Some("2026-03-17T23:59".to_string()),
            timezone_name: "Asia/Krasnoyarsk".to_string(),
        },
    )
    .expect("empty date snapshot");
    assert_eq!(empty_date_snapshot.data_state, FtDashboardDataState::Empty);
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn ft_dashboard_snapshot_surfaces_partial_state_for_ts_only_bundle() {
    let _guard = db_test_guard();
    let database_url = db_url();
    let mut client = Client::connect(&database_url, NoTls).unwrap();
    apply_all_migrations(&mut client);

    let (organization_id, user_id, player_profile_id) =
        seed_actor_shell(&mut client, "Asia/Krasnoyarsk");
    drop(client);

    let bundle_id = enqueue_fixture_bundle(
        &database_url,
        organization_id,
        user_id,
        player_profile_id,
        &[fixture_path(
            "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        )],
    );

    let mut check_client = Client::connect(&database_url, NoTls).unwrap();
    let snapshot = query_ft_dashboard(
        &mut check_client,
        FtDashboardFilters {
            organization_id,
            player_profile_id,
            buyin_total_cents: None,
            bundle_id: Some(bundle_id),
            date_from_local: None,
            date_to_local: None,
            timezone_name: "Asia/Krasnoyarsk".to_string(),
        },
    )
    .expect("partial ft dashboard snapshot");

    assert_eq!(snapshot.data_state, FtDashboardDataState::Partial);
    assert_eq!(snapshot.coverage.summary_tournament_count, 1);
    assert_eq!(snapshot.coverage.hand_tournament_count, 0);
    assert_eq!(snapshot.stat_cards["avgFtStack"].state.as_str(), "blocked");
    assert_eq!(snapshot.charts["ft_stack"].state.as_str(), "blocked");
}
