use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use mbr_stats_runtime::{
    FEATURE_VERSION, materialize_player_hand_features_for_bundle,
    materialize_player_hand_features_for_tournaments,
};
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

fn seed_actor_shell(
    client: &mut impl postgres::GenericClient,
    timezone_name: &str,
) -> (Uuid, Uuid, Uuid) {
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
    runner_name: &str,
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
        run_ingest_runner_until_idle(database_url, runner_name, 3).expect("runner");
    assert!(processed_jobs > 0, "runner must process at least one job");

    bundle.bundle_id
}

fn count_hand_bool_rows_for_tournament(
    client: &mut Client,
    player_profile_id: Uuid,
    tournament_id: Uuid,
) -> i64 {
    client
        .query_one(
            "SELECT COUNT(*)
             FROM analytics.player_hand_bool_features features
             INNER JOIN core.hands hands ON hands.id = features.hand_id
             WHERE features.player_profile_id = $1
               AND features.feature_version = $2
               AND hands.tournament_id = $3",
            &[&player_profile_id, &FEATURE_VERSION, &tournament_id],
        )
        .unwrap()
        .get(0)
}

fn load_bundle_tournament_stats(
    client: &mut Client,
    bundle_id: Uuid,
    player_profile_id: Uuid,
) -> (Uuid, i64) {
    client
        .query_one(
            "SELECT h.tournament_id, COUNT(DISTINCT h.id)::bigint
             FROM import.import_jobs jobs
             INNER JOIN core.hands h
               ON h.source_file_id = jobs.source_file_id
             WHERE jobs.bundle_id = $1
               AND jobs.job_kind = 'file_ingest'
               AND h.player_profile_id = $2
             GROUP BY h.tournament_id",
            &[&bundle_id, &player_profile_id],
        )
        .map(|row| (row.get(0), row.get(1)))
        .unwrap()
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn scoped_materializer_rebuilds_only_requested_tournaments_and_bundle_scope() {
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
            fixture_path(
                "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
            ),
            fixture_path("fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"),
        ],
        "materializer_scoped_first",
    );
    let second_bundle_id = enqueue_fixture_bundle(
        &database_url,
        organization_id,
        user_id,
        player_profile_id,
        &[
            fixture_path(
                "fixtures/mbr/ts/GG20260316 - Tournament #271769484 - Mystery Battle Royale 25.txt",
            ),
            fixture_path("fixtures/mbr/hh/GG20260316-0338 - Mystery Battle Royale 25.txt"),
        ],
        "materializer_scoped_second",
    );

    let mut client = Client::connect(&database_url, NoTls).unwrap();
    let (first_tournament_id, first_tournament_hand_count) =
        load_bundle_tournament_stats(&mut client, first_bundle_id, player_profile_id);
    let (second_tournament_id, second_tournament_hand_count) =
        load_bundle_tournament_stats(&mut client, second_bundle_id, player_profile_id);

    client
        .execute(
            "DELETE FROM analytics.player_hand_bool_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "DELETE FROM analytics.player_hand_num_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "DELETE FROM analytics.player_hand_enum_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "DELETE FROM analytics.player_street_bool_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "DELETE FROM analytics.player_street_num_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "DELETE FROM analytics.player_street_enum_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();

    let tournament_report = materialize_player_hand_features_for_tournaments(
        &mut client,
        organization_id,
        player_profile_id,
        &[first_tournament_id],
    )
    .unwrap();
    assert_eq!(
        tournament_report.hand_count as i64,
        first_tournament_hand_count
    );
    assert!(
        count_hand_bool_rows_for_tournament(&mut client, player_profile_id, first_tournament_id)
            > 0
    );
    assert_eq!(
        count_hand_bool_rows_for_tournament(&mut client, player_profile_id, second_tournament_id),
        0
    );

    client
        .execute(
            "DELETE FROM analytics.player_hand_bool_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "DELETE FROM analytics.player_hand_num_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "DELETE FROM analytics.player_hand_enum_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "DELETE FROM analytics.player_street_bool_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "DELETE FROM analytics.player_street_num_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "DELETE FROM analytics.player_street_enum_features WHERE player_profile_id = $1",
            &[&player_profile_id],
        )
        .unwrap();

    let bundle_report = materialize_player_hand_features_for_bundle(
        &mut client,
        organization_id,
        player_profile_id,
        second_bundle_id,
    )
    .unwrap();
    assert_eq!(
        bundle_report.hand_count as i64,
        second_tournament_hand_count
    );
    assert_eq!(
        count_hand_bool_rows_for_tournament(&mut client, player_profile_id, first_tournament_id),
        0
    );
    assert!(
        count_hand_bool_rows_for_tournament(&mut client, player_profile_id, second_tournament_id)
            > 0
    );

    let empty_report = materialize_player_hand_features_for_tournaments(
        &mut client,
        organization_id,
        player_profile_id,
        &[],
    )
    .unwrap();
    assert_eq!(empty_report.hand_count, 0);
    assert_eq!(empty_report.bool_rows, 0);
    assert_eq!(empty_report.street_row_count, 0);

    let _ = first_bundle_id;
}
