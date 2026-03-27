use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::PathBuf,
    sync::OnceLock,
};

use serde::{Deserialize, Serialize};
use tracker_parser_core::{
    models::{CanonicalParsedHand, Street},
    parsers::hand_history::{parse_canonical_hand, split_hand_history},
    street_strength::{StreetHandStrength, evaluate_street_hand_strength},
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct CorpusHandRef {
    fixture_file: String,
    hand_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct KnownSeatSnapshot {
    player_name: String,
    hole_cards: Vec<String>,
    is_hero: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StreetStrengthRowSnapshot {
    seat_no: u8,
    street: String,
    best_hand_class: String,
    best_hand_rank_value: i64,
    made_hand_category: String,
    draw_category: String,
    overcards_count: u8,
    has_air: bool,
    missed_flush_draw: bool,
    missed_straight_draw: bool,
    is_nut_hand: Option<bool>,
    is_nut_draw: Option<bool>,
    certainty_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CuratedHandSnapshot {
    hand_id: String,
    board: Vec<String>,
    known_seats: BTreeMap<u8, KnownSeatSnapshot>,
    rows: Vec<StreetStrengthRowSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CuratedFixtureSnapshot {
    fixture_file: String,
    hands: BTreeMap<String, CuratedHandSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct StreetStrengthCuratedCorpusSnapshot {
    fixtures: BTreeMap<String, CuratedFixtureSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
struct StreetStrengthFullPackAggregateSnapshot {
    fixture_count: usize,
    hand_count: usize,
    materialized_hand_count: usize,
    materialized_row_count: usize,
    hero_row_count: usize,
    showdown_known_opponent_row_count: usize,
    hands_with_hero_only_rows: usize,
    hands_with_showdown_opponent_rows: usize,
    hands_with_multiple_showdown_opponents: usize,
    best_hand_class_counts: BTreeMap<String, usize>,
    made_hand_category_counts: BTreeMap<String, usize>,
    draw_category_counts: BTreeMap<String, usize>,
    missed_flush_draw_true_count: usize,
    missed_straight_draw_true_count: usize,
    is_nut_hand_true_count: usize,
    is_nut_draw_true_count: usize,
    best_hand_class_witnesses: BTreeMap<String, Vec<CorpusHandRef>>,
    made_hand_category_witnesses: BTreeMap<String, Vec<CorpusHandRef>>,
    draw_category_witnesses: BTreeMap<String, Vec<CorpusHandRef>>,
    scenario_witnesses: BTreeMap<String, Vec<CorpusHandRef>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MaterializedStreetStrengthHand {
    fixture_file: String,
    hand_id: String,
    board: Vec<String>,
    known_seats: BTreeMap<u8, KnownSeatSnapshot>,
    hero_seat_no: Option<u8>,
    rows: Vec<StreetStrengthRowSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreetStrengthCorpusIndex {
    fixture_count: usize,
    hand_count: usize,
    hands: Vec<MaterializedStreetStrengthHand>,
}

#[derive(Debug, Clone, Copy)]
struct CuratedHandSelection {
    fixture_file: &'static str,
    hand_ids: &'static [&'static str],
}

const CURATED_HAND_SELECTION: &[CuratedHandSelection] = &[
    CuratedHandSelection {
        fixture_file: "GG20260316-0307 - Mystery Battle Royale 25.txt",
        hand_ids: &[
            "BR1064992157",
            "BR1064992307",
            "BR1064992513",
            "BR1064992706",
        ],
    },
    CuratedHandSelection {
        fixture_file: "GG20260316-0312 - Mystery Battle Royale 25.txt",
        hand_ids: &["BR1065002400", "BR1065002573"],
    },
    CuratedHandSelection {
        fixture_file: "GG20260316-0316 - Mystery Battle Royale 25.txt",
        hand_ids: &["BR1065011867"],
    },
    CuratedHandSelection {
        fixture_file: "GG20260316-0323 - Mystery Battle Royale 25.txt",
        hand_ids: &["BR1065003863"],
    },
];
const WITNESS_LIMIT: usize = 5;

static CORPUS_INDEX: OnceLock<StreetStrengthCorpusIndex> = OnceLock::new();

#[test]
fn street_strength_curated_corpus_golden_matches_selected_real_hands() {
    let actual = build_curated_corpus_snapshot();
    let golden_path = curated_golden_path();

    if env::var("UPDATE_GOLDENS").is_ok() {
        fs::create_dir_all(
            golden_path
                .parent()
                .expect("golden path must have parent directory"),
        )
        .expect("golden directory must be creatable");
        let json = serde_json::to_string_pretty(&actual).expect("golden JSON must serialize");
        fs::write(&golden_path, json).expect("golden file must be writable");
        eprintln!("Golden updated at: {}", golden_path.display());
        return;
    }

    assert!(
        golden_path.exists(),
        "Missing curated street-strength corpus golden at {}.\nRun with UPDATE_GOLDENS=1 cargo test -p tracker_parser_core --test street_strength_corpus_golden",
        golden_path.display()
    );

    let expected_json = fs::read_to_string(&golden_path).expect("golden file must be readable");
    let expected: StreetStrengthCuratedCorpusSnapshot =
        serde_json::from_str(&expected_json).expect("golden JSON must parse");

    assert_eq!(actual, expected);
}

#[test]
fn street_strength_full_pack_aggregate_golden_matches_committed_hh_pack() {
    let actual = build_full_pack_aggregate_snapshot();
    let golden_path = full_pack_golden_path();

    if env::var("UPDATE_GOLDENS").is_ok() {
        fs::create_dir_all(
            golden_path
                .parent()
                .expect("golden path must have parent directory"),
        )
        .expect("golden directory must be creatable");
        let json = serde_json::to_string_pretty(&actual).expect("golden JSON must serialize");
        fs::write(&golden_path, json).expect("golden file must be writable");
        eprintln!("Golden updated at: {}", golden_path.display());
        return;
    }

    assert!(
        golden_path.exists(),
        "Missing full-pack street-strength aggregate golden at {}.\nRun with UPDATE_GOLDENS=1 cargo test -p tracker_parser_core --test street_strength_corpus_golden",
        golden_path.display()
    );

    let expected_json = fs::read_to_string(&golden_path).expect("golden file must be readable");
    let expected: StreetStrengthFullPackAggregateSnapshot =
        serde_json::from_str(&expected_json).expect("golden JSON must parse");

    assert_eq!(actual, expected);
}

fn build_curated_corpus_snapshot() -> StreetStrengthCuratedCorpusSnapshot {
    assert!(
        !CURATED_HAND_SELECTION.is_empty(),
        "curated street-strength corpus selection must not be empty"
    );
    let selection = curated_selection_map();
    let mut fixtures = BTreeMap::new();
    let mut found = BTreeMap::<String, BTreeSet<String>>::new();

    for hand in &corpus_index().hands {
        let Some(selected_ids) = selection.get(hand.fixture_file.as_str()) else {
            continue;
        };
        if !selected_ids.contains(hand.hand_id.as_str()) {
            continue;
        }

        let fixture_entry = fixtures
            .entry(hand.fixture_file.clone())
            .or_insert_with(|| CuratedFixtureSnapshot {
                fixture_file: hand.fixture_file.clone(),
                hands: BTreeMap::new(),
            });
        found.entry(hand.fixture_file.clone())
            .or_default()
            .insert(hand.hand_id.clone());
        fixture_entry.hands.insert(
            hand.hand_id.clone(),
            CuratedHandSnapshot {
                hand_id: hand.hand_id.clone(),
                board: hand.board.clone(),
                known_seats: hand.known_seats.clone(),
                rows: hand.rows.clone(),
            },
        );
    }

    for selection in CURATED_HAND_SELECTION {
        let found_ids = found
            .get(selection.fixture_file)
            .cloned()
            .unwrap_or_default();
        for hand_id in selection.hand_ids {
            assert!(
                found_ids.contains(*hand_id),
                "curated selection hand {} in fixture {} was not found among materialized street-strength hands",
                hand_id,
                selection.fixture_file
            );
        }
    }

    StreetStrengthCuratedCorpusSnapshot { fixtures }
}

fn build_full_pack_aggregate_snapshot() -> StreetStrengthFullPackAggregateSnapshot {
    let corpus = corpus_index();
    let mut snapshot = StreetStrengthFullPackAggregateSnapshot {
        fixture_count: corpus.fixture_count,
        hand_count: corpus.hand_count,
        materialized_hand_count: corpus.hands.len(),
        ..StreetStrengthFullPackAggregateSnapshot::default()
    };

    for hand in &corpus.hands {
        let hand_ref = CorpusHandRef {
            fixture_file: hand.fixture_file.clone(),
            hand_id: hand.hand_id.clone(),
        };
        let opponent_seats = hand
            .rows
            .iter()
            .map(|row| row.seat_no)
            .filter(|seat_no| Some(*seat_no) != hand.hero_seat_no)
            .collect::<BTreeSet<_>>();

        snapshot.materialized_row_count += hand.rows.len();
        if opponent_seats.is_empty() {
            snapshot.hands_with_hero_only_rows += 1;
            push_witness(
                &mut snapshot.scenario_witnesses,
                "hero_only_rows",
                &hand_ref,
            );
        } else {
            snapshot.hands_with_showdown_opponent_rows += 1;
            push_witness(
                &mut snapshot.scenario_witnesses,
                "showdown_opponent_rows",
                &hand_ref,
            );
        }
        if opponent_seats.len() > 1 {
            snapshot.hands_with_multiple_showdown_opponents += 1;
            push_witness(
                &mut snapshot.scenario_witnesses,
                "multiple_showdown_opponents",
                &hand_ref,
            );
        }

        for row in &hand.rows {
            if Some(row.seat_no) == hand.hero_seat_no {
                snapshot.hero_row_count += 1;
            } else {
                snapshot.showdown_known_opponent_row_count += 1;
            }

            increment_count(&mut snapshot.best_hand_class_counts, &row.best_hand_class);
            increment_count(
                &mut snapshot.made_hand_category_counts,
                &row.made_hand_category,
            );
            increment_count(&mut snapshot.draw_category_counts, &row.draw_category);

            push_witness(
                &mut snapshot.best_hand_class_witnesses,
                &row.best_hand_class,
                &hand_ref,
            );
            push_witness(
                &mut snapshot.made_hand_category_witnesses,
                &row.made_hand_category,
                &hand_ref,
            );
            push_witness(
                &mut snapshot.draw_category_witnesses,
                &row.draw_category,
                &hand_ref,
            );

            if row.missed_flush_draw {
                snapshot.missed_flush_draw_true_count += 1;
                push_witness(
                    &mut snapshot.scenario_witnesses,
                    "missed_flush_draw_true",
                    &hand_ref,
                );
            }
            if row.missed_straight_draw {
                snapshot.missed_straight_draw_true_count += 1;
                push_witness(
                    &mut snapshot.scenario_witnesses,
                    "missed_straight_draw_true",
                    &hand_ref,
                );
            }
            if row.is_nut_hand == Some(true) {
                snapshot.is_nut_hand_true_count += 1;
                push_witness(&mut snapshot.scenario_witnesses, "is_nut_hand_true", &hand_ref);
            }
            if row.is_nut_draw == Some(true) {
                snapshot.is_nut_draw_true_count += 1;
                push_witness(&mut snapshot.scenario_witnesses, "is_nut_draw_true", &hand_ref);
            }
        }
    }

    snapshot
}

fn corpus_index() -> &'static StreetStrengthCorpusIndex {
    CORPUS_INDEX.get_or_init(build_corpus_index)
}

fn build_corpus_index() -> StreetStrengthCorpusIndex {
    let fixture_paths = committed_hh_fixture_paths();
    let mut hand_count = 0;
    let mut hands = Vec::new();

    for fixture_path in &fixture_paths {
        let fixture_text =
            fs::read_to_string(fixture_path).expect("fixture file must be readable as UTF-8");
        let hand_records = split_hand_history(&fixture_text).expect("fixture must split into hands");
        hand_count += hand_records.len();

        for record in hand_records {
            let parsed =
                parse_canonical_hand(&record.raw_text).expect("fixture hand must parse canonically");
            let rows = evaluate_street_hand_strength(&parsed).expect("street strength must evaluate");
            if rows.is_empty() {
                continue;
            }

            hands.push(MaterializedStreetStrengthHand {
                fixture_file: fixture_path
                    .file_name()
                    .expect("fixture file must have name")
                    .to_string_lossy()
                    .into_owned(),
                hand_id: parsed.header.hand_id.clone(),
                board: final_board_cards(&parsed),
                known_seats: known_seats_snapshot(&parsed),
                hero_seat_no: hero_seat_no(&parsed),
                rows: rows.iter().map(snapshot_row).collect(),
            });
        }
    }

    StreetStrengthCorpusIndex {
        fixture_count: fixture_paths.len(),
        hand_count,
        hands,
    }
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

fn curated_selection_map() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    CURATED_HAND_SELECTION
        .iter()
        .map(|selection| {
            (
                selection.fixture_file,
                selection.hand_ids.iter().copied().collect::<BTreeSet<_>>(),
            )
        })
        .collect()
}

fn final_board_cards(hand: &CanonicalParsedHand) -> Vec<String> {
    if !hand.board_final.is_empty() {
        hand.board_final.clone()
    } else {
        hand.summary_board.clone()
    }
}

fn hero_seat_no(hand: &CanonicalParsedHand) -> Option<u8> {
    let hero_name = hand.hero_name.as_deref()?;
    hand.seats
        .iter()
        .find(|seat| seat.player_name == hero_name)
        .map(|seat| seat.seat_no)
}

fn known_seats_snapshot(hand: &CanonicalParsedHand) -> BTreeMap<u8, KnownSeatSnapshot> {
    let seat_by_name = hand
        .seats
        .iter()
        .map(|seat| (seat.player_name.as_str(), seat.seat_no))
        .collect::<BTreeMap<_, _>>();
    let mut known_seats = BTreeMap::new();

    if let (Some(hero_name), Some(hero_cards)) = (&hand.hero_name, &hand.hero_hole_cards)
        && let Some(&seat_no) = seat_by_name.get(hero_name.as_str())
    {
        known_seats.insert(
            seat_no,
            KnownSeatSnapshot {
                player_name: hero_name.clone(),
                hole_cards: hero_cards.clone(),
                is_hero: true,
            },
        );
    }

    for (player_name, cards) in &hand.showdown_hands {
        let Some(&seat_no) = seat_by_name.get(player_name.as_str()) else {
            continue;
        };
        known_seats.entry(seat_no).or_insert_with(|| KnownSeatSnapshot {
            player_name: player_name.clone(),
            hole_cards: cards.clone(),
            is_hero: hand.hero_name.as_deref() == Some(player_name.as_str()),
        });
    }

    known_seats
}

fn snapshot_row(row: &StreetHandStrength) -> StreetStrengthRowSnapshot {
    StreetStrengthRowSnapshot {
        seat_no: row.seat_no,
        street: street_label(row.street).to_string(),
        best_hand_class: row.best_hand_class.as_str().to_string(),
        best_hand_rank_value: row.best_hand_rank_value,
        made_hand_category: row.made_hand_category.as_str().to_string(),
        draw_category: row.draw_category.as_str().to_string(),
        overcards_count: row.overcards_count,
        has_air: row.has_air,
        missed_flush_draw: row.missed_flush_draw,
        missed_straight_draw: row.missed_straight_draw,
        is_nut_hand: row.is_nut_hand,
        is_nut_draw: row.is_nut_draw,
        certainty_state: row.certainty_state.as_str().to_string(),
    }
}

fn street_label(street: Street) -> &'static str {
    match street {
        Street::Preflop => "preflop",
        Street::Flop => "flop",
        Street::Turn => "turn",
        Street::River => "river",
        Street::Showdown => "showdown",
        Street::Summary => "summary",
    }
}

fn increment_count(counts: &mut BTreeMap<String, usize>, key: &str) {
    *counts.entry(key.to_string()).or_default() += 1;
}

fn push_witness(
    witnesses: &mut BTreeMap<String, Vec<CorpusHandRef>>,
    bucket: &str,
    hand_ref: &CorpusHandRef,
) {
    let bucket_witnesses = witnesses.entry(bucket.to_string()).or_default();
    if bucket_witnesses.iter().any(|existing| existing == hand_ref) {
        return;
    }
    if bucket_witnesses.len() < WITNESS_LIMIT {
        bucket_witnesses.push(hand_ref.clone());
    }
}

fn curated_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
        .join("street_strength_curated_corpus.json")
}

fn full_pack_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("goldens")
        .join("street_strength_full_pack_aggregate.json")
}

#[test]
fn committed_fixture_path_discovery_finds_hh_pack() {
    let fixtures = committed_hh_fixture_paths();

    assert!(!fixtures.is_empty());
    assert!(fixtures.iter().all(|path| path.starts_with(backend_root())));
}

#[test]
fn curated_selection_only_references_committed_fixture_files() {
    let fixture_names = committed_hh_fixture_paths()
        .into_iter()
        .map(|path| {
            path.file_name()
                .expect("fixture path must have file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect::<BTreeSet<_>>();

    for selection in CURATED_HAND_SELECTION {
        assert!(
            fixture_names.contains(selection.fixture_file),
            "curated selection references unknown fixture {}",
            selection.fixture_file
        );
    }
}

#[test]
fn curated_selection_covers_required_real_hand_buckets() {
    let curated = build_curated_corpus_snapshot();
    let mut best_hand_classes = BTreeSet::new();
    let mut draw_categories = BTreeSet::new();
    let mut has_hero_only_hand = false;
    let mut has_showdown_opponent_hand = false;
    let mut has_multiple_showdown_opponents_hand = false;
    let mut has_missed_flush_draw = false;
    let mut has_missed_straight_draw = false;
    let mut has_is_nut_hand_true = false;
    let mut has_is_nut_draw_true = false;

    for fixture in curated.fixtures.values() {
        for hand in fixture.hands.values() {
            let hero_seat_count = hand
                .known_seats
                .values()
                .filter(|seat| seat.is_hero)
                .count();
            let showdown_opponent_count = hand
                .known_seats
                .values()
                .filter(|seat| !seat.is_hero)
                .count();

            if hero_seat_count == 1 && showdown_opponent_count == 0 {
                has_hero_only_hand = true;
            }
            if showdown_opponent_count >= 1 {
                has_showdown_opponent_hand = true;
            }
            if showdown_opponent_count > 1 {
                has_multiple_showdown_opponents_hand = true;
            }

            for row in &hand.rows {
                best_hand_classes.insert(row.best_hand_class.as_str());
                if row.draw_category != "none" {
                    draw_categories.insert(row.draw_category.as_str());
                }
                has_missed_flush_draw |= row.missed_flush_draw;
                has_missed_straight_draw |= row.missed_straight_draw;
                has_is_nut_hand_true |= row.is_nut_hand == Some(true);
                has_is_nut_draw_true |= row.is_nut_draw == Some(true);
            }
        }
    }

    for required_class in [
        "pair",
        "trips",
        "two_pair",
        "straight",
        "flush",
        "full_house",
        "quads",
    ] {
        assert!(
            best_hand_classes.contains(required_class),
            "curated selection must cover best_hand_class {}",
            required_class
        );
    }

    for required_draw in [
        "gutshot",
        "open_ended",
        "double_gutshot",
        "flush_draw",
        "combo_draw",
        "backdoor_flush_only",
    ] {
        assert!(
            draw_categories.contains(required_draw),
            "curated selection must cover draw_category {}",
            required_draw
        );
    }

    assert!(has_hero_only_hand, "curated selection must include a hero-only hand");
    assert!(
        has_showdown_opponent_hand,
        "curated selection must include showdown-known opponent materialization"
    );
    assert!(
        has_multiple_showdown_opponents_hand,
        "curated selection must include a hand with multiple showdown-known opponents"
    );
    assert!(
        has_missed_flush_draw,
        "curated selection must include missed_flush_draw = true"
    );
    assert!(
        has_missed_straight_draw,
        "curated selection must include missed_straight_draw = true"
    );
    assert!(
        has_is_nut_hand_true,
        "curated selection must include is_nut_hand = true"
    );
    assert!(
        has_is_nut_draw_true,
        "curated selection must include is_nut_draw = true"
    );
}
