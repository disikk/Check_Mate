use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use tracker_parser_core::{
    normalizer::normalize_hand,
    parsers::hand_history::{parse_canonical_hand, split_hand_history},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct GoldenFixtureSnapshot {
    fixture_file: String,
    hand_count: usize,
    hands: BTreeMap<String, serde_json::Value>,
}

fn backend_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate directory must have parent")
        .parent()
        .expect("backend root must exist")
        .to_path_buf()
}

fn committed_hh_fixture_paths() -> Vec<PathBuf> {
    let mut fixtures = fs::read_dir(backend_root().join("fixtures").join("mbr").join("hh"))
        .expect("committed HH fixture directory must exist")
        .map(|entry| entry.expect("fixture dir entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("txt"))
        .collect::<Vec<_>>();
    fixtures.sort();
    fixtures
}

fn golden_path_for_fixture(fixture_path: &Path) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
        .join(
            fixture_path
                .file_stem()
                .expect("fixture file must have stem")
                .to_string_lossy()
                .to_string()
                + ".json",
        )
}

fn build_fixture_snapshot(fixture_path: &Path) -> GoldenFixtureSnapshot {
    let fixture_text =
        fs::read_to_string(fixture_path).expect("fixture file must be readable as UTF-8");
    let hand_records = split_hand_history(&fixture_text).expect("fixture must split into hands");

    let hands = hand_records
        .iter()
        .map(|record| {
            let parsed = parse_canonical_hand(&record.raw_text)
                .expect("fixture hand must parse canonically");
            let normalized = normalize_hand(&parsed).expect("fixture hand must normalize");
            let serialized =
                serde_json::to_value(&normalized).expect("normalized hand must serialize");
            (parsed.header.hand_id.clone(), serialized)
        })
        .collect::<BTreeMap<_, _>>();

    GoldenFixtureSnapshot {
        fixture_file: fixture_path
            .file_name()
            .expect("fixture file must have name")
            .to_string_lossy()
            .into_owned(),
        hand_count: hands.len(),
        hands,
    }
}

fn describe_fixture_diffs(
    actual: &GoldenFixtureSnapshot,
    expected: &GoldenFixtureSnapshot,
) -> Vec<String> {
    let mut diffs = Vec::new();

    if actual.fixture_file != expected.fixture_file {
        diffs.push(format!(
            "fixture_file: {:?} -> {:?}",
            expected.fixture_file, actual.fixture_file
        ));
    }

    if actual.hand_count != expected.hand_count {
        diffs.push(format!(
            "hand_count: {} -> {}",
            expected.hand_count, actual.hand_count
        ));
    }

    let actual_ids = actual.hands.keys().cloned().collect::<BTreeSet<_>>();
    let expected_ids = expected.hands.keys().cloned().collect::<BTreeSet<_>>();

    let missing = expected_ids
        .difference(&actual_ids)
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        diffs.push(format!("missing hand_ids: {:?}", missing));
    }

    let extra = actual_ids
        .difference(&expected_ids)
        .cloned()
        .collect::<Vec<_>>();
    if !extra.is_empty() {
        diffs.push(format!("extra hand_ids: {:?}", extra));
    }

    let changed = expected_ids
        .intersection(&actual_ids)
        .filter(|hand_id| actual.hands.get(*hand_id) != expected.hands.get(*hand_id))
        .cloned()
        .collect::<Vec<_>>();
    if !changed.is_empty() {
        diffs.push(format!("changed hand payloads: {:?}", changed));
    }

    diffs
}

fn assert_fixture_snapshot_matches(
    fixture_path: &Path,
    actual: &GoldenFixtureSnapshot,
    expected: &GoldenFixtureSnapshot,
) {
    let diffs = describe_fixture_diffs(actual, expected);
    if diffs.is_empty() {
        return;
    }

    panic!(
        "Golden mismatch for fixture {}.\n{}\nRun with UPDATE_GOLDENS=1 to refresh it.",
        fixture_path.display(),
        diffs.join("\n")
    );
}

#[test]
fn normalized_hand_golden_matches_committed_pack() {
    for fixture_path in committed_hh_fixture_paths() {
        let actual = build_fixture_snapshot(&fixture_path);
        let golden_path = golden_path_for_fixture(&fixture_path);

        if env::var("UPDATE_GOLDENS").is_ok() {
            fs::create_dir_all(
                golden_path
                    .parent()
                    .expect("golden file must have parent directory"),
            )
            .expect("golden directory must be creatable");
            let json = serde_json::to_string_pretty(&actual).expect("golden JSON must serialize");
            fs::write(&golden_path, json).expect("golden file must be writable");
            eprintln!("Golden updated at: {}", golden_path.display());
            continue;
        }

        assert!(
            golden_path.exists(),
            "Missing golden for fixture {} at {}.\nRun with UPDATE_GOLDENS=1 to create it.",
            fixture_path.display(),
            golden_path.display()
        );

        let expected_json = fs::read_to_string(&golden_path).expect("golden file must be readable");
        let expected: GoldenFixtureSnapshot =
            serde_json::from_str(&expected_json).expect("golden JSON must parse");
        assert_fixture_snapshot_matches(&fixture_path, &actual, &expected);
    }
}

#[test]
fn fixture_diff_reports_missing_and_extra_hand_ids() {
    let expected = GoldenFixtureSnapshot {
        fixture_file: "fixture.txt".to_string(),
        hand_count: 2,
        hands: BTreeMap::from([
            ("H1".to_string(), serde_json::json!({"value": 1})),
            ("H2".to_string(), serde_json::json!({"value": 2})),
        ]),
    };
    let actual = GoldenFixtureSnapshot {
        fixture_file: "fixture.txt".to_string(),
        hand_count: 2,
        hands: BTreeMap::from([
            ("H1".to_string(), serde_json::json!({"value": 1})),
            ("H3".to_string(), serde_json::json!({"value": 3})),
        ]),
    };

    let diffs = describe_fixture_diffs(&actual, &expected);

    assert!(
        diffs
            .iter()
            .any(|diff| diff.contains("missing hand_ids: [\"H2\"]"))
    );
    assert!(
        diffs
            .iter()
            .any(|diff| diff.contains("extra hand_ids: [\"H3\"]"))
    );
}

#[test]
fn fixture_diff_reports_changed_hand_payload() {
    let expected = GoldenFixtureSnapshot {
        fixture_file: "fixture.txt".to_string(),
        hand_count: 1,
        hands: BTreeMap::from([("H1".to_string(), serde_json::json!({"value": 1}))]),
    };
    let actual = GoldenFixtureSnapshot {
        fixture_file: "fixture.txt".to_string(),
        hand_count: 1,
        hands: BTreeMap::from([("H1".to_string(), serde_json::json!({"value": 9}))]),
    };

    let diffs = describe_fixture_diffs(&actual, &expected);

    assert!(
        diffs
            .iter()
            .any(|diff| diff.contains("changed hand payloads: [\"H1\"]"))
    );
}
