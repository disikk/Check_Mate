use std::{collections::BTreeMap, env, fs, path::Path};

use anyhow::{Context, Result, anyhow};
use mbr_stats_runtime::materialize_player_hand_features;
use postgres::{Client, NoTls, Transaction};
use sha2::{Digest, Sha256};
use tracker_parser_core::{
    SourceKind, detect_source_kind,
    models::{ActionType, CanonicalParsedHand, CertaintyState, Street, TournamentSummary},
    normalizer::normalize_hand,
    parsers::{
        hand_history::{parse_canonical_hand, split_hand_history},
        tournament_summary::parse_tournament_summary,
    },
    street_strength::{STREET_HAND_STRENGTH_VERSION, evaluate_street_hand_strength},
};
use uuid::Uuid;

const DEV_ORG_NAME: &str = "Check Mate Dev Org";
const DEV_USER_EMAIL: &str = "mbr-dev-student@example.com";
const DEV_PLAYER_NAME: &str = "Hero";
const HAND_RESOLUTION_VERSION: &str = "gg_mbr_v1";

#[derive(Debug)]
pub struct LocalImportReport {
    pub file_kind: &'static str,
    pub source_file_id: Uuid,
    pub import_job_id: Uuid,
    pub tournament_id: Uuid,
    pub fragments_persisted: usize,
    pub hands_persisted: usize,
}

