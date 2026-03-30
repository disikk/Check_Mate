//! F3-T2: End-to-end golden tests for the full 60-key canonical stat snapshot.
//!
//! These tests require a live PostgreSQL database with committed fixtures imported.
//! They run `query_canonical_stats` and compare the full snapshot against a golden
//! JSON file. Any unexpected change in the snapshot fails the test with a diff-friendly
//! output showing old vs new values.
//!
//! To update goldens after an intentional change:
//!   1. Run the test with `UPDATE_GOLDENS=1` env var
//!   2. Review the diff in the golden JSON file
//!   3. Commit the updated golden alongside the code change

use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

use mbr_stats_runtime::CANONICAL_STAT_KEYS;
use mbr_stats_runtime::models::{
    CanonicalStatNumericValue, CanonicalStatPoint, CanonicalStatState, SeedStatsFilters,
};
use mbr_stats_runtime::queries::query_canonical_stats;
use parser_worker::local_import::run_ingest_runner_until_idle;
use postgres::{Client, NoTls};
use tracker_ingest_runtime::{FileKind, IngestBundleInput, IngestFileInput, enqueue_bundle};
use uuid::Uuid;

/// Serializable representation of a single stat point for golden comparison.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct GoldenStatPoint {
    state: String,
    value: Option<serde_json::Value>,
}

impl From<&CanonicalStatPoint> for GoldenStatPoint {
    fn from(point: &CanonicalStatPoint) -> Self {
        let state = match point.state {
            CanonicalStatState::Value => "value".to_string(),
            CanonicalStatState::Null => "null".to_string(),
            CanonicalStatState::Blocked => "blocked".to_string(),
        };
        let value = point.value.as_ref().map(|v| match v {
            CanonicalStatNumericValue::Integer(i) => serde_json::json!(*i),
            CanonicalStatNumericValue::Float(f) => {
                // Round to 6 decimal places for stable golden comparison
                serde_json::json!((f * 1_000_000.0).round() / 1_000_000.0)
            }
        });
        GoldenStatPoint { state, value }
    }
}

/// Full golden snapshot structure.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct GoldenSnapshot {
    summary_tournament_count: u64,
    hand_tournament_count: u64,
    key_count: usize,
    stats: BTreeMap<String, GoldenStatPoint>,
}

fn golden_path() -> PathBuf {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("fixtures")
        .join("goldens")
        .join("canonical_snapshot_committed_pack.json")
}

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

