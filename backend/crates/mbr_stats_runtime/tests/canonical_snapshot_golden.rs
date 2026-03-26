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

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use mbr_stats_runtime::models::{
    CanonicalStatNumericValue, CanonicalStatPoint, CanonicalStatState, SeedStatsFilters,
};
use mbr_stats_runtime::queries::query_canonical_stats;
use mbr_stats_runtime::CANONICAL_STAT_KEYS;

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

fn connect_test_db() -> Option<postgres::Client> {
    let url = env::var("CHECK_MATE_DATABASE_URL").ok()?;
    postgres::Client::connect(&url, postgres::NoTls).ok()
}

fn resolve_dev_ids(client: &mut postgres::Client) -> Option<(uuid::Uuid, uuid::Uuid)> {
    let row = client
        .query_opt(
            "SELECT pp.organization_id, pp.id
             FROM core.player_profiles pp
             WHERE pp.room = 'gg' AND pp.screen_name = 'Hero'
             LIMIT 1",
            &[],
        )
        .ok()??;
    Some((row.get(0), row.get(1)))
}

#[test]
#[ignore = "requires CHECK_MATE_DATABASE_URL with committed fixtures imported"]
fn golden_canonical_snapshot_matches_committed_pack() {
    let mut client = connect_test_db().expect("database connection");
    let (org_id, player_id) = resolve_dev_ids(&mut client).expect("dev player context");

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
    let expected: GoldenSnapshot =
        serde_json::from_str(&golden_json).expect("parse golden JSON");

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
    let mut client = connect_test_db().expect("database connection");
    let (org_id, player_id) = resolve_dev_ids(&mut client).expect("dev player context");

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