#[derive(Debug)]
struct DevContext {
    organization_id: Uuid,
    user_id: Uuid,
    player_profile_id: Uuid,
    player_aliases: Vec<String>,
    room_id: Uuid,
    format_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CanonicalHandPersistence {
    seats: Vec<HandSeatRow>,
    hole_cards: Vec<HandHoleCardsRow>,
    actions: Vec<HandActionRow>,
    board: Option<HandBoardRow>,
    showdowns: Vec<HandShowdownRow>,
    parse_issues: Vec<ParseIssueRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandSeatRow {
    seat_no: i32,
    player_name: String,
    starting_stack: i64,
    is_hero: bool,
    is_button: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandHoleCardsRow {
    seat_no: i32,
    card1: Option<String>,
    card2: Option<String>,
    known_to_hero: bool,
    known_at_showdown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandActionRow {
    sequence_no: i32,
    street: String,
    seat_no: Option<i32>,
    action_type: String,
    raw_amount: Option<i64>,
    to_amount: Option<i64>,
    is_all_in: bool,
    references_previous_bet: bool,
    raw_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandBoardRow {
    flop1: Option<String>,
    flop2: Option<String>,
    flop3: Option<String>,
    turn: Option<String>,
    river: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandShowdownRow {
    seat_no: i32,
    shown_cards: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParseIssueRow {
    severity: String,
    code: String,
    message: String,
    raw_line: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandStateResolutionRow {
    resolution_version: String,
    chip_conservation_ok: bool,
    pot_conservation_ok: bool,
    rake_amount: i64,
    final_stacks: BTreeMap<String, i64>,
    invariant_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandPotRow {
    pot_no: i32,
    pot_type: String,
    amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandPotContributionRow {
    pot_no: i32,
    seat_no: i32,
    amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandPotWinnerRow {
    pot_no: i32,
    seat_no: i32,
    share_amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandReturnRow {
    seat_no: i32,
    amount: i64,
    reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandEliminationRow {
    eliminated_seat_no: i32,
    eliminated_player_name: String,
    resolved_by_pot_no: Option<i32>,
    ko_involved_winner_count: i32,
    hero_involved: bool,
    hero_share_fraction: Option<String>,
    is_split_ko: bool,
    split_n: Option<i32>,
    is_sidepot_based: bool,
    certainty_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MbrStageResolutionRow {
    player_profile_id: Uuid,
    played_ft_hand: bool,
    played_ft_hand_state: String,
    entered_boundary_zone: bool,
    entered_boundary_zone_state: String,
    ft_table_size: Option<i32>,
    boundary_ko_ev: Option<String>,
    boundary_ko_min: Option<String>,
    boundary_ko_max: Option<String>,
    boundary_ko_method: Option<String>,
    boundary_ko_certainty: Option<String>,
    boundary_ko_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TournamentEntryEconomics {
    regular_prize_cents: i64,
    mystery_money_cents: i64,
}

#[derive(Debug, Clone, PartialEq)]
struct StageHandFact {
    hand_id: String,
    played_at: String,
    max_players: u8,
    seat_count: usize,
    exact_hero_boundary_ko_share: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreetHandStrengthRow {
    seat_no: i32,
    street: String,
    best_hand_class: String,
    best_hand_rank_value: i64,
    pair_strength: String,
    is_nut_hand: Option<bool>,
    is_nut_draw: Option<bool>,
    has_flush_draw: bool,
    has_backdoor_flush_draw: bool,
    has_open_ended: bool,
    has_gutshot: bool,
    has_double_gutshot: bool,
    has_pair_plus_draw: bool,
    has_overcards: bool,
    has_air: bool,
    has_missed_draw_by_river: bool,
    descriptor_version: String,
    certainty_state: String,
}

pub fn import_path(path: &str) -> Result<LocalImportReport> {
    let input = fs::read_to_string(path).with_context(|| format!("failed to read `{path}`"))?;
    let database_url = env::var("CHECK_MATE_DATABASE_URL")
        .context("CHECK_MATE_DATABASE_URL is required for `import-local`")?;

    let mut client =
        Client::connect(&database_url, NoTls).context("failed to connect to PostgreSQL")?;
    let mut tx = client
        .transaction()
        .context("failed to start import transaction")?;
    let context = ensure_dev_context(&mut tx)?;

    let report = match detect_source_kind(&input)? {
        SourceKind::TournamentSummary => {
            import_tournament_summary(&mut tx, &context, path, &input)?
        }
        SourceKind::HandHistory => import_hand_history(&mut tx, &context, path, &input)?,
    };
    materialize_player_hand_features(&mut tx, context.organization_id, context.player_profile_id)?;

    tx.commit().context("failed to commit import transaction")?;
    Ok(report)
}

fn ensure_dev_context(tx: &mut Transaction<'_>) -> Result<DevContext> {
    let organization_id = if let Some(row) = tx.query_opt(
        "SELECT id FROM org.organizations WHERE name = $1",
        &[&DEV_ORG_NAME],
    )? {
        row.get(0)
    } else {
        tx.query_one(
            "INSERT INTO org.organizations (name) VALUES ($1) RETURNING id",
            &[&DEV_ORG_NAME],
        )?
        .get(0)
    };

    let user_id = if let Some(row) = tx.query_opt(
        "SELECT id FROM auth.users WHERE email = $1",
        &[&DEV_USER_EMAIL],
    )? {
        row.get(0)
    } else {
        tx.query_one(
                "INSERT INTO auth.users (email, auth_provider, status) VALUES ($1, 'seed', 'active') RETURNING id",
                &[&DEV_USER_EMAIL],
            )?
            .get(0)
    };

    tx.execute(
        "INSERT INTO org.organization_memberships (organization_id, user_id, role)
         VALUES ($1, $2, 'student')
         ON CONFLICT (organization_id, user_id) DO NOTHING",
        &[&organization_id, &user_id],
    )?;

    let player_profile_id = if let Some(row) = tx.query_opt(
        "SELECT id FROM core.player_profiles WHERE organization_id = $1 AND room = 'gg' AND screen_name = $2",
        &[&organization_id, &DEV_PLAYER_NAME],
    )? {
        row.get(0)
    } else {
        tx.query_one(
            "INSERT INTO core.player_profiles (organization_id, owner_user_id, room, network, screen_name)
             VALUES ($1, $2, 'gg', 'gg', $3)
             RETURNING id",
            &[&organization_id, &user_id, &DEV_PLAYER_NAME],
        )?
        .get(0)
    };

    tx.execute(
        "INSERT INTO core.player_aliases (
            organization_id,
            player_profile_id,
            room,
            alias,
            is_primary,
            source
        )
        VALUES ($1, $2, 'gg', $3, TRUE, 'dev_context')
        ON CONFLICT (player_profile_id, room, alias)
        DO UPDATE SET
            is_primary = TRUE,
            source = EXCLUDED.source",
        &[&organization_id, &player_profile_id, &DEV_PLAYER_NAME],
    )?;

    let player_aliases = tx
        .query(
            "SELECT alias
             FROM core.player_aliases
             WHERE organization_id = $1
               AND player_profile_id = $2
               AND room = 'gg'
             ORDER BY is_primary DESC, created_at, alias",
            &[&organization_id, &player_profile_id],
        )?
        .into_iter()
        .map(|row| row.get::<_, String>(0))
        .collect::<Vec<_>>();

    let room_id = tx
        .query_one("SELECT id FROM core.rooms WHERE code = 'gg'", &[])?
        .get(0);
    let format_id = tx
        .query_one("SELECT id FROM core.formats WHERE code = 'mbr'", &[])?
        .get(0);

    Ok(DevContext {
        organization_id,
        user_id,
        player_profile_id,
        player_aliases,
        room_id,
        format_id,
    })
}

fn import_tournament_summary(
    tx: &mut Transaction<'_>,
    context: &DevContext,
    path: &str,
    input: &str,
) -> Result<LocalImportReport> {
    let summary = parse_tournament_summary(input)?;
    let tournament_entry_economics = load_tournament_entry_economics(tx, context, &summary)?;
    let source_file_id = insert_source_file(tx, context, path, input, "ts")?;
    insert_source_file_member(tx, source_file_id, path, "ts", input)?;
    let import_job_id = insert_import_job(tx, context.organization_id, source_file_id)?;
    insert_job_attempt(tx, import_job_id)?;
    insert_file_fragment(tx, source_file_id, 0, None, "summary", input)?;

    let tournament_id: Uuid = tx
        .query_one(
            "INSERT INTO core.tournaments (
                organization_id,
                player_profile_id,
                room_id,
                format_id,
                external_tournament_id,
                buyin_total,
                buyin_prize_component,
                buyin_bounty_component,
                fee_component,
                currency,
                max_players,
                started_at,
                started_at_raw,
                started_at_local,
                started_at_tz_provenance,
                source_summary_file_id
            )
            VALUES (
                $1, $2, $3, $4, $5,
                ($6::double precision)::numeric(12,2),
                ($7::double precision)::numeric(12,2),
                ($8::double precision)::numeric(12,2),
                ($9::double precision)::numeric(12,2),
                'USD',
                $10,
                NULL,
                $11,
                replace($11, '/', '-')::timestamp,
                'gg_text_without_timezone',
                $12
            )
            ON CONFLICT (player_profile_id, room_id, external_tournament_id)
            DO UPDATE SET
                buyin_total = EXCLUDED.buyin_total,
                buyin_prize_component = EXCLUDED.buyin_prize_component,
                buyin_bounty_component = EXCLUDED.buyin_bounty_component,
                fee_component = EXCLUDED.fee_component,
                currency = EXCLUDED.currency,
                max_players = EXCLUDED.max_players,
                started_at = COALESCE(EXCLUDED.started_at, core.tournaments.started_at),
                started_at_raw = EXCLUDED.started_at_raw,
                started_at_local = EXCLUDED.started_at_local,
                started_at_tz_provenance = EXCLUDED.started_at_tz_provenance,
                source_summary_file_id = COALESCE(EXCLUDED.source_summary_file_id, core.tournaments.source_summary_file_id)
            RETURNING id",
            &[
                &context.organization_id,
                &context.player_profile_id,
                &context.room_id,
                &context.format_id,
                &summary.tournament_id.to_string(),
                &cents_to_f64(summary.buy_in_cents + summary.rake_cents + summary.bounty_cents),
                &cents_to_f64(summary.buy_in_cents),
                &cents_to_f64(summary.bounty_cents),
                &cents_to_f64(summary.rake_cents),
                &(summary.entrants as i32),
                &summary.started_at,
                &source_file_id,
            ],
        )?
        .get(0);

    tx.execute(
        "INSERT INTO core.tournament_entries (
            tournament_id,
            player_profile_id,
            finish_place,
            regular_prize_money,
            total_payout_money,
            mystery_money_total,
            is_winner
        )
        VALUES (
            $1,
            $2,
            $3,
            ($4::double precision)::numeric(12,2),
            ($5::double precision)::numeric(12,2),
            ($6::double precision)::numeric(12,2),
            $7
        )
        ON CONFLICT (tournament_id, player_profile_id)
        DO UPDATE SET
            finish_place = EXCLUDED.finish_place,
            regular_prize_money = EXCLUDED.regular_prize_money,
            total_payout_money = EXCLUDED.total_payout_money,
            mystery_money_total = EXCLUDED.mystery_money_total,
            is_winner = EXCLUDED.is_winner",
        &[
            &tournament_id,
            &context.player_profile_id,
            &(summary.finish_place as i32),
            &cents_to_f64(tournament_entry_economics.regular_prize_cents),
            &cents_to_f64(summary.payout_cents),
            &cents_to_f64(tournament_entry_economics.mystery_money_cents),
            &(summary.finish_place == 1),
        ],
    )?;

    Ok(LocalImportReport {
        file_kind: "ts",
        source_file_id,
        import_job_id,
        tournament_id,
        fragments_persisted: 1,
        hands_persisted: 0,
    })
}

fn import_hand_history(
    tx: &mut Transaction<'_>,
    context: &DevContext,
    path: &str,
    input: &str,
) -> Result<LocalImportReport> {
    let hands = split_hand_history(input)?;
    let canonical_hands = hands
        .iter()
        .map(|hand| parse_canonical_hand(&hand.raw_text))
        .collect::<Result<Vec<_>, _>>()?;
    let normalized_hands = canonical_hands
        .iter()
        .map(normalize_hand)
        .collect::<Result<Vec<_>, _>>()?;
    let first_hand = hands
        .first()
        .ok_or_else(|| anyhow!("hand history contains no parsed hands"))?;

    let tournament_id: Uuid = tx
        .query_opt(
            "SELECT id
             FROM core.tournaments
             WHERE player_profile_id = $1
               AND room_id = $2
               AND external_tournament_id = $3",
            &[
                &context.player_profile_id,
                &context.room_id,
                &first_hand.header.tournament_id.to_string(),
            ],
        )?
        .map(|row| row.get(0))
        .ok_or_else(|| {
            anyhow!(
                "tournament {} is missing in core.tournaments; import the matching TS file first",
                first_hand.header.tournament_id
            )
        })?;

    let source_file_id = insert_source_file(tx, context, path, input, "hh")?;
    insert_source_file_member(tx, source_file_id, path, "hh", input)?;
    let import_job_id = insert_import_job(tx, context.organization_id, source_file_id)?;
    insert_job_attempt(tx, import_job_id)?;
    let stage_facts = canonical_hands
        .iter()
        .zip(normalized_hands.iter())
        .map(|(hand, normalized_hand)| StageHandFact {
            hand_id: hand.header.hand_id.clone(),
            played_at: hand.header.played_at.clone(),
            max_players: hand.header.max_players,
            seat_count: hand.seats.len(),
            exact_hero_boundary_ko_share: Some(
                normalized_hand
                    .eliminations
                    .iter()
                    .filter(|elimination| elimination.certainty_state == CertaintyState::Exact)
                    .map(|elimination| elimination.hero_share_fraction.unwrap_or(0.0))
                    .sum::<f64>(),
            ),
        })
        .collect::<Vec<_>>();
    let mbr_stage_resolutions =
        build_mbr_stage_resolutions_from_facts(context.player_profile_id, &stage_facts);

    for (index, hand) in hands.iter().enumerate() {
        let fragment_id = insert_file_fragment(
            tx,
            source_file_id,
            index as i32,
            Some(hand.header.hand_id.as_str()),
            "hand",
            &hand.raw_text,
        )?;
        let canonical_hand = &canonical_hands[index];
        let hand_id = upsert_hand_row(
            tx,
            context,
            tournament_id,
            source_file_id,
            fragment_id,
            canonical_hand,
        )?;
        persist_canonical_hand(tx, context, source_file_id, fragment_id, hand_id, canonical_hand)?;
        let normalized_hand = &normalized_hands[index];
        persist_normalized_hand(tx, hand_id, &normalized_hand)?;
        let street_strength_rows = build_street_hand_strength_rows(canonical_hand)?;
        persist_street_hand_strength(tx, hand_id, &street_strength_rows)?;
        let mbr_stage_resolution = mbr_stage_resolutions
            .get(&canonical_hand.header.hand_id)
            .ok_or_else(|| {
                anyhow!(
                    "missing mbr stage resolution for hand {}",
                    canonical_hand.header.hand_id
                )
            })?;
        persist_mbr_stage_resolution(tx, hand_id, mbr_stage_resolution)?;
    }

    Ok(LocalImportReport {
        file_kind: "hh",
        source_file_id,
        import_job_id,
        tournament_id,
        fragments_persisted: hands.len(),
        hands_persisted: hands.len(),
    })
}

fn upsert_hand_row(
    tx: &mut Transaction<'_>,
    context: &DevContext,
    tournament_id: Uuid,
    source_file_id: Uuid,
    fragment_id: Uuid,
    hand: &CanonicalParsedHand,
) -> Result<Uuid> {
    Ok(tx
        .query_one(
            "INSERT INTO core.hands (
                organization_id,
                player_profile_id,
                tournament_id,
                source_file_id,
                external_hand_id,
                hand_started_at,
                hand_started_at_raw,
                hand_started_at_local,
                hand_started_at_tz_provenance,
                table_name,
                table_max_seats,
                dealer_seat_no,
                small_blind,
                big_blind,
                ante,
                currency,
                raw_fragment_id
            )
            VALUES (
                $1,
                $2,
                $3,
                $4,
                $5,
                NULL,
                $6,
                replace($6, '/', '-')::timestamp,
                'gg_text_without_timezone',
                $7,
                $8,
                $9,
                $10,
                $11,
                $12,
                'USD',
                $13
            )
            ON CONFLICT (player_profile_id, external_hand_id)
            DO UPDATE SET
                tournament_id = EXCLUDED.tournament_id,
                source_file_id = EXCLUDED.source_file_id,
                hand_started_at = EXCLUDED.hand_started_at,
                hand_started_at_raw = EXCLUDED.hand_started_at_raw,
                hand_started_at_local = EXCLUDED.hand_started_at_local,
                hand_started_at_tz_provenance = EXCLUDED.hand_started_at_tz_provenance,
                table_name = EXCLUDED.table_name,
                table_max_seats = EXCLUDED.table_max_seats,
                dealer_seat_no = EXCLUDED.dealer_seat_no,
                small_blind = EXCLUDED.small_blind,
                big_blind = EXCLUDED.big_blind,
                ante = EXCLUDED.ante,
                currency = EXCLUDED.currency,
                raw_fragment_id = EXCLUDED.raw_fragment_id
            RETURNING id",
            &[
                &context.organization_id,
                &context.player_profile_id,
                &tournament_id,
                &source_file_id,
                &hand.header.hand_id,
                &hand.header.played_at,
                &hand.header.table_name,
                &(hand.header.max_players as i32),
                &(hand.header.button_seat as i32),
                &(hand.header.small_blind as i64),
                &(hand.header.big_blind as i64),
                &(hand.header.ante as i64),
                &fragment_id,
            ],
        )?
        .get(0))
}

fn persist_canonical_hand(
    tx: &mut Transaction<'_>,
    context: &DevContext,
    source_file_id: Uuid,
    fragment_id: Uuid,
    hand_id: Uuid,
    hand: &CanonicalParsedHand,
) -> Result<()> {
    let rows = build_canonical_persistence(hand);
    replace_hand_children(tx, source_file_id, fragment_id, hand_id)?;

    for seat in &rows.seats {
        tx.execute(
            "INSERT INTO core.hand_seats (
                hand_id,
                seat_no,
                player_name,
                player_profile_id,
                starting_stack,
                is_hero,
                is_button
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &hand_id,
                &seat.seat_no,
                &seat.player_name,
                &context
                    .player_aliases
                    .iter()
                    .any(|alias| alias == &seat.player_name)
                    .then_some(context.player_profile_id),
                &seat.starting_stack,
                &seat.is_hero,
                &seat.is_button,
            ],
        )?;
    }

    for hole_cards in &rows.hole_cards {
        tx.execute(
            "INSERT INTO core.hand_hole_cards (
                hand_id,
                seat_no,
                card1,
                card2,
                known_to_hero,
                known_at_showdown
            )
            VALUES ($1, $2, $3, $4, $5, $6)",
            &[
                &hand_id,
                &hole_cards.seat_no,
                &hole_cards.card1,
                &hole_cards.card2,
                &hole_cards.known_to_hero,
                &hole_cards.known_at_showdown,
            ],
        )?;
    }

    for action in &rows.actions {
        tx.execute(
            "INSERT INTO core.hand_actions (
                hand_id,
                sequence_no,
                street,
                seat_no,
                action_type,
                raw_amount,
                to_amount,
                is_all_in,
                references_previous_bet,
                raw_line
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            &[
                &hand_id,
                &action.sequence_no,
                &action.street,
                &action.seat_no,
                &action.action_type,
                &action.raw_amount,
                &action.to_amount,
                &action.is_all_in,
                &action.references_previous_bet,
                &action.raw_line,
            ],
        )?;
    }

    if let Some(board) = &rows.board {
        tx.execute(
            "INSERT INTO core.hand_boards (
                hand_id,
                flop1,
                flop2,
                flop3,
                turn,
                river
            )
            VALUES ($1, $2, $3, $4, $5, $6)",
            &[
                &hand_id,
                &board.flop1,
                &board.flop2,
                &board.flop3,
                &board.turn,
                &board.river,
            ],
        )?;
    }

    for showdown in &rows.showdowns {
        tx.execute(
            "INSERT INTO core.hand_showdowns (
                hand_id,
                seat_no,
                shown_cards
            )
            VALUES ($1, $2, $3)",
            &[&hand_id, &showdown.seat_no, &showdown.shown_cards],
        )?;
    }

    for issue in &rows.parse_issues {
        tx.execute(
            "INSERT INTO core.parse_issues (
                source_file_id,
                fragment_id,
                hand_id,
                severity,
                code,
                message,
                raw_line
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)",
            &[
                &source_file_id,
                &fragment_id,
                &hand_id,
                &issue.severity,
                &issue.code,
                &issue.message,
                &issue.raw_line,
            ],
        )?;
    }

    Ok(())
}

fn persist_normalized_hand(
    tx: &mut Transaction<'_>,
    hand_id: Uuid,
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Result<()> {
    let row = build_hand_state_resolution(normalized_hand);
    let pot_rows = build_hand_pot_rows(normalized_hand);
    let contribution_rows = build_hand_pot_contribution_rows(normalized_hand);
    let winner_rows = build_hand_pot_winner_rows(normalized_hand);
    let return_rows = build_hand_return_rows(normalized_hand);
    let elimination_rows = build_hand_elimination_rows(normalized_hand);
    let final_stacks_json = serde_json::to_string(&row.final_stacks)?;
    let invariant_errors_json = serde_json::to_string(&row.invariant_errors)?;

    tx.execute(
        "INSERT INTO derived.hand_state_resolutions (
            hand_id,
            resolution_version,
            chip_conservation_ok,
            pot_conservation_ok,
            rake_amount,
            final_stacks,
            invariant_errors
        )
        VALUES ($1, $2, $3, $4, $5, ($6::text)::jsonb, ($7::text)::jsonb)
        ON CONFLICT (hand_id, resolution_version)
        DO UPDATE SET
            chip_conservation_ok = EXCLUDED.chip_conservation_ok,
            pot_conservation_ok = EXCLUDED.pot_conservation_ok,
            rake_amount = EXCLUDED.rake_amount,
            final_stacks = EXCLUDED.final_stacks,
            invariant_errors = EXCLUDED.invariant_errors",
        &[
            &hand_id,
            &row.resolution_version,
            &row.chip_conservation_ok,
            &row.pot_conservation_ok,
            &row.rake_amount,
            &final_stacks_json,
            &invariant_errors_json,
        ],
    )?;

    for pot_row in pot_rows {
        tx.execute(
            "INSERT INTO core.hand_pots (
                hand_id,
                pot_no,
                pot_type,
                amount
            )
            VALUES ($1, $2, $3, $4)",
            &[
                &hand_id,
                &pot_row.pot_no,
                &pot_row.pot_type,
                &pot_row.amount,
            ],
        )?;
    }

    for contribution_row in contribution_rows {
        tx.execute(
            "INSERT INTO core.hand_pot_contributions (
                hand_id,
                pot_no,
                seat_no,
                amount
            )
            VALUES ($1, $2, $3, $4)",
            &[
                &hand_id,
                &contribution_row.pot_no,
                &contribution_row.seat_no,
                &contribution_row.amount,
            ],
        )?;
    }

    for winner_row in winner_rows {
        tx.execute(
            "INSERT INTO core.hand_pot_winners (
                hand_id,
                pot_no,
                seat_no,
                share_amount
            )
            VALUES ($1, $2, $3, $4)",
            &[
                &hand_id,
                &winner_row.pot_no,
                &winner_row.seat_no,
                &winner_row.share_amount,
            ],
        )?;
    }

    for return_row in return_rows {
        tx.execute(
            "INSERT INTO core.hand_returns (
                hand_id,
                seat_no,
                amount,
                reason
            )
            VALUES ($1, $2, $3, $4)",
            &[
                &hand_id,
                &return_row.seat_no,
                &return_row.amount,
                &return_row.reason,
            ],
        )?;
    }

    tx.execute(
        "DELETE FROM derived.hand_eliminations WHERE hand_id = $1",
        &[&hand_id],
    )?;

    for elimination_row in elimination_rows {
        tx.execute(
            "INSERT INTO derived.hand_eliminations (
                hand_id,
                eliminated_seat_no,
                eliminated_player_name,
                resolved_by_pot_no,
                ko_involved_winner_count,
                hero_involved,
                hero_share_fraction,
                is_split_ko,
                split_n,
                is_sidepot_based,
                certainty_state
            )
            VALUES ($1, $2, $3, $4, $5, $6, ($7::text)::numeric(12,6), $8, $9, $10, $11)",
            &[
                &hand_id,
                &elimination_row.eliminated_seat_no,
                &elimination_row.eliminated_player_name,
                &elimination_row.resolved_by_pot_no,
                &elimination_row.ko_involved_winner_count,
                &elimination_row.hero_involved,
                &elimination_row.hero_share_fraction,
                &elimination_row.is_split_ko,
                &elimination_row.split_n,
                &elimination_row.is_sidepot_based,
                &elimination_row.certainty_state,
            ],
        )?;
    }

    Ok(())
}

fn persist_mbr_stage_resolution(
    tx: &mut Transaction<'_>,
    hand_id: Uuid,
    row: &MbrStageResolutionRow,
) -> Result<()> {
    tx.execute(
        "INSERT INTO derived.mbr_stage_resolution (
            hand_id,
            player_profile_id,
            played_ft_hand,
            played_ft_hand_state,
            entered_boundary_zone,
            entered_boundary_zone_state,
            ft_table_size,
            boundary_ko_ev,
            boundary_ko_min,
            boundary_ko_max,
            boundary_ko_method,
            boundary_ko_certainty,
            boundary_ko_state
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7,
            ($8::text)::numeric(12,6),
            ($9::text)::numeric(12,6),
            ($10::text)::numeric(12,6),
            $11, $12, $13
        )
        ON CONFLICT (hand_id, player_profile_id)
        DO UPDATE SET
            played_ft_hand = EXCLUDED.played_ft_hand,
            played_ft_hand_state = EXCLUDED.played_ft_hand_state,
            entered_boundary_zone = EXCLUDED.entered_boundary_zone,
            entered_boundary_zone_state = EXCLUDED.entered_boundary_zone_state,
            ft_table_size = EXCLUDED.ft_table_size,
            boundary_ko_ev = EXCLUDED.boundary_ko_ev,
            boundary_ko_min = EXCLUDED.boundary_ko_min,
            boundary_ko_max = EXCLUDED.boundary_ko_max,
            boundary_ko_method = EXCLUDED.boundary_ko_method,
            boundary_ko_certainty = EXCLUDED.boundary_ko_certainty,
            boundary_ko_state = EXCLUDED.boundary_ko_state",
        &[
            &hand_id,
            &row.player_profile_id,
            &row.played_ft_hand,
            &row.played_ft_hand_state,
            &row.entered_boundary_zone,
            &row.entered_boundary_zone_state,
            &row.ft_table_size,
            &row.boundary_ko_ev,
            &row.boundary_ko_min,
            &row.boundary_ko_max,
            &row.boundary_ko_method,
            &row.boundary_ko_certainty,
            &row.boundary_ko_state,
        ],
    )?;

    Ok(())
}

fn persist_street_hand_strength(
    tx: &mut Transaction<'_>,
    hand_id: Uuid,
    rows: &[StreetHandStrengthRow],
) -> Result<()> {
    tx.execute(
        "DELETE FROM derived.street_hand_strength
         WHERE hand_id = $1
           AND descriptor_version = $2",
        &[&hand_id, &STREET_HAND_STRENGTH_VERSION],
    )?;

    for row in rows {
        tx.execute(
            "INSERT INTO derived.street_hand_strength (
                hand_id,
                seat_no,
                street,
                best_hand_class,
                best_hand_rank_value,
                pair_strength,
                is_nut_hand,
                is_nut_draw,
                has_flush_draw,
                has_backdoor_flush_draw,
                has_open_ended,
                has_gutshot,
                has_double_gutshot,
                has_pair_plus_draw,
                has_overcards,
                has_air,
                has_missed_draw_by_river,
                descriptor_version,
                certainty_state
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14, $15, $16, $17, $18, $19
            )",
            &[
                &hand_id,
                &row.seat_no,
                &row.street,
                &row.best_hand_class,
                &row.best_hand_rank_value,
                &row.pair_strength,
                &row.is_nut_hand,
                &row.is_nut_draw,
                &row.has_flush_draw,
                &row.has_backdoor_flush_draw,
                &row.has_open_ended,
                &row.has_gutshot,
                &row.has_double_gutshot,
                &row.has_pair_plus_draw,
                &row.has_overcards,
                &row.has_air,
                &row.has_missed_draw_by_river,
                &row.descriptor_version,
                &row.certainty_state,
            ],
        )?;
    }

