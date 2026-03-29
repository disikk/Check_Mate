use std::{
    env, fs,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

use mbr_stats_runtime::{CanonicalStatNumericValue, SeedStatsFilters, query_canonical_stats};
use postgres::{Client, NoTls};
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

fn numeric_value_as_f64(value: &CanonicalStatNumericValue) -> f64 {
    match value {
        CanonicalStatNumericValue::Integer(value) => *value as f64,
        CanonicalStatNumericValue::Float(value) => *value,
    }
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
             VALUES ($1, $2, $3, 'gg', 'gg', 'Hero')",
            &[&player_profile_id, &organization_id, &user_id],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO core.player_aliases (organization_id, player_profile_id, room, alias, is_primary, source)
             VALUES ($1, $2, 'gg', 'Hero', TRUE, 'manual')",
            &[&organization_id, &player_profile_id],
        )
        .unwrap();

    (organization_id, user_id, player_profile_id)
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
fn pre_ft_ko_does_not_double_count_boundary_ko_for_multi_elimination_hand() {
    let _guard = db_test_guard();
    let database_url = db_url();
    let mut client = Client::connect(&database_url, NoTls).unwrap();
    apply_all_migrations(&mut client);

    let (organization_id, user_id, player_profile_id) =
        seed_actor_shell(&mut client, "Asia/Krasnoyarsk");

    let room_id: Uuid = client
        .query_one("SELECT id FROM core.rooms WHERE code = 'gg'", &[])
        .unwrap()
        .get(0);
    let format_id: Uuid = client
        .query_one("SELECT id FROM core.formats WHERE code = 'mbr'", &[])
        .unwrap()
        .get(0);

    let source_file_id = Uuid::new_v4();
    let tournament_id = Uuid::new_v4();
    let hand_id = Uuid::new_v4();

    client
        .execute(
            "INSERT INTO import.source_files (
                id, organization_id, uploaded_by_user_id, owner_user_id, player_profile_id,
                room, file_kind, sha256, original_filename, byte_size, storage_uri
             ) VALUES (
                $1, $2, $3, $3, $4,
                'gg', 'hh', repeat('a', 64), 'boundary.txt', 1, 'local:///tmp/boundary.txt'
             )",
            &[
                &source_file_id,
                &organization_id,
                &user_id,
                &player_profile_id,
            ],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO core.tournaments (
                id, organization_id, player_profile_id, room_id, format_id, external_tournament_id,
                buyin_total, buyin_prize_component, buyin_bounty_component, fee_component, currency, max_players
             ) VALUES (
                $1, $2, $3, $4, $5, 'boundary-regression',
                10.00, 5.00, 4.20, 0.80, 'USD', 18
             )",
            &[&tournament_id, &organization_id, &player_profile_id, &room_id, &format_id],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO core.hands (
                id, organization_id, player_profile_id, tournament_id, source_file_id, external_hand_id,
                hand_started_at, table_name, table_max_seats, dealer_seat_no, small_blind, big_blind, ante, currency
             ) VALUES (
                $1, $2, $3, $4, $5, 'BRBOUNDARY1',
                '2026-03-28 10:00:00+07', '1', 9, 1, 100, 200, 40, 'USD'
             )",
            &[&hand_id, &organization_id, &player_profile_id, &tournament_id, &source_file_id],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO core.hand_seats (hand_id, seat_no, player_name, player_profile_id, starting_stack, is_hero)
             VALUES
                ($1, 1, 'Hero', $2, 2000, TRUE),
                ($1, 2, 'Villain-1', NULL, 400, FALSE),
                ($1, 3, 'Villain-2', NULL, 300, FALSE)",
            &[&hand_id, &player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO derived.mbr_stage_resolution (
                hand_id, player_profile_id, entered_boundary_zone, entered_boundary_zone_state,
                boundary_ko_ev, boundary_ko_min, boundary_ko_max, boundary_ko_method, boundary_ko_certainty,
                boundary_ko_state, boundary_resolution_state, boundary_candidate_count, is_boundary_hand
             ) VALUES (
                $1, $2, TRUE, 'exact',
                1.0, 1.0, 1.0, 'exact', 'exact',
                'exact', 'exact', 1, TRUE
             )",
            &[&hand_id, &player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO derived.mbr_tournament_ft_helper (
                tournament_id, player_profile_id, reached_ft_exact, first_ft_table_size,
                entered_boundary_zone, boundary_resolution_state
             ) VALUES (
                $1, $2, TRUE, 8,
                TRUE, 'exact'
             )",
            &[&tournament_id, &player_profile_id],
        )
        .unwrap();
    client
        .execute(
            "INSERT INTO derived.hand_eliminations (
                hand_id, eliminated_seat_no, eliminated_player_name,
                ko_involved_winner_count, hero_involved, is_split_ko, is_sidepot_based,
                certainty_state, joint_ko, ko_pot_resolution_type, money_share_model_state,
                elimination_certainty_state, ko_certainty_state
             ) VALUES
                ($1, 2, 'Villain-1', 0, FALSE, FALSE, FALSE, 'exact', FALSE, 'unresolved', 'blocked_uncertain_event', 'exact', 'uncertain'),
                ($1, 3, 'Villain-2', 0, FALSE, FALSE, FALSE, 'exact', FALSE, 'unresolved', 'blocked_uncertain_event', 'exact', 'uncertain')",
            &[&hand_id],
        )
        .unwrap();

    let snapshot = query_canonical_stats(
        &mut client,
        SeedStatsFilters {
            organization_id,
            player_profile_id,
            buyin_total_cents: None,
        },
    )
    .expect("canonical stats query");

    let pre_ft_ko = snapshot.values["pre_ft_ko"]
        .value
        .as_ref()
        .map(numeric_value_as_f64)
        .expect("pre_ft_ko value");
    let total_ko = snapshot.values["total_ko"]
        .value
        .as_ref()
        .map(numeric_value_as_f64)
        .expect("total_ko value");

    assert!(
        (pre_ft_ko - 0.4).abs() < 1e-9,
        "expected weighted boundary KO to count once, got {pre_ft_ko}"
    );
    assert!(
        (total_ko - 0.4).abs() < 1e-9,
        "expected total_ko to inherit single weighted boundary KO, got {total_ko}"
    );
}