fn connect_test_db() -> Option<Client> {
    Client::connect(&db_url(), NoTls).ok()
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
            &[&organization_id, &format!("golden-org-{organization_id}")],
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
             VALUES ($1, $2, $3, 'gg', 'gg', 'Hero')",
            &[&player_profile_id, &organization_id, &user_id],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO core.player_aliases (
                organization_id,
                player_profile_id,
                room,
                alias,
                is_primary,
                source
             )
             VALUES ($1, $2, 'gg', 'Hero', TRUE, 'golden_test')",
            &[&organization_id, &player_profile_id],
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

fn committed_pack_fixture_paths() -> Vec<PathBuf> {
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

    let mut paths = Vec::with_capacity(FULL_PACK_FIXTURE_PAIRS.len() * 2);
    for (ts_fixture, hh_fixture) in FULL_PACK_FIXTURE_PAIRS {
        paths.push(fixture_path(&format!("fixtures/mbr/ts/{ts_fixture}")));
        paths.push(fixture_path(&format!("fixtures/mbr/hh/{hh_fixture}")));
    }
    paths
}

fn enqueue_fixture_bundle(
    database_url: &str,
    organization_id: Uuid,
    user_id: Uuid,
    player_profile_id: Uuid,
    fixture_paths: &[PathBuf],
) {
    let mut client = Client::connect(database_url, NoTls).expect("database connection");
    let mut tx = client.transaction().expect("bundle enqueue transaction");
    enqueue_bundle(
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
        run_ingest_runner_until_idle(database_url, "canonical_snapshot_golden", 3).expect("runner");
    assert!(processed_jobs > 0, "runner must process at least one job");
}

fn committed_pack_context() -> (Uuid, Uuid) {
    static CONTEXT: OnceLock<(Uuid, Uuid)> = OnceLock::new();

    *CONTEXT.get_or_init(|| {
        let _guard = db_test_guard();
        let database_url = db_url();
        let mut client = Client::connect(&database_url, NoTls).expect("database connection");
        apply_all_migrations(&mut client);
        let (organization_id, user_id, player_profile_id) =
            seed_actor_shell(&mut client, "Asia/Krasnoyarsk");
        drop(client);

        enqueue_fixture_bundle(
            &database_url,
            organization_id,
            user_id,
            player_profile_id,
            &committed_pack_fixture_paths(),
        );

        (organization_id, player_profile_id)
    })
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL with committed fixtures imported"]
fn golden_canonical_snapshot_matches_committed_pack() {
    let (org_id, player_id) = committed_pack_context();
    let mut client = connect_test_db().expect("database connection");

    let snapshot = query_canonical_stats(
        &mut client,
        SeedStatsFilters {
            organization_id: org_id,
            player_profile_id: player_id,
            buyin_total_cents: None,
        },
    )
    .expect("canonical stats query");

    // Build golden-format snapshot
    let actual = GoldenSnapshot {
        summary_tournament_count: snapshot.coverage.summary_tournament_count,
        hand_tournament_count: snapshot.coverage.hand_tournament_count,
        key_count: snapshot.values.len(),
        stats: snapshot
            .values
            .iter()
            .map(|(key, point)| (key.clone(), GoldenStatPoint::from(point)))
            .collect(),
    };

    // Verify all 60 keys are present
    assert_eq!(
        actual.key_count,
        CANONICAL_STAT_KEYS.len(),
        "snapshot must contain exactly {} keys, got {}",
        CANONICAL_STAT_KEYS.len(),
        actual.key_count
    );

    let golden_file = golden_path();

    if env::var("UPDATE_GOLDENS").is_ok() {
        // Write new golden
        fs::create_dir_all(golden_file.parent().unwrap()).unwrap();
        let json = serde_json::to_string_pretty(&actual).unwrap();
        fs::write(&golden_file, json).unwrap();
        eprintln!("Golden updated at: {}", golden_file.display());
        return;
    }

    if !golden_file.exists() {
        // First run: create golden and pass
        fs::create_dir_all(golden_file.parent().unwrap()).unwrap();
        let json = serde_json::to_string_pretty(&actual).unwrap();
        fs::write(&golden_file, json).unwrap();
        eprintln!(
            "Golden created at: {} — review and commit this file",
            golden_file.display()
        );
        return;
    }

    // Load and compare
    let golden_json = fs::read_to_string(&golden_file).expect("read golden file");
    let expected: GoldenSnapshot = serde_json::from_str(&golden_json).expect("parse golden JSON");

    if actual != expected {
        // Produce diff-friendly output
        let mut diffs = Vec::new();

        if actual.summary_tournament_count != expected.summary_tournament_count {
            diffs.push(format!(
                "summary_tournament_count: {} → {}",
                expected.summary_tournament_count, actual.summary_tournament_count
            ));
        }
        if actual.hand_tournament_count != expected.hand_tournament_count {
            diffs.push(format!(
                "hand_tournament_count: {} → {}",
                expected.hand_tournament_count, actual.hand_tournament_count
            ));
        }

        for key in CANONICAL_STAT_KEYS {
            let key_str = key.to_string();
            let old = expected.stats.get(&key_str);
            let new = actual.stats.get(&key_str);
            if old != new {
                diffs.push(format!(
                    "  {}: {:?} → {:?}",
                    key_str,
                    old.map(|p| format!("{}:{:?}", p.state, p.value)),
                    new.map(|p| format!("{}:{:?}", p.state, p.value))
                ));
            }
        }

        panic!(
            "Golden snapshot mismatch!\n\
             {} diffs detected:\n{}\n\n\
             To update, run with UPDATE_GOLDENS=1",
            diffs.len(),
            diffs.join("\n")
        );
    }
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL with committed fixtures imported"]
fn golden_snapshot_has_no_blocked_or_missing_keys() {
    let (org_id, player_id) = committed_pack_context();
    let mut client = connect_test_db().expect("database connection");

    let snapshot = query_canonical_stats(
        &mut client,
        SeedStatsFilters {
            organization_id: org_id,
            player_profile_id: player_id,
            buyin_total_cents: None,
        },
    )
    .expect("canonical stats query");

    // Every canonical key must be present in the snapshot
    let missing: Vec<_> = CANONICAL_STAT_KEYS
        .iter()
        .filter(|key| !snapshot.values.contains_key(**key))
        .collect();
    assert!(
        missing.is_empty(),
        "Missing keys in canonical snapshot: {:?}",
        missing
    );

    // No key should be in Blocked state for the committed pack
    let blocked: Vec<_> = snapshot
        .values
        .iter()
        .filter(|(_, point)| point.state == CanonicalStatState::Blocked)
        .map(|(key, _)| key.as_str())
        .collect();
    assert!(
        blocked.is_empty(),
        "Blocked keys in canonical snapshot: {:?}",
        blocked
    );
}