    Ok(())
}

fn replace_hand_children(
    tx: &mut Transaction<'_>,
    source_file_id: Uuid,
    fragment_id: Uuid,
    hand_id: Uuid,
) -> Result<()> {
    tx.execute(
        "DELETE FROM core.parse_issues WHERE source_file_id = $1 AND fragment_id = $2",
        &[&source_file_id, &fragment_id],
    )?;
    tx.execute(
        "DELETE FROM core.hand_showdowns WHERE hand_id = $1",
        &[&hand_id],
    )?;
    tx.execute(
        "DELETE FROM core.hand_hole_cards WHERE hand_id = $1",
        &[&hand_id],
    )?;
    tx.execute(
        "DELETE FROM core.hand_actions WHERE hand_id = $1",
        &[&hand_id],
    )?;
    tx.execute(
        "DELETE FROM core.hand_returns WHERE hand_id = $1",
        &[&hand_id],
    )?;
    tx.execute(
        "DELETE FROM core.hand_pot_winners WHERE hand_id = $1",
        &[&hand_id],
    )?;
    tx.execute(
        "DELETE FROM core.hand_pot_contributions WHERE hand_id = $1",
        &[&hand_id],
    )?;
    tx.execute("DELETE FROM core.hand_pots WHERE hand_id = $1", &[&hand_id])?;
    tx.execute(
        "DELETE FROM core.hand_boards WHERE hand_id = $1",
        &[&hand_id],
    )?;
    tx.execute(
        "DELETE FROM core.hand_seats WHERE hand_id = $1",
        &[&hand_id],
    )?;
    Ok(())
}

fn build_hand_state_resolution(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> HandStateResolutionRow {
    HandStateResolutionRow {
        resolution_version: HAND_RESOLUTION_VERSION.to_string(),
        chip_conservation_ok: normalized_hand.invariants.chip_conservation_ok,
        pot_conservation_ok: normalized_hand.invariants.pot_conservation_ok,
        rake_amount: normalized_hand.actual.rake_amount,
        final_stacks: normalized_hand.actual.stacks_after_actual.clone(),
        invariant_errors: normalized_hand.invariants.invariant_errors.clone(),
    }
}

fn build_hand_pot_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandPotRow> {
    normalized_hand
        .final_pots
        .iter()
        .map(|pot| HandPotRow {
            pot_no: i32::from(pot.pot_no),
            pot_type: if pot.is_main {
                "main".to_string()
            } else {
                "side".to_string()
            },
            amount: pot.amount,
        })
        .collect()
}

fn build_street_hand_strength_rows(hand: &CanonicalParsedHand) -> Result<Vec<StreetHandStrengthRow>> {
    Ok(evaluate_street_hand_strength(hand)?
        .into_iter()
        .map(|descriptor| StreetHandStrengthRow {
            seat_no: descriptor.seat_no as i32,
            street: street_code(descriptor.street).to_string(),
            best_hand_class: descriptor.best_hand_class.as_str().to_string(),
            best_hand_rank_value: descriptor.best_hand_rank_value,
            pair_strength: descriptor.pair_strength.as_str().to_string(),
            is_nut_hand: descriptor.is_nut_hand,
            is_nut_draw: descriptor.is_nut_draw,
            has_flush_draw: descriptor.has_flush_draw,
            has_backdoor_flush_draw: descriptor.has_backdoor_flush_draw,
            has_open_ended: descriptor.has_open_ended,
            has_gutshot: descriptor.has_gutshot,
            has_double_gutshot: descriptor.has_double_gutshot,
            has_pair_plus_draw: descriptor.has_pair_plus_draw,
            has_overcards: descriptor.has_overcards,
            has_air: descriptor.has_air,
            has_missed_draw_by_river: descriptor.has_missed_draw_by_river,
            descriptor_version: descriptor.descriptor_version.to_string(),
            certainty_state: certainty_state_code(descriptor.certainty_state).to_string(),
        })
        .collect())
}

fn build_hand_pot_contribution_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandPotContributionRow> {
    normalized_hand
        .pot_contributions
        .iter()
        .map(|contribution| HandPotContributionRow {
            pot_no: i32::from(contribution.pot_no),
            seat_no: i32::from(contribution.seat_no),
            amount: contribution.amount,
        })
        .collect()
}

fn build_hand_pot_winner_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandPotWinnerRow> {
    normalized_hand
        .pot_winners
        .iter()
        .map(|winner| HandPotWinnerRow {
            pot_no: i32::from(winner.pot_no),
            seat_no: i32::from(winner.seat_no),
            share_amount: winner.share_amount,
        })
        .collect()
}

fn build_hand_return_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandReturnRow> {
    normalized_hand
        .returns
        .iter()
        .map(|hand_return| HandReturnRow {
            seat_no: i32::from(hand_return.seat_no),
            amount: hand_return.amount,
            reason: hand_return.reason.clone(),
        })
        .collect()
}

fn build_hand_elimination_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandEliminationRow> {
    normalized_hand
        .eliminations
        .iter()
        .map(|elimination| HandEliminationRow {
            eliminated_seat_no: elimination.eliminated_seat_no as i32,
            eliminated_player_name: elimination.eliminated_player_name.clone(),
            resolved_by_pot_no: elimination.resolved_by_pot_no.map(i32::from),
            ko_involved_winner_count: i32::from(elimination.ko_involved_winner_count),
            hero_involved: elimination.hero_involved,
            hero_share_fraction: elimination
                .hero_share_fraction
                .map(|fraction| format!("{fraction:.6}")),
            is_split_ko: elimination.is_split_ko,
            split_n: elimination.split_n.map(i32::from),
            is_sidepot_based: elimination.is_sidepot_based,
            certainty_state: certainty_state_code(elimination.certainty_state).to_string(),
        })
        .collect()
}

fn build_mbr_stage_resolutions(
    player_profile_id: Uuid,
    hands: &[CanonicalParsedHand],
) -> BTreeMap<String, MbrStageResolutionRow> {
    let facts = hands
        .iter()
        .map(|hand| StageHandFact {
            hand_id: hand.header.hand_id.clone(),
            played_at: hand.header.played_at.clone(),
            max_players: hand.header.max_players,
            seat_count: hand.seats.len(),
            exact_hero_boundary_ko_share: Some(0.0),
        })
        .collect::<Vec<_>>();

    build_mbr_stage_resolutions_from_facts(player_profile_id, &facts)
}

fn build_mbr_stage_resolutions_from_facts(
    player_profile_id: Uuid,
    facts: &[StageHandFact],
) -> BTreeMap<String, MbrStageResolutionRow> {
    let mut chronological = facts.iter().collect::<Vec<_>>();
    chronological.sort_by(|left, right| left.played_at.cmp(&right.played_at));

    let first_ft_index = chronological.iter().position(|hand| hand.max_players == 9);
    let boundary_hand_id = first_ft_index.and_then(|index| {
        chronological[..index]
            .iter()
            .rev()
            .find(|hand| hand.max_players == 5)
            .map(|hand| hand.hand_id.clone())
    });

    facts
        .iter()
        .map(|fact| {
            let played_ft_hand = fact.max_players == 9;
            let is_boundary_hand = boundary_hand_id.as_deref() == Some(fact.hand_id.as_str());
            let boundary_ko_value = is_boundary_hand.then_some(
                fact.exact_hero_boundary_ko_share.unwrap_or_default(),
            );

            (
                fact.hand_id.clone(),
                MbrStageResolutionRow {
                    player_profile_id,
                    played_ft_hand,
                    played_ft_hand_state: "exact".to_string(),
                    entered_boundary_zone: is_boundary_hand,
                    entered_boundary_zone_state: if is_boundary_hand {
                        "estimated".to_string()
                    } else {
                        "exact".to_string()
                    },
                    ft_table_size: played_ft_hand.then_some(fact.seat_count as i32),
                    boundary_ko_ev: boundary_ko_value.map(|value| format!("{value:.6}")),
                    boundary_ko_min: boundary_ko_value.map(|value| format!("{value:.6}")),
                    boundary_ko_max: boundary_ko_value.map(|value| format!("{value:.6}")),
                    boundary_ko_method: is_boundary_hand
                        .then_some("legacy_pre_ft_candidate_v1".to_string()),
                    boundary_ko_certainty: is_boundary_hand.then_some("estimated".to_string()),
                    boundary_ko_state: if is_boundary_hand {
                        "estimated".to_string()
                    } else {
                        "uncertain".to_string()
                    },
                },
            )
        })
        .collect()
}

fn load_tournament_entry_economics(
    tx: &mut Transaction<'_>,
    context: &DevContext,
    summary: &TournamentSummary,
) -> Result<TournamentEntryEconomics> {
    let regular_prize_cents: i64 = tx
        .query_opt(
            "SELECT COALESCE((prize.regular_prize_money * 100)::bigint, 0::bigint)
             FROM ref.mbr_buyin_configs config
             LEFT JOIN ref.mbr_regular_prizes prize
               ON prize.buyin_config_id = config.id
              AND prize.finish_place = $5
             WHERE config.room_id = $1
               AND config.format_id = $2
               AND config.buyin_total = ($3::double precision)::numeric(12,2)
               AND config.currency = $4
               AND config.max_players = $6",
            &[
                &context.room_id,
                &context.format_id,
                &cents_to_f64(summary.buy_in_cents + summary.rake_cents + summary.bounty_cents),
                &"USD",
                &(summary.finish_place as i32),
                &(summary.entrants as i32),
            ],
        )?
        .map(|row| row.get(0))
        .ok_or_else(|| {
            anyhow!(
                "missing MBR buy-in config for buyin_total={}, entrants={}",
                summary.buy_in_cents + summary.rake_cents + summary.bounty_cents,
                summary.entrants
            )
        })?;

    resolve_tournament_entry_economics(summary, regular_prize_cents)
}

fn resolve_tournament_entry_economics(
    summary: &TournamentSummary,
    regular_prize_cents: i64,
) -> Result<TournamentEntryEconomics> {
    let mystery_money_cents = summary.payout_cents - regular_prize_cents;
    if mystery_money_cents < 0 {
        return Err(anyhow!(
            "mystery_money_total cannot be negative: payout_cents={}, regular_prize_cents={}",
            summary.payout_cents,
            regular_prize_cents
        ));
    }

    Ok(TournamentEntryEconomics {
        regular_prize_cents,
        mystery_money_cents,
    })
}

fn build_canonical_persistence(hand: &CanonicalParsedHand) -> CanonicalHandPersistence {
    let mut seat_lookup = BTreeMap::new();
    let mut seats = Vec::new();
    for seat in &hand.seats {
        seat_lookup.insert(seat.player_name.clone(), seat.seat_no);
        seats.push(HandSeatRow {
            seat_no: i32::from(seat.seat_no),
            player_name: seat.player_name.clone(),
            starting_stack: seat.starting_stack,
            is_hero: hand.hero_name.as_deref() == Some(seat.player_name.as_str()),
            is_button: seat.seat_no == hand.header.button_seat,
        });
    }

    let mut parse_issues = hand
        .parse_warnings
        .iter()
        .map(|warning| parse_warning_to_issue(warning))
        .collect::<Vec<_>>();

    let mut hole_cards_by_seat = BTreeMap::new();
    if let (Some(hero_name), Some(hero_cards)) = (&hand.hero_name, &hand.hero_hole_cards) {
        match seat_lookup.get(hero_name) {
            Some(seat_no) => upsert_hole_cards(
                &mut hole_cards_by_seat,
                *seat_no,
                hero_cards,
                true,
                hand.showdown_hands.contains_key(hero_name),
            ),
            None => parse_issues.push(error_parse_issue(
                "hero_cards_missing_seat",
                format!("hero hole cards exist but hero `{hero_name}` has no seat row"),
                None,
            )),
        }
    }

    let mut showdowns = Vec::new();
    for (player_name, shown_cards) in &hand.showdown_hands {
        match seat_lookup.get(player_name) {
            Some(seat_no) => {
                upsert_hole_cards(&mut hole_cards_by_seat, *seat_no, shown_cards, false, true);
                showdowns.push(HandShowdownRow {
                    seat_no: i32::from(*seat_no),
                    shown_cards: shown_cards.clone(),
                });
            }
            None => parse_issues.push(error_parse_issue(
                "showdown_player_missing_seat",
                format!("showdown hand exists for `{player_name}` without seat row"),
                None,
            )),
        }
    }

    let mut actions = Vec::new();
    for event in &hand.actions {
        let seat_no = event
            .player_name
            .as_ref()
            .and_then(|player_name| seat_lookup.get(player_name).copied());

        if let Some(player_name) = &event.player_name
            && seat_no.is_none()
        {
            parse_issues.push(error_parse_issue(
                "action_player_missing_seat",
                format!("action references `{player_name}` without seat row"),
                Some(event.raw_line.clone()),
            ));
        }

        actions.push(HandActionRow {
            sequence_no: event.seq as i32,
            street: street_code(event.street).to_string(),
            seat_no: seat_no.map(i32::from),
            action_type: action_code(event.action_type).to_string(),
            raw_amount: event.amount,
            to_amount: event.to_amount,
            is_all_in: event.is_all_in,
            references_previous_bet: matches!(
                event.action_type,
                ActionType::Call | ActionType::RaiseTo
            ),
            raw_line: event.raw_line.clone(),
        });
    }

    CanonicalHandPersistence {
        seats,
        hole_cards: hole_cards_by_seat.into_values().collect(),
        actions,
        board: build_board_row(&hand.board_final),
        showdowns,
        parse_issues,
    }
}

fn upsert_hole_cards(
    map: &mut BTreeMap<u8, HandHoleCardsRow>,
    seat_no: u8,
    cards: &[String],
    known_to_hero: bool,
    known_at_showdown: bool,
) {
    let entry = map.entry(seat_no).or_insert_with(|| HandHoleCardsRow {
        seat_no: i32::from(seat_no),
        card1: cards.first().cloned(),
        card2: cards.get(1).cloned(),
        known_to_hero: false,
        known_at_showdown: false,
    });

    if entry.card1.is_none() {
        entry.card1 = cards.first().cloned();
    }
    if entry.card2.is_none() {
        entry.card2 = cards.get(1).cloned();
    }
    entry.known_to_hero |= known_to_hero;
    entry.known_at_showdown |= known_at_showdown;
}

fn build_board_row(cards: &[String]) -> Option<HandBoardRow> {
    if cards.is_empty() {
        return None;
    }

    Some(HandBoardRow {
        flop1: cards.first().cloned(),
        flop2: cards.get(1).cloned(),
        flop3: cards.get(2).cloned(),
        turn: cards.get(3).cloned(),
        river: cards.get(4).cloned(),
    })
}

fn parse_warning_to_issue(warning: &str) -> ParseIssueRow {
    if let Some(raw_line) = warning.strip_prefix("unparsed_line: ") {
        warning_parse_issue(
            "unparsed_line",
            warning.to_string(),
            Some(raw_line.to_string()),
        )
    } else {
        warning_parse_issue("parser_warning", warning.to_string(), None)
    }
}

fn warning_parse_issue(code: &str, message: String, raw_line: Option<String>) -> ParseIssueRow {
    ParseIssueRow {
        severity: "warning".to_string(),
        code: code.to_string(),
        message,
        raw_line,
    }
}

fn error_parse_issue(code: &str, message: String, raw_line: Option<String>) -> ParseIssueRow {
    ParseIssueRow {
        severity: "error".to_string(),
        code: code.to_string(),
        message,
        raw_line,
    }
}

fn street_code(street: Street) -> &'static str {
    match street {
        Street::Preflop => "preflop",
        Street::Flop => "flop",
        Street::Turn => "turn",
        Street::River => "river",
        Street::Showdown => "showdown",
        Street::Summary => "summary",
    }
}

fn action_code(action_type: ActionType) -> &'static str {
    match action_type {
        ActionType::PostAnte => "post_ante",
        ActionType::PostSb => "post_sb",
        ActionType::PostBb => "post_bb",
        ActionType::PostDead => "post_dead",
        ActionType::Fold => "fold",
        ActionType::Check => "check",
        ActionType::Call => "call",
        ActionType::Bet => "bet",
        ActionType::RaiseTo => "raise_to",
        ActionType::ReturnUncalled => "return_uncalled",
        ActionType::Collect => "collect",
        ActionType::Show => "show",
        ActionType::Muck => "muck",
    }
}

fn certainty_state_code(state: tracker_parser_core::models::CertaintyState) -> &'static str {
    match state {
        tracker_parser_core::models::CertaintyState::Exact => "exact",
        tracker_parser_core::models::CertaintyState::Estimated => "estimated",
        tracker_parser_core::models::CertaintyState::Uncertain => "uncertain",
        tracker_parser_core::models::CertaintyState::Inconsistent => "inconsistent",
    }
}

fn insert_source_file(
    tx: &mut Transaction<'_>,
    context: &DevContext,
    path: &str,
    input: &str,
    file_kind: &str,
) -> Result<Uuid> {
    let filename = source_filename(path)?;
    let storage_uri = format!("local://{}", path.replace('\\', "/"));
    let sha256 = sha256_hex(input);

    Ok(tx
        .query_one(
            "INSERT INTO import.source_files (
                organization_id,
                uploaded_by_user_id,
                owner_user_id,
                player_profile_id,
                room,
                file_kind,
                sha256,
                original_filename,
                byte_size,
                storage_uri
            )
            VALUES ($1, $2, $3, $4, 'gg', $5, $6, $7, $8, $9)
            ON CONFLICT (player_profile_id, room, file_kind, sha256)
            DO UPDATE SET
                organization_id = EXCLUDED.organization_id,
                uploaded_by_user_id = EXCLUDED.uploaded_by_user_id,
                owner_user_id = EXCLUDED.owner_user_id,
                original_filename = EXCLUDED.original_filename,
                byte_size = EXCLUDED.byte_size,
                storage_uri = EXCLUDED.storage_uri
            RETURNING id",
            &[
                &context.organization_id,
                &context.user_id,
                &context.user_id,
                &context.player_profile_id,
                &file_kind,
                &sha256,
                &filename,
                &(input.len() as i64),
                &storage_uri,
            ],
        )?
        .get(0))
}

fn insert_source_file_member(
    tx: &mut Transaction<'_>,
    source_file_id: Uuid,
    path: &str,
    member_kind: &str,
    input: &str,
) -> Result<Uuid> {
    let member_path = source_filename(path)?;
    let sha256 = sha256_hex(input);

    Ok(tx
        .query_one(
            "INSERT INTO import.source_file_members (
                source_file_id,
                member_index,
                member_path,
                member_kind,
                sha256,
                byte_size
            )
            VALUES ($1, 0, $2, $3, $4, $5)
            ON CONFLICT (source_file_id, member_index)
            DO UPDATE SET
                member_path = EXCLUDED.member_path,
                member_kind = EXCLUDED.member_kind,
                sha256 = EXCLUDED.sha256,
                byte_size = EXCLUDED.byte_size
            RETURNING id",
            &[
                &source_file_id,
                &member_path,
                &member_kind,
                &sha256,
                &(input.len() as i64),
            ],
        )?
        .get(0))
}

fn insert_import_job(
    tx: &mut Transaction<'_>,
    organization_id: Uuid,
    source_file_id: Uuid,
) -> Result<Uuid> {
    Ok(tx
        .query_one(
            "INSERT INTO import.import_jobs (
                organization_id,
                source_file_id,
                status,
                stage,
                started_at,
                finished_at
            )
            VALUES ($1, $2, 'done', 'done', now(), now())
            RETURNING id",
            &[&organization_id, &source_file_id],
        )?
        .get(0))
}

fn insert_job_attempt(tx: &mut Transaction<'_>, import_job_id: Uuid) -> Result<Uuid> {
    Ok(tx
        .query_one(
            "INSERT INTO import.job_attempts (
                import_job_id,
                attempt_no,
                status,
                stage,
                started_at,
                finished_at
            )
            VALUES ($1, 1, 'done', 'done', now(), now())
            ON CONFLICT (import_job_id, attempt_no)
            DO UPDATE SET
                status = EXCLUDED.status,
                stage = EXCLUDED.stage,
                started_at = EXCLUDED.started_at,
                finished_at = EXCLUDED.finished_at
            RETURNING id",
            &[&import_job_id],
        )?
        .get(0))
}

fn insert_file_fragment(
    tx: &mut Transaction<'_>,
    source_file_id: Uuid,
    fragment_index: i32,
    external_hand_id: Option<&str>,
    kind: &str,
    raw_text: &str,
) -> Result<Uuid> {
    let sha256 = sha256_hex(raw_text);

    Ok(tx
        .query_one(
            "INSERT INTO import.file_fragments (
                source_file_id,
                fragment_index,
                external_hand_id,
                kind,
                raw_text,
                sha256
            )
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (source_file_id, fragment_index)
            DO UPDATE SET
                external_hand_id = EXCLUDED.external_hand_id,
                kind = EXCLUDED.kind,
                raw_text = EXCLUDED.raw_text,
                sha256 = EXCLUDED.sha256
            RETURNING id",
            &[
                &source_file_id,
                &fragment_index,
                &external_hand_id,
                &kind,
                &raw_text,
                &sha256,
            ],
        )?
        .get(0))
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn source_filename(path: &str) -> Result<String> {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("failed to derive filename from `{path}`"))
}

fn cents_to_f64(cents: i64) -> f64 {
    (cents as f64) / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use mbr_stats_runtime::{SeedStatsFilters, query_seed_stats};
    use std::{
        path::PathBuf,
        sync::{Mutex, OnceLock},
    };

    const FT_HAND_ID: &str = "BR1064987693";
    const FIRST_FT_HAND_ID: &str = "BR1064986938";
    const BOUNDARY_RUSH_HAND_ID: &str = "BR1065004819";
    const EARLY_RUSH_HAND_ID: &str = "BR1065004261";
    const MULTI_COLLECT_HAND_ID: &str = "BR1064987148";
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

    fn db_test_guard() -> std::sync::MutexGuard<'static, ()> {
        static DB_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

        DB_TEST_MUTEX
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap()
    }

    #[test]
    fn builds_canonical_rows_for_ft_all_in_hand() {
        let hand_text = first_ft_hand_text();
        let hand = parse_canonical_hand(&hand_text).unwrap();

        let rows = build_canonical_persistence(&hand);

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
                },
                HandSeatRow {
                    seat_no: 7,
                    player_name: "Hero".to_string(),
                    starting_stack: 16_008,
                    is_hero: true,
                    is_button: false,
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
        hand.parse_warnings
            .push("unparsed_line: Dealer note: test-only unexpected line".to_string());
        hand.actions.push(tracker_parser_core::models::HandActionEvent {
            seq: 999,
            street: Street::Summary,
            player_name: Some("Ghost".to_string()),
            action_type: ActionType::Fold,
            is_forced: false,
            is_all_in: false,
            amount: None,
            to_amount: None,
            cards: None,
            raw_line: "Ghost: folds".to_string(),
        });

        let rows = build_canonical_persistence(&hand);

        assert!(rows.parse_issues.contains(&ParseIssueRow {
            severity: "warning".to_string(),
            code: "unparsed_line".to_string(),
            message: "unparsed_line: Dealer note: test-only unexpected line".to_string(),
            raw_line: Some("Dealer note: test-only unexpected line".to_string()),
        }));
        assert!(rows.parse_issues.contains(&ParseIssueRow {
            severity: "error".to_string(),
            code: "action_player_missing_seat".to_string(),
            message: "action references `Ghost` without seat row".to_string(),
            raw_line: Some("Ghost: folds".to_string()),
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
        assert_eq!(row.rake_amount, 0);
        assert_eq!(row.final_stacks.get("Hero"), Some(&18_000));
        assert_eq!(row.final_stacks.get("f02e54a6"), Some(&0));
        assert!(row.invariant_errors.is_empty());
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
        assert_eq!(normalized.eliminations[0].resolved_by_pot_no, Some(1));
        assert_eq!(normalized.eliminations[0].ko_involved_winner_count, 1);

        let rows = build_hand_elimination_rows(&normalized);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].resolved_by_pot_no, Some(1));
        assert!(rows[0].hero_involved);
        assert_eq!(rows[0].hero_share_fraction.as_deref(), Some("1.000000"));
        assert!(!rows[0].is_split_ko);
        assert_eq!(rows[0].split_n, Some(1));
        assert!(!rows[0].is_sidepot_based);
        assert_eq!(rows[0].certainty_state, "exact");
    }

    #[test]
    fn builds_pot_and_return_rows_for_ft_hands() {
        let ft_hand = parse_canonical_hand(&first_ft_hand_text()).unwrap();
        let ft_normalized = normalize_hand(&ft_hand).unwrap();

        let pot_rows = build_hand_pot_rows(&ft_normalized);
        let contribution_rows = build_hand_pot_contribution_rows(&ft_normalized);
        let winner_rows = build_hand_pot_winner_rows(&ft_normalized);
        let return_rows = build_hand_return_rows(&ft_normalized);

        assert_eq!(pot_rows.len(), 1);
        assert_eq!(pot_rows[0].pot_no, 1);
        assert_eq!(pot_rows[0].pot_type, "main");
        assert_eq!(pot_rows[0].amount, 3_984);
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
    fn builds_mbr_stage_resolution_for_ft_and_rush_hands() {
        let hands = all_hands_from_fixture("GG20260316-0344 - Mystery Battle Royale 25.txt");
        let rows = build_mbr_stage_resolutions(Uuid::nil(), &hands);

        let ft_row = rows.get(FIRST_FT_HAND_ID).unwrap();
        assert_eq!(ft_row.player_profile_id, Uuid::nil());
        assert!(ft_row.played_ft_hand);
        assert_eq!(ft_row.played_ft_hand_state, "exact");
        assert!(!ft_row.entered_boundary_zone);
        assert_eq!(ft_row.entered_boundary_zone_state, "exact");
        assert_eq!(ft_row.ft_table_size, Some(9));
        assert_eq!(ft_row.boundary_ko_state, "uncertain");

        let boundary_row = rows.get(BOUNDARY_RUSH_HAND_ID).unwrap();
        assert_eq!(boundary_row.player_profile_id, Uuid::nil());
        assert!(!boundary_row.played_ft_hand);
        assert_eq!(boundary_row.played_ft_hand_state, "exact");
        assert!(boundary_row.entered_boundary_zone);
        assert_eq!(boundary_row.entered_boundary_zone_state, "estimated");
        assert_eq!(boundary_row.ft_table_size, None);
        assert_eq!(boundary_row.boundary_ko_min.as_deref(), Some("0.000000"));
        assert_eq!(boundary_row.boundary_ko_ev.as_deref(), Some("0.000000"));
        assert_eq!(boundary_row.boundary_ko_max.as_deref(), Some("0.000000"));
        assert_eq!(boundary_row.boundary_ko_state, "estimated");

        let early_rush_row = rows.get(EARLY_RUSH_HAND_ID).unwrap();
        assert!(!early_rush_row.played_ft_hand);
        assert!(!early_rush_row.entered_boundary_zone);
        assert_eq!(early_rush_row.entered_boundary_zone_state, "exact");
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
        };

        let error = resolve_tournament_entry_economics(&summary, 10_000).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("mystery_money_total cannot be negative")
        );
    }

    #[test]
    fn builds_boundary_estimate_for_candidate_hand_from_exact_hero_share() {
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
        assert_eq!(boundary.entered_boundary_zone_state, "estimated");
        assert_eq!(boundary.boundary_ko_min.as_deref(), Some("0.500000"));
        assert_eq!(boundary.boundary_ko_ev.as_deref(), Some("0.500000"));
        assert_eq!(boundary.boundary_ko_max.as_deref(), Some("0.500000"));
        assert_eq!(
            boundary.boundary_ko_method.as_deref(),
            Some("legacy_pre_ft_candidate_v1")
        );
        assert_eq!(
            boundary.boundary_ko_certainty.as_deref(),
            Some("estimated")
        );
        assert_eq!(boundary.boundary_ko_state, "estimated");

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
        assert_eq!(rush.boundary_ko_state, "uncertain");
        assert!(rush.boundary_ko_ev.is_none());
        assert!(rush.boundary_ko_min.is_none());
        assert!(rush.boundary_ko_max.is_none());
    }

    #[test]
    fn selects_last_five_max_before_first_final_table_even_with_intermediate_table_sizes() {
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

        let boundary = rows.get("rush-5-max").unwrap();
        assert!(boundary.entered_boundary_zone);
        assert_eq!(boundary.entered_boundary_zone_state, "estimated");
        assert_eq!(boundary.boundary_ko_ev.as_deref(), Some("0.250000"));
        assert_eq!(boundary.boundary_ko_state, "estimated");

        let intermediate = rows.get("rush-2-max").unwrap();
        assert!(!intermediate.entered_boundary_zone);
        assert_eq!(intermediate.entered_boundary_zone_state, "exact");
        assert_eq!(intermediate.boundary_ko_state, "uncertain");
        assert!(intermediate.boundary_ko_ev.is_none());
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn migration_v0004_adds_schema_v2_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut client);
        apply_sql_file(
            &mut client,
            &fixture_path("../../migrations/0002_exact_pot_ko_core.sql"),
        );
        apply_sql_file(
            &mut client,
            &fixture_path("../../migrations/0003_mbr_stage_economics.sql"),
        );
        apply_sql_file(
            &mut client,
            &fixture_path("../../migrations/0004_exact_core_schema_v2.sql"),
        );

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
                ("analytics".to_string(), "materialization_policies".to_string()),
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
                ("core".to_string(), "hands".to_string(), "hand_started_at_local".to_string()),
                ("core".to_string(), "hands".to_string(), "hand_started_at_raw".to_string()),
                (
                    "core".to_string(),
                    "hands".to_string(),
                    "hand_started_at_tz_provenance".to_string(),
                ),
                ("core".to_string(), "tournaments".to_string(), "started_at_local".to_string()),
                ("core".to_string(), "tournaments".to_string(), "started_at_raw".to_string()),
                (
                    "core".to_string(),
                    "tournaments".to_string(),
                    "started_at_tz_provenance".to_string(),
                ),
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
        apply_sql_file(
            &mut client,
            &fixture_path("../../migrations/0002_exact_pot_ko_core.sql"),
        );
        apply_sql_file(
            &mut client,
            &fixture_path("../../migrations/0003_mbr_stage_economics.sql"),
        );
        apply_sql_file(
            &mut client,
            &fixture_path("../../migrations/0004_exact_core_schema_v2.sql"),
        );

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
        apply_sql_file(
            &mut client,
            &fixture_path("../../migrations/0002_exact_pot_ko_core.sql"),
        );
        apply_sql_file(
            &mut client,
            &fixture_path("../../migrations/0003_mbr_stage_economics.sql"),
        );
        apply_sql_file(
            &mut client,
            &fixture_path("../../migrations/0004_exact_core_schema_v2.sql"),
        );
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
                ("ft_stage_bucket".to_string(), "mbr_runtime_v1".to_string(), "enum".to_string(), "enum".to_string()),
                ("ft_table_size".to_string(), "mbr_runtime_v1".to_string(), "num".to_string(), "double".to_string()),
                ("has_exact_ko".to_string(), "mbr_runtime_v1".to_string(), "bool".to_string(), "bool".to_string()),
                ("has_sidepot_ko".to_string(), "mbr_runtime_v1".to_string(), "bool".to_string(), "bool".to_string()),
                ("has_split_ko".to_string(), "mbr_runtime_v1".to_string(), "bool".to_string(), "bool".to_string()),
                ("hero_exact_ko_count".to_string(), "mbr_runtime_v1".to_string(), "num".to_string(), "double".to_string()),
                ("hero_sidepot_ko_count".to_string(), "mbr_runtime_v1".to_string(), "num".to_string(), "double".to_string()),
                ("hero_split_ko_count".to_string(), "mbr_runtime_v1".to_string(), "num".to_string(), "double".to_string()),
                ("played_ft_hand".to_string(), "mbr_runtime_v1".to_string(), "bool".to_string(), "bool".to_string()),
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
                ("avg_finish_place".to_string(), "seed_snapshot".to_string(), "exact".to_string()),
                ("avg_ko_per_tournament".to_string(), "seed_snapshot".to_string(), "exact".to_string()),
                ("final_table_reach_percent".to_string(), "seed_snapshot".to_string(), "exact".to_string()),
                ("roi_pct".to_string(), "seed_snapshot".to_string(), "exact".to_string()),
                ("total_ko".to_string(), "seed_snapshot".to_string(), "exact".to_string()),
            ]
        );

        let dependency_count: i64 = client
            .query_one("SELECT COUNT(*) FROM analytics.stat_dependencies", &[])
            .unwrap()
            .get(0);
        let policy_count: i64 = client
            .query_one("SELECT COUNT(*) FROM analytics.materialization_policies", &[])
            .unwrap()
            .get(0);

        assert!(dependency_count >= 5);
        assert!(policy_count >= 9);
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_persists_time_provenance_members_and_alias_contracts() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0002_exact_pot_ko_core.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0003_mbr_stage_economics.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0004_exact_core_schema_v2.sql"),
        );
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
            Some("gg_text_without_timezone")
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
            Some("gg_text_without_timezone")
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
                    "GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt"
                        .to_string(),
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
        assert_eq!(
            hero_seat.get::<_, Option<Uuid>>(1),
            Some(player_profile_id)
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
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0002_exact_pot_ko_core.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0003_mbr_stage_economics.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0004_exact_core_schema_v2.sql"),
        );
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
                &[&first_ts_report.source_file_id, &first_hh_report.source_file_id],
            )
            .unwrap()
            .get(0);
        let attempt_count_after_first: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM import.job_attempts attempts
                 JOIN import.import_jobs jobs ON jobs.id = attempts.import_job_id
                 WHERE jobs.source_file_id IN ($1, $2)",
                &[&first_ts_report.source_file_id, &first_hh_report.source_file_id],
            )
            .unwrap()
            .get(0);

        let second_ts_report = import_path(&ts_path).unwrap();
        let second_hh_report = import_path(&hh_path).unwrap();

        assert_eq!(first_ts_report.source_file_id, second_ts_report.source_file_id);
        assert_eq!(first_hh_report.source_file_id, second_hh_report.source_file_id);

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
                &[&first_ts_report.source_file_id, &first_hh_report.source_file_id],
            )
            .unwrap()
            .get(0);
        let attempt_count_after_second: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM import.job_attempts attempts
                 JOIN import.import_jobs jobs ON jobs.id = attempts.import_job_id
                 WHERE jobs.source_file_id IN ($1, $2)",
                &[&first_ts_report.source_file_id, &first_hh_report.source_file_id],
            )
            .unwrap()
            .get(0);

        assert_eq!(source_file_count_after_first + 0, source_file_count_after_second);
        assert_eq!(member_count_after_first + 0, member_count_after_second);
        assert_eq!(import_job_count_after_first + 2, import_job_count_after_second);
        assert_eq!(attempt_count_after_first + 2, attempt_count_after_second);
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_persists_canonical_hand_layer_to_postgres() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0002_exact_pot_ko_core.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0003_mbr_stage_economics.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0004_exact_core_schema_v2.sql"),
        );
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
                    rake_amount,
                    final_stacks->>'Hero',
                    final_stacks->>'f02e54a6',
                    invariant_errors::text
                 FROM derived.hand_state_resolutions
                 WHERE hand_id = $1
                   AND resolution_version = $2",
                &[&hand_id, &HAND_RESOLUTION_VERSION],
            )
            .unwrap();

        assert!(resolution.get::<_, bool>(0));
        assert!(resolution.get::<_, bool>(1));
        assert_eq!(resolution.get::<_, i64>(2), 0);
        assert_eq!(
            resolution.get::<_, Option<String>>(3).as_deref(),
            Some("18000")
        );
        assert_eq!(resolution.get::<_, Option<String>>(4).as_deref(), Some("0"));
        assert_eq!(resolution.get::<_, String>(5), "[]");

        let mbr_stage = client
            .query_one(
                "SELECT
                    played_ft_hand,
                    played_ft_hand_state,
                    entered_boundary_zone,
                    entered_boundary_zone_state,
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
        assert!(!mbr_stage.get::<_, bool>(2));
        assert_eq!(mbr_stage.get::<_, String>(3), "exact");
        assert_eq!(mbr_stage.get::<_, Option<i32>>(4), Some(2));
        assert_eq!(mbr_stage.get::<_, Option<String>>(5), None);
        assert_eq!(mbr_stage.get::<_, String>(6), "uncertain");

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
                    entered_boundary_zone,
                    entered_boundary_zone_state,
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
        assert!(boundary_stage.get::<_, bool>(1));
        assert_eq!(boundary_stage.get::<_, String>(2), "estimated");
        assert_eq!(boundary_stage.get::<_, Option<i32>>(3), None);
        assert_eq!(
            boundary_stage.get::<_, Option<String>>(4).as_deref(),
            Some("0.000000")
        );
        assert_eq!(boundary_stage.get::<_, String>(5), "estimated");

        let elimination = client
            .query_one(
                "SELECT
                    eliminated_seat_no,
                    eliminated_player_name,
                    resolved_by_pot_no,
                    ko_involved_winner_count,
                    hero_involved,
                    hero_share_fraction::text,
                    is_split_ko,
                    split_n,
                    is_sidepot_based,
                    certainty_state
                 FROM derived.hand_eliminations
                 WHERE hand_id = $1",
                &[&hand_id],
            )
            .unwrap();

        assert_eq!(elimination.get::<_, i32>(0), 3);
        assert_eq!(elimination.get::<_, String>(1), "f02e54a6");
        assert_eq!(elimination.get::<_, Option<i32>>(2), Some(1));
        assert_eq!(elimination.get::<_, i32>(3), 1);
        assert!(elimination.get::<_, bool>(4));
        assert_eq!(
            elimination.get::<_, Option<String>>(5).as_deref(),
            Some("1.000000")
        );
        assert!(!elimination.get::<_, bool>(6));
        assert_eq!(elimination.get::<_, Option<i32>>(7), Some(1));
        assert!(!elimination.get::<_, bool>(8));
        assert_eq!(elimination.get::<_, String>(9), "exact");

        let street_strength_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.street_hand_strength
                 WHERE hand_id = $1
                   AND descriptor_version = 'gg_mbr_street_strength_v1'",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let hero_street_strength_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.street_hand_strength
                 WHERE hand_id = $1
                   AND seat_no = 7
                   AND descriptor_version = 'gg_mbr_street_strength_v1'",
                &[&hand_id],
            )
            .unwrap()
            .get(0);
        let villain_street_strength_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.street_hand_strength
                 WHERE hand_id = $1
                   AND seat_no = 3
                   AND descriptor_version = 'gg_mbr_street_strength_v1'",
                &[&hand_id],
            )
            .unwrap()
            .get(0);

        assert_eq!(street_strength_count, 6);
        assert_eq!(hero_street_strength_count, 3);
        assert_eq!(villain_street_strength_count, 3);

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
    fn import_local_refreshes_analytics_features_and_seed_stats() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0002_exact_pot_ko_core.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0003_mbr_stage_economics.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0004_exact_core_schema_v2.sql"),
        );
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

        assert_eq!(economics.get::<_, Option<String>>(0).as_deref(), Some("100.00"));
        assert_eq!(economics.get::<_, Option<String>>(1).as_deref(), Some("205.00"));
        assert_eq!(economics.get::<_, Option<String>>(2).as_deref(), Some("105.00"));
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
        assert!(seed_stats.total_ko >= 1);
    }

    #[test]
    #[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
    fn import_local_full_pack_smoke_is_clean_and_idempotent() {
        let _guard = db_test_guard();
        let database_url = env::var("CHECK_MATE_DATABASE_URL")
            .expect("CHECK_MATE_DATABASE_URL must exist for integration test");
        let mut setup_client = Client::connect(&database_url, NoTls).unwrap();
        reset_dev_player_data(&mut setup_client);
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0002_exact_pot_ko_core.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0003_mbr_stage_economics.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../migrations/0004_exact_core_schema_v2.sql"),
        );
        apply_sql_file(
            &mut setup_client,
            &fixture_path("../../seeds/0001_reference_data.sql"),
        );

        for (ts_fixture, hh_fixture) in FULL_PACK_FIXTURE_PAIRS {
            let ts_path = fixture_path(&format!("../../fixtures/mbr/ts/{ts_fixture}"));
            let hh_path = fixture_path(&format!("../../fixtures/mbr/hh/{hh_fixture}"));

            import_path(&ts_path).unwrap();
            import_path(&hh_path).unwrap();
        }

        let mut client = Client::connect(&database_url, NoTls).unwrap();
        let player_profile_id = dev_player_profile_id(&mut client);

        let parse_issue_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.parse_issues pi
                 JOIN import.source_files sf ON sf.id = pi.source_file_id
                 WHERE sf.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(parse_issue_count, 0);

        let invariant_mismatch_count: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.hand_state_resolutions hs
                 JOIN core.hands h ON h.id = hs.hand_id
                 WHERE h.player_profile_id = $1
                   AND (
                       NOT hs.chip_conservation_ok
                       OR NOT hs.pot_conservation_ok
                       OR jsonb_array_length(hs.invariant_errors) > 0
                   )",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(invariant_mismatch_count, 0);

        let counts_before = player_hand_row_counts(&mut client, player_profile_id);

        for (_, hh_fixture) in FULL_PACK_FIXTURE_PAIRS {
            let hh_path = fixture_path(&format!("../../fixtures/mbr/hh/{hh_fixture}"));
            import_path(&hh_path).unwrap();
        }

        let mut verify_client = Client::connect(&database_url, NoTls).unwrap();
        let counts_after = player_hand_row_counts(&mut verify_client, player_profile_id);
        assert_eq!(counts_after, counts_before);

        let parse_issue_count_after: i64 = verify_client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.parse_issues pi
                 JOIN import.source_files sf ON sf.id = pi.source_file_id
                 WHERE sf.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        assert_eq!(parse_issue_count_after, 0);
    }

    fn first_ft_hand_text() -> String {
        let content = fs::read_to_string(fixture_path(
            "../../fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt",
        ))
        .unwrap();
        content.split("\n\n").next().unwrap().trim().to_string()
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

    fn player_hand_row_counts(
        client: &mut Client,
        player_profile_id: Uuid,
    ) -> (i64, i64, i64, i64, i64, i64, i64, i64, i64) {
        let hands: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.hands
                 WHERE player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let actions: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.hand_actions a
                 JOIN core.hands h ON h.id = a.hand_id
                 WHERE h.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let pots: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.hand_pots p
                 JOIN core.hands h ON h.id = p.hand_id
                 WHERE h.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let contributions: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.hand_pot_contributions c
                 JOIN core.hands h ON h.id = c.hand_id
                 WHERE h.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let winners: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.hand_pot_winners w
                 JOIN core.hands h ON h.id = w.hand_id
                 WHERE h.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let returns: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM core.hand_returns r
                 JOIN core.hands h ON h.id = r.hand_id
                 WHERE h.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let resolutions: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.hand_state_resolutions hs
                 JOIN core.hands h ON h.id = hs.hand_id
                 WHERE h.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let eliminations: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.hand_eliminations e
                 JOIN core.hands h ON h.id = e.hand_id
                 WHERE h.player_profile_id = $1",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);
        let street_strength: i64 = client
            .query_one(
                "SELECT COUNT(*)
                 FROM derived.street_hand_strength sh
                 JOIN core.hands h ON h.id = sh.hand_id
                 WHERE h.player_profile_id = $1
                   AND sh.descriptor_version = 'gg_mbr_street_strength_v1'",
                &[&player_profile_id],
            )
            .unwrap()
            .get(0);

        (
            hands,
            actions,
            pots,
            contributions,
            winners,
            returns,
            resolutions,
            eliminations,
            street_strength,
        )
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

        let Some(player_profile_id) = player_profile_id else {
            return;
        };

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
                "DELETE FROM import.source_file_members
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
                "DELETE FROM import.import_jobs
                 WHERE source_file_id IN (
                     SELECT id FROM import.source_files WHERE player_profile_id = $1
                 )",
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
    }
}
