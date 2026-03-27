use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::Read,
    path::Path,
};

use anyhow::{Context, Result, anyhow};
use mbr_stats_runtime::{GG_MBR_FT_MAX_PLAYERS, materialize_player_hand_features};
use postgres::{Client, NoTls, Transaction};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tracker_ingest_runtime::{
    BundleStatus as IngestBundleStatus, ClaimedJob as IngestClaimedJob, FileKind as IngestFileKind,
    IngestBundleInput, IngestFileInput, JobExecutionError, JobExecutor, enqueue_bundle,
    load_bundle_summary, run_next_job,
};
use tracker_parser_core::{
    EXACT_CORE_RESOLUTION_VERSION,
    SourceKind, detect_source_kind,
    models::{
        ActionType, CanonicalParsedHand, CertaintyState, HandSettlement, InvariantIssue,
        ParseIssue, ParseIssueCode, ParseIssuePayload, Street, TournamentSummary,
    },
    normalizer::normalize_hand,
    parsers::{
        hand_history::{parse_canonical_hand, split_hand_history},
        tournament_summary::parse_tournament_summary,
    },
    positions::{PositionSeatInput, compute_position_facts},
    street_strength::evaluate_street_hand_strength,
};
use uuid::Uuid;

const DEV_ORG_NAME: &str = "Check Mate Dev Org";
const DEV_USER_EMAIL: &str = "mbr-dev-student@example.com";
const DEV_PLAYER_NAME: &str = "Hero";
const HAND_RESOLUTION_VERSION: &str = EXACT_CORE_RESOLUTION_VERSION;

#[derive(Debug)]
pub struct LocalImportReport {
    pub file_kind: &'static str,
    pub source_file_id: Uuid,
    pub import_job_id: Uuid,
    pub tournament_id: Uuid,
    pub fragments_persisted: usize,
    pub hands_persisted: usize,
}

struct LocalImportExecutor {
    report: Option<LocalImportReport>,
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
    positions: Vec<HandPositionRow>,
    hole_cards: Vec<HandHoleCardsRow>,
    actions: Vec<HandActionRow>,
    board: Option<HandBoardRow>,
    showdowns: Vec<HandShowdownRow>,
    summary_seat_outcomes: Vec<HandSummarySeatOutcomeRow>,
    parse_issues: Vec<ParseIssueRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandSeatRow {
    seat_no: i32,
    player_name: String,
    starting_stack: i64,
    is_hero: bool,
    is_button: bool,
    is_sitting_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandPositionRow {
    seat_no: i32,
    position_index: i32,
    position_label: String,
    preflop_act_order_index: i32,
    postflop_act_order_index: i32,
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
    all_in_reason: Option<String>,
    forced_all_in_preflop: bool,
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
struct HandSummarySeatOutcomeRow {
    seat_no: i32,
    player_name: String,
    position_marker: Option<String>,
    outcome_kind: String,
    folded_street: Option<String>,
    shown_cards: Option<Vec<String>>,
    won_amount: Option<i64>,
    hand_class: Option<String>,
    raw_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParseIssueRow {
    severity: String,
    code: String,
    message: String,
    raw_line: Option<String>,
    payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandStateResolutionRow {
    resolution_version: String,
    chip_conservation_ok: bool,
    pot_conservation_ok: bool,
    settlement_state: String,
    rake_amount: i64,
    final_stacks: BTreeMap<String, i64>,
    settlement: HandSettlement,
    invariant_issues: Vec<InvariantIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandPotRow {
    pot_no: i32,
    pot_type: String,
    amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandPotEligibilityRow {
    pot_no: i32,
    seat_no: i32,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HandEliminationKoShareRow {
    seat_no: i32,
    player_name: String,
    share_fraction: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandEliminationRow {
    eliminated_seat_no: i32,
    eliminated_player_name: String,
    pots_participated_by_busted: Vec<i32>,
    pots_causing_bust: Vec<i32>,
    last_busting_pot_no: Option<i32>,
    ko_winner_set: Vec<String>,
    ko_share_fraction_by_winner: Vec<HandEliminationKoShareRow>,
    elimination_certainty_state: String,
    ko_certainty_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MbrStageResolutionRow {
    player_profile_id: Uuid,
    played_ft_hand: bool,
    played_ft_hand_state: String,
    is_ft_hand: bool,
    ft_players_remaining_exact: Option<i32>,
    is_stage_2: bool,
    is_stage_3_4: bool,
    is_stage_4_5: bool,
    is_stage_5_6: bool,
    is_stage_6_9: bool,
    is_boundary_hand: bool,
    entered_boundary_zone: bool,
    entered_boundary_zone_state: String,
    boundary_resolution_state: String,
    boundary_candidate_count: i32,
    boundary_resolution_method: String,
    boundary_confidence_class: String,
    ft_table_size: Option<i32>,
    boundary_ko_ev: Option<String>,
    boundary_ko_min: Option<String>,
    boundary_ko_max: Option<String>,
    boundary_ko_method: Option<String>,
    boundary_ko_certainty: Option<String>,
    boundary_ko_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MbrTournamentFtHelperRow {
    tournament_id: Uuid,
    player_profile_id: Uuid,
    reached_ft_exact: bool,
    first_ft_hand_id: Option<Uuid>,
    first_ft_hand_started_local: Option<String>,
    first_ft_table_size: Option<i32>,
    ft_started_incomplete: Option<bool>,
    deepest_ft_size_reached: Option<i32>,
    hero_ft_entry_stack_chips: Option<i64>,
    hero_ft_entry_stack_bb: Option<String>,
    entered_boundary_zone: bool,
    boundary_resolution_state: String,
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
struct BoundaryResolution {
    candidate_hand_ids: BTreeSet<String>,
    resolution_state: String,
    resolution_method: String,
    confidence_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TournamentFtHelperSourceHand {
    hand_id: Uuid,
    external_hand_id: String,
    hand_started_at_local: String,
    played_ft_hand: bool,
    played_ft_hand_state: String,
    ft_table_size: Option<i32>,
    entered_boundary_zone: bool,
    boundary_resolution_state: String,
    hero_starting_stack: Option<i64>,
    big_blind: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StreetHandStrengthRow {
    seat_no: i32,
    street: String,
    best_hand_class: String,
    best_hand_rank_value: i64,
    made_hand_category: String,
    draw_category: String,
    overcards_count: i32,
    has_air: bool,
    missed_flush_draw: bool,
    missed_straight_draw: bool,
    is_nut_hand: Option<bool>,
    is_nut_draw: Option<bool>,
    certainty_state: String,
}

pub fn import_path(path: &str) -> Result<LocalImportReport> {
    let database_url = env::var("CHECK_MATE_DATABASE_URL")
        .context("CHECK_MATE_DATABASE_URL is required for `import-local`")?;
    let input = fs::read_to_string(path).with_context(|| format!("failed to read `{path}`"))?;

    let mut client =
        Client::connect(&database_url, NoTls).context("failed to connect to PostgreSQL")?;
    let mut tx = client
        .transaction()
        .context("failed to start ingest enqueue transaction")?;
    let context = ensure_dev_context(&mut tx)?;
    let bundle = enqueue_bundle(
        &mut tx,
        &IngestBundleInput {
            organization_id: context.organization_id,
            player_profile_id: context.player_profile_id,
            created_by_user_id: context.user_id,
            files: vec![build_ingest_file_input(path, &input)?],
        },
    )?;
    tx.commit()
        .context("failed to commit ingest enqueue transaction")?;

    let mut executor = LocalImportExecutor { report: None };
    loop {
        let mut tx = client
            .transaction()
            .context("failed to start ingest runner transaction")?;
        let claimed = run_next_job(&mut tx, "parser_worker_local", 3, &mut executor)?;
        let summary = load_bundle_summary(&mut tx, bundle.bundle_id)?;
        tx.commit()
            .context("failed to commit ingest runner transaction")?;

        if matches!(
            summary.status,
            IngestBundleStatus::Succeeded
                | IngestBundleStatus::PartialSuccess
                | IngestBundleStatus::Failed
        ) && !summary.finalize_job_running
        {
            break;
        }

        if claimed.is_none() && !summary.finalize_job_present {
            break;
        }
    }

    executor
        .report
        .ok_or_else(|| anyhow!("ingest bundle for `{path}` finished without successful file import"))
}

pub fn run_ingest_runner_until_idle(
    database_url: &str,
    runner_name: &str,
    max_attempts: i32,
) -> Result<usize> {
    let mut client =
        Client::connect(database_url, NoTls).context("failed to connect to PostgreSQL")?;
    let mut executor = LocalImportExecutor { report: None };
    let mut processed_jobs = 0usize;

    loop {
        let mut tx = client
            .transaction()
            .context("failed to start ingest runner transaction")?;
        let claimed = run_next_job(&mut tx, runner_name, max_attempts, &mut executor)?;
        tx.commit()
            .context("failed to commit ingest runner transaction")?;

        if claimed.is_some() {
            processed_jobs += 1;
        } else {
            break;
        }
    }

    Ok(processed_jobs)
}

fn build_ingest_file_input(path: &str, input: &str) -> Result<IngestFileInput> {
    let file_kind = match detect_source_kind(input)? {
        SourceKind::TournamentSummary => IngestFileKind::TournamentSummary,
        SourceKind::HandHistory => IngestFileKind::HandHistory,
    };

    Ok(IngestFileInput {
        room: "gg".to_string(),
        file_kind,
        sha256: sha256_hex(input),
        original_filename: source_filename(path)?,
        byte_size: input.len() as i64,
        storage_uri: format!("local://{}", path.replace('\\', "/")),
        members: vec![],
        diagnostics: vec![],
    })
}

fn storage_path_from_uri(storage_uri: &str) -> std::result::Result<&str, JobExecutionError> {
    storage_uri
        .strip_prefix("local://")
        .ok_or_else(|| JobExecutionError::terminal("unsupported_storage_uri"))
}

fn read_archive_member_text(path: &str, member_path: &str) -> Result<String> {
    let file =
        fs::File::open(path).with_context(|| format!("failed to open archive `{path}`"))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("failed to open ZIP archive `{path}`"))?;
    let mut member = archive
        .by_name(member_path)
        .with_context(|| format!("missing ZIP member `{member_path}` in `{path}`"))?;
    let mut input = String::new();
    member
        .read_to_string(&mut input)
        .with_context(|| format!("failed to read ZIP member `{member_path}` as UTF-8 text"))?;
    Ok(input)
}

fn load_ingest_job_input(
    job: &IngestClaimedJob,
) -> std::result::Result<(String, String), JobExecutionError> {
    let storage_uri = job
        .storage_uri
        .as_deref()
        .ok_or_else(|| JobExecutionError::terminal("missing_storage_uri"))?;
    let path = storage_path_from_uri(storage_uri)?;

    match job.source_file_kind {
        Some(IngestFileKind::Archive) => {
            let member_path = job
                .member_path
                .as_deref()
                .ok_or_else(|| JobExecutionError::terminal("missing_archive_member_path"))?;
            let input = read_archive_member_text(path, member_path)
                .map_err(|_| JobExecutionError::retriable("archive_member_read_failed"))?;
            Ok((member_path.to_string(), input))
        }
        _ => {
            let input =
                fs::read_to_string(path).map_err(|_| JobExecutionError::retriable("storage_read_failed"))?;
            Ok((path.to_string(), input))
        }
    }
}

fn import_path_with_database_url(database_url: &str, path: &str) -> Result<LocalImportReport> {
    let input = fs::read_to_string(path).with_context(|| format!("failed to read `{path}`"))?;

    let mut client =
        Client::connect(database_url, NoTls).context("failed to connect to PostgreSQL")?;
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

impl JobExecutor for LocalImportExecutor {
    fn execute_file_job<C: postgres::GenericClient>(
        &mut self,
        client: &mut C,
        job: &IngestClaimedJob,
    ) -> std::result::Result<(), JobExecutionError> {
        let (path, input) = load_ingest_job_input(job)?;
        let context = load_existing_context(client, job.organization_id, job.player_profile_id)
            .map_err(|_| JobExecutionError::terminal("missing_execution_context"))?;

        let report: Result<LocalImportReport> = match job.file_kind {
            Some(IngestFileKind::TournamentSummary) => import_tournament_summary_registered(
                client,
                &context,
                &path,
                &input,
                job.source_file_id
                    .ok_or_else(|| JobExecutionError::terminal("missing_source_file_id"))?,
                job.source_file_member_id
                    .ok_or_else(|| JobExecutionError::terminal("missing_source_file_member_id"))?,
                job.job_id,
            ),
            Some(IngestFileKind::HandHistory) => import_hand_history_registered(
                client,
                &context,
                &path,
                &input,
                job.source_file_id
                    .ok_or_else(|| JobExecutionError::terminal("missing_source_file_id"))?,
                job.source_file_member_id
                    .ok_or_else(|| JobExecutionError::terminal("missing_source_file_member_id"))?,
                job.job_id,
            ),
            Some(IngestFileKind::Archive) => Err(anyhow!(
                "archive top-level kind cannot be executed as a parsed member job"
            )),
            None => Err(anyhow!("missing file kind for file_ingest job")),
        };
        let report = report.map_err(|error| JobExecutionError::terminal(format!("{error:#}")))?;

        self.report = Some(report);
        Ok(())
    }

    fn finalize_bundle<C: postgres::GenericClient>(
        &mut self,
        client: &mut C,
        job: &IngestClaimedJob,
    ) -> std::result::Result<(), JobExecutionError> {
        materialize_player_hand_features(client, job.organization_id, job.player_profile_id)
            .map(|_| ())
            .map_err(|error| JobExecutionError::retriable(error.to_string()))
    }
}

fn ensure_dev_context(client: &mut impl postgres::GenericClient) -> Result<DevContext> {
    let organization_id = if let Some(row) = client.query_opt(
        "SELECT id FROM org.organizations WHERE name = $1",
        &[&DEV_ORG_NAME],
    )? {
        row.get(0)
    } else {
        client.query_one(
            "INSERT INTO org.organizations (name) VALUES ($1) RETURNING id",
            &[&DEV_ORG_NAME],
        )?
        .get(0)
    };

    let user_id = if let Some(row) = client.query_opt(
        "SELECT id FROM auth.users WHERE email = $1",
        &[&DEV_USER_EMAIL],
    )? {
        row.get(0)
    } else {
        client.query_one(
                "INSERT INTO auth.users (email, auth_provider, status) VALUES ($1, 'seed', 'active') RETURNING id",
                &[&DEV_USER_EMAIL],
            )?
            .get(0)
    };

    client.execute(
        "INSERT INTO org.organization_memberships (organization_id, user_id, role)
         VALUES ($1, $2, 'student')
         ON CONFLICT (organization_id, user_id) DO NOTHING",
        &[&organization_id, &user_id],
    )?;

    let player_profile_id = if let Some(row) = client.query_opt(
        "SELECT id FROM core.player_profiles WHERE organization_id = $1 AND room = 'gg' AND screen_name = $2",
        &[&organization_id, &DEV_PLAYER_NAME],
    )? {
        row.get(0)
    } else {
        client.query_one(
            "INSERT INTO core.player_profiles (organization_id, owner_user_id, room, network, screen_name)
             VALUES ($1, $2, 'gg', 'gg', $3)
             RETURNING id",
            &[&organization_id, &user_id, &DEV_PLAYER_NAME],
        )?
        .get(0)
    };

    client.execute(
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

    let player_aliases = client
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

    let room_id = client
        .query_one("SELECT id FROM core.rooms WHERE code = 'gg'", &[])?
        .get(0);
    let format_id = client
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

fn load_existing_context(
    client: &mut impl postgres::GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<DevContext> {
    let row = client
        .query_opt(
            "SELECT owner_user_id
             FROM core.player_profiles
             WHERE id = $1
               AND organization_id = $2
               AND room = 'gg'",
            &[&player_profile_id, &organization_id],
        )?
        .ok_or_else(|| {
            anyhow!(
                "player profile {} is missing in organization {}",
                player_profile_id,
                organization_id
            )
        })?;
    let user_id: Uuid = row.get(0);

    let player_aliases = client
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

    let room_id = client
        .query_one("SELECT id FROM core.rooms WHERE code = 'gg'", &[])?
        .get(0);
    let format_id = client
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
    let source_file_id = insert_source_file(tx, context, path, input, "ts")?;
    let source_file_member_id = insert_source_file_member(tx, source_file_id, path, "ts", input)?;
    let import_job_id = insert_import_job(tx, context.organization_id, source_file_id)?;
    insert_job_attempt(tx, import_job_id)?;
    import_tournament_summary_registered(
        tx,
        context,
        path,
        input,
        source_file_id,
        source_file_member_id,
        import_job_id,
    )
}

fn import_tournament_summary_registered(
    tx: &mut impl postgres::GenericClient,
    context: &DevContext,
    _path: &str,
    input: &str,
    source_file_id: Uuid,
    source_file_member_id: Uuid,
    import_job_id: Uuid,
) -> Result<LocalImportReport> {
    let summary = parse_tournament_summary(input)?;
    let tournament_entry_economics = load_tournament_entry_economics(tx, context, &summary)?;
    let fragment_id = insert_file_fragment(
        tx,
        source_file_id,
        source_file_member_id,
        0,
        None,
        "summary",
        input,
    )?;

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
                'gg_user_timezone_missing',
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

    tx.execute(
        "DELETE FROM core.parse_issues
         WHERE source_file_id = $1
           AND fragment_id = $2",
        &[&source_file_id, &fragment_id],
    )?;

    for issue in tournament_summary_parse_issues(&summary) {
        tx.execute(
            "INSERT INTO core.parse_issues (
                source_file_id,
                fragment_id,
                hand_id,
                severity,
                code,
                message,
                raw_line,
                payload
            )
            VALUES ($1, $2, NULL, $3, $4, $5, $6, ($7::text)::jsonb)",
            &[
                &source_file_id,
                &fragment_id,
                &issue.severity,
                &issue.code,
                &issue.message,
                &issue.raw_line,
                &issue.payload.to_string(),
            ],
        )?;
    }

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
    let source_file_id = insert_source_file(tx, context, path, input, "hh")?;
    let source_file_member_id = insert_source_file_member(tx, source_file_id, path, "hh", input)?;
    let import_job_id = insert_import_job(tx, context.organization_id, source_file_id)?;
    insert_job_attempt(tx, import_job_id)?;
    import_hand_history_registered(
        tx,
        context,
        path,
        input,
        source_file_id,
        source_file_member_id,
        import_job_id,
    )
}

fn import_hand_history_registered(
    tx: &mut impl postgres::GenericClient,
    context: &DevContext,
    _path: &str,
    input: &str,
    source_file_id: Uuid,
    source_file_member_id: Uuid,
    import_job_id: Uuid,
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
            let known_tournaments = tx
                .query(
                    "SELECT external_tournament_id
                     FROM core.tournaments
                     WHERE player_profile_id = $1
                       AND room_id = $2
                     ORDER BY external_tournament_id",
                    &[&context.player_profile_id, &context.room_id],
                )
                .map(|rows| {
                    rows.into_iter()
                        .map(|row| row.get::<_, String>(0))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let all_tournaments = tx
                .query(
                    "SELECT player_profile_id::text, external_tournament_id
                     FROM core.tournaments
                     ORDER BY player_profile_id, external_tournament_id",
                    &[],
                )
                .map(|rows| {
                    rows.into_iter()
                        .map(|row| {
                            format!(
                                "{}:{}",
                                row.get::<_, String>(0),
                                row.get::<_, String>(1)
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            anyhow!(
                "tournament {} is missing in core.tournaments; player_profile_id={} room_id={} known_tournaments={:?} all_tournaments={:?}; import the matching TS file first",
                first_hand.header.tournament_id,
                context.player_profile_id,
                context.room_id,
                known_tournaments,
                all_tournaments
            )
        })?;

    let stage_facts = canonical_hands
        .iter()
        .zip(normalized_hands.iter())
        .map(|(hand, normalized_hand)| {
            let exact_hero_boundary_ko_share =
                exact_hero_boundary_ko_share(hand, normalized_hand);

            StageHandFact {
                hand_id: hand.header.hand_id.clone(),
                played_at: hand.header.played_at.clone(),
                max_players: hand.header.max_players,
                seat_count: hand.seats.len(),
                exact_hero_boundary_ko_share,
            }
        })
        .collect::<Vec<_>>();
    let mbr_stage_resolutions =
        build_mbr_stage_resolutions_from_facts(context.player_profile_id, &stage_facts);
    let mut tournament_ft_helper_source_hands = Vec::with_capacity(canonical_hands.len());

    for (index, hand) in hands.iter().enumerate() {
        let fragment_id = insert_file_fragment(
            tx,
            source_file_id,
            source_file_member_id,
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
        persist_canonical_hand(
            tx,
            context,
            source_file_id,
            fragment_id,
            hand_id,
            canonical_hand,
        )?;
        let normalized_hand = &normalized_hands[index];
        persist_normalized_hand(tx, hand_id, normalized_hand)?;
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
        tournament_ft_helper_source_hands.push(build_tournament_ft_helper_source_hand(
            hand_id,
            canonical_hand,
            mbr_stage_resolution,
        ));
    }

    // F2-T2: Compute stable tournament_hand_order for all hands in this tournament.
    // Uses the same sort criteria as Rust-side chronological sort: timestamp + external_hand_id + id.
    tx.execute(
        "WITH ordered AS (
            SELECT id,
                   ROW_NUMBER() OVER (
                       ORDER BY hand_started_at_local NULLS LAST,
                                external_hand_id,
                                id
                   )::int AS computed_order
            FROM core.hands
            WHERE tournament_id = $1
        )
        UPDATE core.hands h
        SET tournament_hand_order = ordered.computed_order
        FROM ordered
        WHERE h.id = ordered.id",
        &[&tournament_id],
    )?;

    let tournament_ft_helper_row = build_mbr_tournament_ft_helper_row(
        tournament_id,
        context.player_profile_id,
        &tournament_ft_helper_source_hands,
    );
    persist_mbr_tournament_ft_helper(tx, &tournament_ft_helper_row)?;

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
    tx: &mut impl postgres::GenericClient,
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
                'gg_user_timezone_missing',
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
    tx: &mut impl postgres::GenericClient,
    context: &DevContext,
    source_file_id: Uuid,
    fragment_id: Uuid,
    hand_id: Uuid,
    hand: &CanonicalParsedHand,
) -> Result<()> {
    let rows = build_canonical_persistence(hand)?;
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
                is_button,
                is_sitting_out
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
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
                &seat.is_sitting_out,
            ],
        )?;
    }

    for position in &rows.positions {
        tx.execute(
            "INSERT INTO core.hand_positions (
                hand_id,
                seat_no,
                position_index,
                position_label,
                preflop_act_order_index,
                postflop_act_order_index
            )
            VALUES ($1, $2, $3, $4, $5, $6)",
            &[
                &hand_id,
                &position.seat_no,
                &position.position_index,
                &position.position_label,
                &position.preflop_act_order_index,
                &position.postflop_act_order_index,
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
                all_in_reason,
                forced_all_in_preflop,
                references_previous_bet,
                raw_line
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
            &[
                &hand_id,
                &action.sequence_no,
                &action.street,
                &action.seat_no,
                &action.action_type,
                &action.raw_amount,
                &action.to_amount,
                &action.is_all_in,
                &action.all_in_reason,
                &action.forced_all_in_preflop,
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

    for summary_outcome in &rows.summary_seat_outcomes {
        tx.execute(
            "INSERT INTO core.hand_summary_results (
                hand_id,
                seat_no,
                player_name,
                position_marker,
                outcome_kind,
                folded_street,
                shown_cards,
                won_amount,
                hand_class,
                raw_line
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            &[
                &hand_id,
                &summary_outcome.seat_no,
                &summary_outcome.player_name,
                &summary_outcome.position_marker,
                &summary_outcome.outcome_kind,
                &summary_outcome.folded_street,
                &summary_outcome.shown_cards,
                &summary_outcome.won_amount,
                &summary_outcome.hand_class,
                &summary_outcome.raw_line,
            ],
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
                raw_line,
                payload
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, ($8::text)::jsonb)",
            &[
                &source_file_id,
                &fragment_id,
                &hand_id,
                &issue.severity,
                &issue.code,
                &issue.message,
                &issue.raw_line,
                &issue.payload.to_string(),
            ],
        )?;
    }

    Ok(())
}

fn persist_normalized_hand(
    tx: &mut impl postgres::GenericClient,
    hand_id: Uuid,
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Result<()> {
    let row = build_hand_state_resolution(normalized_hand);
    let pot_rows = build_hand_pot_rows(normalized_hand);
    let eligibility_rows = build_hand_pot_eligibility_rows(normalized_hand);
    let contribution_rows = build_hand_pot_contribution_rows(normalized_hand);
    let winner_rows = build_hand_pot_winner_rows(normalized_hand);
    let return_rows = build_hand_return_rows(normalized_hand);
    let elimination_rows = build_hand_elimination_rows(normalized_hand);
    let final_stacks_json = serde_json::to_string(&row.final_stacks)?;
    let settlement_json = serde_json::to_string(&row.settlement)?;
    let invariant_issues_json = serde_json::to_string(&row.invariant_issues)?;

    tx.execute(
        "INSERT INTO derived.hand_state_resolutions (
            hand_id,
            resolution_version,
            chip_conservation_ok,
            pot_conservation_ok,
            settlement_state,
            rake_amount,
            final_stacks,
            settlement,
            invariant_issues
        )
        VALUES ($1, $2, $3, $4, $5, $6, ($7::text)::jsonb, ($8::text)::jsonb, ($9::text)::jsonb)
        ON CONFLICT (hand_id, resolution_version)
        DO UPDATE SET
            chip_conservation_ok = EXCLUDED.chip_conservation_ok,
            pot_conservation_ok = EXCLUDED.pot_conservation_ok,
            settlement_state = EXCLUDED.settlement_state,
            rake_amount = EXCLUDED.rake_amount,
            final_stacks = EXCLUDED.final_stacks,
            settlement = EXCLUDED.settlement,
            invariant_issues = EXCLUDED.invariant_issues",
        &[
            &hand_id,
            &row.resolution_version,
            &row.chip_conservation_ok,
            &row.pot_conservation_ok,
            &row.settlement_state,
            &row.rake_amount,
            &final_stacks_json,
            &settlement_json,
            &invariant_issues_json,
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

    for eligibility_row in eligibility_rows {
        tx.execute(
            "INSERT INTO core.hand_pot_eligibility (
                hand_id,
                pot_no,
                seat_no
            )
            VALUES ($1, $2, $3)",
            &[&hand_id, &eligibility_row.pot_no, &eligibility_row.seat_no],
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
        let ko_share_fraction_by_winner_json =
            serde_json::to_string(&elimination_row.ko_share_fraction_by_winner)?;
        tx.execute(
            "INSERT INTO derived.hand_eliminations (
                hand_id,
                eliminated_seat_no,
                eliminated_player_name,
                pots_participated_by_busted,
                pots_causing_bust,
                last_busting_pot_no,
                ko_winner_set,
                ko_share_fraction_by_winner,
                elimination_certainty_state,
                ko_certainty_state
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, ($8::text)::jsonb, $9, $10
            )",
            &[
                &hand_id,
                &elimination_row.eliminated_seat_no,
                &elimination_row.eliminated_player_name,
                &elimination_row.pots_participated_by_busted,
                &elimination_row.pots_causing_bust,
                &elimination_row.last_busting_pot_no,
                &elimination_row.ko_winner_set,
                &ko_share_fraction_by_winner_json,
                &elimination_row.elimination_certainty_state,
                &elimination_row.ko_certainty_state,
            ],
        )?;
    }

    Ok(())
}

fn persist_mbr_stage_resolution(
    tx: &mut impl postgres::GenericClient,
    hand_id: Uuid,
    row: &MbrStageResolutionRow,
) -> Result<()> {
    tx.execute(
        "INSERT INTO derived.mbr_stage_resolution (
            hand_id,
            player_profile_id,
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
            boundary_resolution_method,
            boundary_confidence_class,
            ft_table_size,
            boundary_ko_ev,
            boundary_ko_min,
            boundary_ko_max,
            boundary_ko_method,
            boundary_ko_certainty,
            boundary_ko_state
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18,
            $19,
            ($20::text)::numeric(12,6),
            ($21::text)::numeric(12,6),
            ($22::text)::numeric(12,6),
            $23, $24, $25
        )
        ON CONFLICT (hand_id, player_profile_id)
        DO UPDATE SET
            played_ft_hand = EXCLUDED.played_ft_hand,
            played_ft_hand_state = EXCLUDED.played_ft_hand_state,
            is_ft_hand = EXCLUDED.is_ft_hand,
            ft_players_remaining_exact = EXCLUDED.ft_players_remaining_exact,
            is_stage_2 = EXCLUDED.is_stage_2,
            is_stage_3_4 = EXCLUDED.is_stage_3_4,
            is_stage_4_5 = EXCLUDED.is_stage_4_5,
            is_stage_5_6 = EXCLUDED.is_stage_5_6,
            is_stage_6_9 = EXCLUDED.is_stage_6_9,
            is_boundary_hand = EXCLUDED.is_boundary_hand,
            entered_boundary_zone = EXCLUDED.entered_boundary_zone,
            entered_boundary_zone_state = EXCLUDED.entered_boundary_zone_state,
            boundary_resolution_state = EXCLUDED.boundary_resolution_state,
            boundary_candidate_count = EXCLUDED.boundary_candidate_count,
            boundary_resolution_method = EXCLUDED.boundary_resolution_method,
            boundary_confidence_class = EXCLUDED.boundary_confidence_class,
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
            &row.is_ft_hand,
            &row.ft_players_remaining_exact,
            &row.is_stage_2,
            &row.is_stage_3_4,
            &row.is_stage_4_5,
            &row.is_stage_5_6,
            &row.is_stage_6_9,
            &row.is_boundary_hand,
            &row.entered_boundary_zone,
            &row.entered_boundary_zone_state,
            &row.boundary_resolution_state,
            &row.boundary_candidate_count,
            &row.boundary_resolution_method,
            &row.boundary_confidence_class,
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

fn persist_mbr_tournament_ft_helper(
    tx: &mut impl postgres::GenericClient,
    row: &MbrTournamentFtHelperRow,
) -> Result<()> {
    tx.execute(
        "INSERT INTO derived.mbr_tournament_ft_helper (
            tournament_id,
            player_profile_id,
            reached_ft_exact,
            first_ft_hand_id,
            first_ft_hand_started_local,
            first_ft_table_size,
            ft_started_incomplete,
            deepest_ft_size_reached,
            hero_ft_entry_stack_chips,
            hero_ft_entry_stack_bb,
            entered_boundary_zone,
            boundary_resolution_state
        )
        VALUES (
            $1,
            $2,
            $3,
            $4,
            replace($5, '/', '-')::timestamp,
            $6,
            $7,
            $8,
            $9,
            ($10::text)::numeric(18,6),
            $11,
            $12
        )
        ON CONFLICT (tournament_id, player_profile_id)
        DO UPDATE SET
            reached_ft_exact = EXCLUDED.reached_ft_exact,
            first_ft_hand_id = EXCLUDED.first_ft_hand_id,
            first_ft_hand_started_local = EXCLUDED.first_ft_hand_started_local,
            first_ft_table_size = EXCLUDED.first_ft_table_size,
            ft_started_incomplete = EXCLUDED.ft_started_incomplete,
            deepest_ft_size_reached = EXCLUDED.deepest_ft_size_reached,
            hero_ft_entry_stack_chips = EXCLUDED.hero_ft_entry_stack_chips,
            hero_ft_entry_stack_bb = EXCLUDED.hero_ft_entry_stack_bb,
            entered_boundary_zone = EXCLUDED.entered_boundary_zone,
            boundary_resolution_state = EXCLUDED.boundary_resolution_state",
        &[
            &row.tournament_id,
            &row.player_profile_id,
            &row.reached_ft_exact,
            &row.first_ft_hand_id,
            &row.first_ft_hand_started_local,
            &row.first_ft_table_size,
            &row.ft_started_incomplete,
            &row.deepest_ft_size_reached,
            &row.hero_ft_entry_stack_chips,
            &row.hero_ft_entry_stack_bb,
            &row.entered_boundary_zone,
            &row.boundary_resolution_state,
        ],
    )?;

    Ok(())
}

fn persist_street_hand_strength(
    tx: &mut impl postgres::GenericClient,
    hand_id: Uuid,
    rows: &[StreetHandStrengthRow],
) -> Result<()> {
    tx.execute(
        "DELETE FROM derived.street_hand_strength
         WHERE hand_id = $1",
        &[&hand_id],
    )?;

    for row in rows {
        tx.execute(
            "INSERT INTO derived.street_hand_strength (
                hand_id,
                seat_no,
                street,
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
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10,
                $11, $12, $13, $14
            )",
            &[
                &hand_id,
                &row.seat_no,
                &row.street,
                &row.best_hand_class,
                &row.best_hand_rank_value,
                &row.made_hand_category,
                &row.draw_category,
                &row.overcards_count,
                &row.has_air,
                &row.missed_flush_draw,
                &row.missed_straight_draw,
                &row.is_nut_hand,
                &row.is_nut_draw,
                &row.certainty_state,
            ],
        )?;
    }

    Ok(())
}

fn replace_hand_children(
    tx: &mut impl postgres::GenericClient,
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
        "DELETE FROM core.hand_summary_results WHERE hand_id = $1",
        &[&hand_id],
    )?;
    tx.execute(
        "DELETE FROM core.hand_positions WHERE hand_id = $1",
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
        "DELETE FROM core.hand_pot_eligibility WHERE hand_id = $1",
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
        settlement_state: certainty_state_code(normalized_hand.settlement.certainty_state)
            .to_string(),
        rake_amount: normalized_hand.actual.rake_amount,
        final_stacks: normalized_hand.actual.stacks_after_actual.clone(),
        settlement: normalized_hand.settlement.clone(),
        invariant_issues: normalized_hand.invariants.issues.clone(),
    }
}

fn build_hand_pot_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandPotRow> {
    normalized_hand
        .settlement
        .pots
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

fn build_street_hand_strength_rows(
    hand: &CanonicalParsedHand,
) -> Result<Vec<StreetHandStrengthRow>> {
    Ok(evaluate_street_hand_strength(hand)?
        .into_iter()
        .map(|descriptor| StreetHandStrengthRow {
            seat_no: descriptor.seat_no as i32,
            street: street_code(descriptor.street).to_string(),
            best_hand_class: descriptor.best_hand_class.as_str().to_string(),
            best_hand_rank_value: descriptor.best_hand_rank_value,
            made_hand_category: descriptor.made_hand_category.as_str().to_string(),
            draw_category: descriptor.draw_category.as_str().to_string(),
            overcards_count: i32::from(descriptor.overcards_count),
            has_air: descriptor.has_air,
            missed_flush_draw: descriptor.missed_flush_draw,
            missed_straight_draw: descriptor.missed_straight_draw,
            is_nut_hand: descriptor.is_nut_hand,
            is_nut_draw: descriptor.is_nut_draw,
            certainty_state: certainty_state_code(descriptor.certainty_state).to_string(),
        })
        .collect())
}

fn build_hand_pot_eligibility_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandPotEligibilityRow> {
    normalized_hand
        .settlement
        .pots
        .iter()
        .flat_map(|pot| pot.eligibilities.iter())
        .map(|eligibility| HandPotEligibilityRow {
            pot_no: i32::from(eligibility.pot_no),
            seat_no: i32::from(eligibility.seat_no),
        })
        .collect()
}

fn build_hand_pot_contribution_rows(
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Vec<HandPotContributionRow> {
    normalized_hand
        .settlement
        .pots
        .iter()
        .flat_map(|pot| pot.contributions.iter())
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
        .settlement
        .pots
        .iter()
        .flat_map(|pot| {
            pot.selected_allocation
                .iter()
                .flat_map(move |allocation| {
                    allocation
                        .shares
                        .iter()
                        .map(move |share| HandPotWinnerRow {
                            pot_no: i32::from(pot.pot_no),
                            seat_no: i32::from(share.seat_no),
                            share_amount: share.share_amount,
                        })
                })
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
        .map(|elimination| {
            HandEliminationRow {
                eliminated_seat_no: elimination.eliminated_seat_no as i32,
                eliminated_player_name: elimination.eliminated_player_name.clone(),
                pots_participated_by_busted: elimination
                    .pots_participated_by_busted
                    .iter()
                    .copied()
                    .map(i32::from)
                    .collect(),
                pots_causing_bust: elimination
                    .pots_causing_bust
                    .iter()
                    .copied()
                    .map(i32::from)
                    .collect(),
                last_busting_pot_no: elimination.last_busting_pot_no.map(i32::from),
                ko_winner_set: elimination.ko_winner_set.clone(),
                ko_share_fraction_by_winner: elimination
                    .ko_share_fraction_by_winner
                    .iter()
                    .map(|share| HandEliminationKoShareRow {
                        seat_no: i32::from(share.seat_no),
                        player_name: share.player_name.clone(),
                        share_fraction: format_fraction_value(share.share_fraction),
                    })
                    .collect(),
                elimination_certainty_state: certainty_state_code(
                    elimination.elimination_certainty_state,
                )
                .to_string(),
                ko_certainty_state: certainty_state_code(elimination.ko_certainty_state)
                    .to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
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
            exact_hero_boundary_ko_share: None,
        })
        .collect::<Vec<_>>();

    build_mbr_stage_resolutions_from_facts(player_profile_id, &facts)
}

fn resolve_boundary_candidates(facts: &[StageHandFact]) -> BoundaryResolution {
    let mut chronological = facts.iter().collect::<Vec<_>>();
    chronological.sort_by(|left, right| {
        left.played_at
            .cmp(&right.played_at)
            .then_with(|| left.hand_id.cmp(&right.hand_id))
    });

    let Some(first_ft_index) = chronological.iter().position(|hand| hand.max_players == GG_MBR_FT_MAX_PLAYERS as u8) else {
        return BoundaryResolution {
            candidate_hand_ids: BTreeSet::new(),
            resolution_state: "uncertain".to_string(),
            resolution_method: "timeline_last_non_ft_candidate_v2".to_string(),
            confidence_class: "no_exact_ft_hand".to_string(),
        };
    };

    if first_ft_index == 0 {
        return BoundaryResolution {
            candidate_hand_ids: BTreeSet::new(),
            resolution_state: "uncertain".to_string(),
            resolution_method: "timeline_last_non_ft_candidate_v2".to_string(),
            confidence_class: "no_pre_ft_candidate".to_string(),
        };
    }

    let last_non_ft_timestamp = chronological[first_ft_index - 1].played_at.as_str();
    let candidate_hand_ids = chronological[..first_ft_index]
        .iter()
        .rev()
        .take_while(|hand| hand.played_at == last_non_ft_timestamp)
        .map(|hand| hand.hand_id.clone())
        .collect::<BTreeSet<_>>();

    let (resolution_state, confidence_class) = if candidate_hand_ids.len() == 1 {
        ("exact".to_string(), "single_candidate".to_string())
    } else {
        (
            "uncertain".to_string(),
            "multi_candidate_same_timestamp".to_string(),
        )
    };

    BoundaryResolution {
        candidate_hand_ids,
        resolution_state,
        resolution_method: "timeline_last_non_ft_candidate_v2".to_string(),
        confidence_class,
    }
}

fn build_mbr_stage_resolutions_from_facts(
    player_profile_id: Uuid,
    facts: &[StageHandFact],
) -> BTreeMap<String, MbrStageResolutionRow> {
    let boundary_resolution = resolve_boundary_candidates(facts);
    let boundary_candidate_count = boundary_resolution.candidate_hand_ids.len() as i32;

    facts
        .iter()
        .map(|fact| {
            let played_ft_hand = fact.max_players == GG_MBR_FT_MAX_PLAYERS as u8;
            let ft_players_remaining_exact = played_ft_hand.then_some(fact.seat_count as i32);
            let is_stage_2 = ft_players_remaining_exact == Some(2);
            let is_stage_3_4 = matches!(ft_players_remaining_exact, Some(3 | 4));
            let is_stage_4_5 = matches!(ft_players_remaining_exact, Some(4 | 5));
            let is_stage_5_6 = matches!(ft_players_remaining_exact, Some(5 | 6));
            let is_stage_6_9 = matches!(ft_players_remaining_exact, Some(6..=9));
            let is_boundary_hand = boundary_resolution
                .candidate_hand_ids
                .contains(&fact.hand_id);
            let boundary_is_exact =
                boundary_resolution.resolution_state == "exact" && is_boundary_hand;
            let boundary_ko_value = if boundary_is_exact {
                fact.exact_hero_boundary_ko_share
            } else {
                None
            };

            (
                fact.hand_id.clone(),
                MbrStageResolutionRow {
                    player_profile_id,
                    played_ft_hand,
                    played_ft_hand_state: "exact".to_string(),
                    is_ft_hand: played_ft_hand,
                    ft_players_remaining_exact,
                    is_stage_2,
                    is_stage_3_4,
                    is_stage_4_5,
                    is_stage_5_6,
                    is_stage_6_9,
                    is_boundary_hand,
                    entered_boundary_zone: is_boundary_hand,
                    entered_boundary_zone_state: if boundary_is_exact {
                        "exact".to_string()
                    } else if is_boundary_hand {
                        "estimated".to_string()
                    } else {
                        "exact".to_string()
                    },
                    boundary_resolution_state: boundary_resolution.resolution_state.clone(),
                    boundary_candidate_count,
                    boundary_resolution_method: boundary_resolution.resolution_method.clone(),
                    boundary_confidence_class: boundary_resolution.confidence_class.clone(),
                    ft_table_size: ft_players_remaining_exact,
                    boundary_ko_ev: boundary_ko_value.map(|value| format!("{value:.6}")),
                    boundary_ko_min: boundary_ko_value.map(|value| format!("{value:.6}")),
                    boundary_ko_max: boundary_ko_value.map(|value| format!("{value:.6}")),
                    boundary_ko_method: is_boundary_hand
                        .then_some(boundary_resolution.resolution_method.clone()),
                    boundary_ko_certainty: if boundary_ko_value.is_some() {
                        Some("exact".to_string())
                    } else if is_boundary_hand {
                        Some(boundary_resolution.resolution_state.clone())
                    } else {
                        None
                    },
                    boundary_ko_state: if boundary_ko_value.is_some() {
                        "exact".to_string()
                    } else {
                        "uncertain".to_string()
                    },
                },
            )
        })
        .collect()
}

fn build_tournament_ft_helper_source_hand(
    hand_id: Uuid,
    hand: &CanonicalParsedHand,
    stage_row: &MbrStageResolutionRow,
) -> TournamentFtHelperSourceHand {
    TournamentFtHelperSourceHand {
        hand_id,
        external_hand_id: hand.header.hand_id.clone(),
        hand_started_at_local: hand.header.played_at.clone(),
        played_ft_hand: stage_row.played_ft_hand,
        played_ft_hand_state: stage_row.played_ft_hand_state.clone(),
        ft_table_size: stage_row.ft_table_size,
        entered_boundary_zone: stage_row.entered_boundary_zone,
        boundary_resolution_state: stage_row.boundary_resolution_state.clone(),
        hero_starting_stack: hand.hero_name.as_deref().and_then(|hero_name| {
            hand.seats
                .iter()
                .find(|seat| seat.player_name == hero_name)
                .map(|seat| seat.starting_stack)
        }),
        big_blind: i64::from(hand.header.big_blind),
    }
}

fn build_mbr_tournament_ft_helper_row(
    tournament_id: Uuid,
    player_profile_id: Uuid,
    facts: &[TournamentFtHelperSourceHand],
) -> MbrTournamentFtHelperRow {
    let mut chronological = facts.iter().collect::<Vec<_>>();
    chronological.sort_by(|left, right| {
        left.hand_started_at_local
            .cmp(&right.hand_started_at_local)
            .then_with(|| left.external_hand_id.cmp(&right.external_hand_id))
    });

    let first_ft_hand = chronological
        .iter()
        .find(|fact| fact.played_ft_hand && fact.played_ft_hand_state == "exact")
        .copied();
    let reached_ft_exact = first_ft_hand.is_some();
    let deepest_ft_size_reached = chronological
        .iter()
        .filter(|fact| fact.played_ft_hand && fact.played_ft_hand_state == "exact")
        .filter_map(|fact| fact.ft_table_size)
        .min();
    let entered_boundary_zone = facts.iter().any(|fact| fact.entered_boundary_zone);

    let boundary_resolution_state = {
        let states = facts
            .iter()
            .map(|fact| fact.boundary_resolution_state.as_str())
            .collect::<BTreeSet<_>>();
        if states.len() == 1 {
            states
                .iter()
                .next()
                .copied()
                .unwrap_or("uncertain")
                .to_string()
        } else {
            "inconsistent".to_string()
        }
    };

    let (
        first_ft_hand_id,
        first_ft_hand_started_local,
        first_ft_table_size,
        ft_started_incomplete,
        hero_ft_entry_stack_chips,
        hero_ft_entry_stack_bb,
    ) = match first_ft_hand {
        Some(first_ft_hand) => {
            let first_ft_table_size = first_ft_hand.ft_table_size;
            let hero_ft_entry_stack_bb = match (
                first_ft_hand.hero_starting_stack,
                first_ft_hand.big_blind > 0,
            ) {
                (Some(stack), true) => Some(format!(
                    "{:.6}",
                    stack as f64 / first_ft_hand.big_blind as f64
                )),
                _ => None,
            };

            (
                Some(first_ft_hand.hand_id),
                Some(first_ft_hand.hand_started_at_local.clone()),
                first_ft_table_size,
                first_ft_table_size.map(|table_size| table_size < GG_MBR_FT_MAX_PLAYERS),
                first_ft_hand.hero_starting_stack,
                hero_ft_entry_stack_bb,
            )
        }
        None => (None, None, None, None, None, None),
    };

    MbrTournamentFtHelperRow {
        tournament_id,
        player_profile_id,
        reached_ft_exact,
        first_ft_hand_id,
        first_ft_hand_started_local,
        first_ft_table_size,
        ft_started_incomplete,
        deepest_ft_size_reached,
        hero_ft_entry_stack_chips,
        hero_ft_entry_stack_bb,
        entered_boundary_zone,
        boundary_resolution_state,
    }
}

fn load_tournament_entry_economics(
    tx: &mut impl postgres::GenericClient,
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

fn build_canonical_persistence(hand: &CanonicalParsedHand) -> Result<CanonicalHandPersistence> {
    let mut seat_lookup = BTreeMap::new();
    let mut seat_lookup_by_no = BTreeMap::new();
    let mut seats = Vec::new();
    for seat in &hand.seats {
        seat_lookup.insert(seat.player_name.clone(), seat.seat_no);
        seat_lookup_by_no.insert(seat.seat_no, seat.player_name.clone());
        seats.push(HandSeatRow {
            seat_no: i32::from(seat.seat_no),
            player_name: seat.player_name.clone(),
            starting_stack: seat.starting_stack,
            is_hero: hand.hero_name.as_deref() == Some(seat.player_name.as_str()),
            is_button: seat.seat_no == hand.header.button_seat,
            is_sitting_out: seat.is_sitting_out,
        });
    }

    let positions = build_position_rows(hand)?;

    let mut parse_issues = hand
        .parse_issues
        .iter()
        .map(parse_issue_row)
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
            None => parse_issues.push(error_issue_row(
                ParseIssueCode::HeroCardsMissingSeat,
                format!("hero hole cards exist but hero `{hero_name}` has no seat row"),
                None,
                Some(ParseIssuePayload::HeroCardsMissingSeat {
                    hero_name: hero_name.clone(),
                }),
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
            None => parse_issues.push(error_issue_row(
                ParseIssueCode::ShowdownPlayerMissingSeat,
                format!("showdown hand exists for `{player_name}` without seat row"),
                None,
                Some(ParseIssuePayload::ShowdownPlayerMissingSeat {
                    player_name: player_name.clone(),
                }),
            )),
        }
    }

    let mut summary_seat_outcomes = Vec::new();
    for outcome in &hand.summary_seat_outcomes {
        match seat_lookup_by_no.get(&outcome.seat_no) {
            Some(canonical_player_name) if canonical_player_name == &outcome.player_name => {
                summary_seat_outcomes.push(HandSummarySeatOutcomeRow {
                    seat_no: i32::from(outcome.seat_no),
                    player_name: outcome.player_name.clone(),
                    position_marker: outcome.position_marker.map(|marker| match marker {
                        tracker_parser_core::models::SummarySeatMarker::Button => {
                            "button".to_string()
                        }
                        tracker_parser_core::models::SummarySeatMarker::SmallBlind => {
                            "small blind".to_string()
                        }
                        tracker_parser_core::models::SummarySeatMarker::BigBlind => {
                            "big blind".to_string()
                        }
                    }),
                    outcome_kind: match outcome.outcome_kind {
                        tracker_parser_core::models::SummarySeatOutcomeKind::Folded => {
                            "folded".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::ShowedWon => {
                            "showed_won".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::ShowedLost => {
                            "showed_lost".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::Lost => {
                            "lost".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::Mucked => {
                            "mucked".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::Won => {
                            "won".to_string()
                        }
                        tracker_parser_core::models::SummarySeatOutcomeKind::Collected => {
                            "collected".to_string()
                        }
                    },
                    folded_street: outcome.folded_at.map(street_code).map(str::to_string),
                    shown_cards: outcome.shown_cards.clone(),
                    won_amount: outcome.won_amount,
                    hand_class: outcome.hand_class.clone(),
                    raw_line: outcome.raw_line.clone(),
                })
            }
            Some(canonical_player_name) => parse_issues.push(error_issue_row(
                ParseIssueCode::SummarySeatOutcomeSeatMismatch,
                format!(
                    "summary seat {} references `{}` but canonical seat belongs to `{}`",
                    outcome.seat_no, outcome.player_name, canonical_player_name
                ),
                Some(outcome.raw_line.clone()),
                Some(ParseIssuePayload::SummarySeatOutcomeSeatMismatch {
                    seat_no: outcome.seat_no,
                    player_name: outcome.player_name.clone(),
                    canonical_player_name: canonical_player_name.clone(),
                }),
            )),
            None => parse_issues.push(error_issue_row(
                ParseIssueCode::SummarySeatOutcomeMissingSeat,
                format!(
                    "summary seat {} references `{}` without seat row",
                    outcome.seat_no, outcome.player_name
                ),
                Some(outcome.raw_line.clone()),
                Some(ParseIssuePayload::SummarySeatOutcomeMissingSeat {
                    seat_no: outcome.seat_no,
                    player_name: outcome.player_name.clone(),
                }),
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
            parse_issues.push(error_issue_row(
                ParseIssueCode::ActionPlayerMissingSeat,
                format!("action references `{player_name}` without seat row"),
                Some(event.raw_line.clone()),
                Some(ParseIssuePayload::ActionPlayerMissingSeat {
                    player_name: player_name.clone(),
                    raw_line: event.raw_line.clone(),
                }),
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
            all_in_reason: event
                .all_in_reason
                .map(|reason| reason.as_str().to_string()),
            forced_all_in_preflop: event.forced_all_in_preflop,
            references_previous_bet: matches!(
                event.action_type,
                ActionType::Call | ActionType::RaiseTo
            ),
            raw_line: event.raw_line.clone(),
        });
    }

    Ok(CanonicalHandPersistence {
        seats,
        positions,
        hole_cards: hole_cards_by_seat.into_values().collect(),
        actions,
        board: build_board_row(&hand.board_final),
        showdowns,
        summary_seat_outcomes,
        parse_issues,
    })
}

fn build_position_rows(hand: &CanonicalParsedHand) -> Result<Vec<HandPositionRow>> {
    let position_inputs = hand
        .seats
        .iter()
        .map(|seat| PositionSeatInput {
            seat_no: seat.seat_no,
            is_active: seat.starting_stack > 0 && !seat.is_sitting_out,
        })
        .collect::<Vec<_>>();

    let positions = compute_position_facts(
        hand.header.max_players,
        hand.header.button_seat,
        &position_inputs,
    )
    .map_err(|error| {
        anyhow!(
            "failed to compute positions for hand {}: {error}",
            hand.header.hand_id
        )
    })?;

    Ok(positions
        .into_iter()
        .map(|position| HandPositionRow {
            seat_no: i32::from(position.seat_no),
            position_index: i32::from(position.position_index),
            position_label: position.position_label.as_str().to_string(),
            preflop_act_order_index: i32::from(position.preflop_act_order_index),
            postflop_act_order_index: i32::from(position.postflop_act_order_index),
        })
        .collect())
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

fn tournament_summary_parse_issues(
    summary: &tracker_parser_core::models::TournamentSummary,
) -> Vec<ParseIssueRow> {
    summary
        .parse_issues
        .iter()
        .map(parse_issue_row)
        .collect()
}

fn parse_issue_row(issue: &ParseIssue) -> ParseIssueRow {
    ParseIssueRow {
        severity: issue.severity.as_str().to_string(),
        code: issue.code.as_str().to_string(),
        message: issue.message.clone(),
        raw_line: issue.raw_line.clone(),
        payload: issue_payload_json(issue),
    }
}

fn error_issue_row(
    code: ParseIssueCode,
    message: String,
    raw_line: Option<String>,
    payload: Option<ParseIssuePayload>,
) -> ParseIssueRow {
    parse_issue_row(&ParseIssue::error(code, message, raw_line, payload))
}

fn issue_payload_json(issue: &ParseIssue) -> serde_json::Value {
    issue
        .payload
        .as_ref()
        .map(|payload| serde_json::to_value(payload).expect("parse issue payload must serialize"))
        .unwrap_or_else(|| serde_json::json!({}))
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

fn format_fraction_value(value: f64) -> String {
    format!("{value:.6}")
}

fn exact_hero_boundary_ko_share(
    hand: &CanonicalParsedHand,
    normalized_hand: &tracker_parser_core::models::NormalizedHand,
) -> Option<f64> {
    let hero_name = hand.hero_name.as_deref()?;

    normalized_hand
        .eliminations
        .iter()
        .filter(|elimination| elimination.ko_certainty_state == CertaintyState::Exact)
        .filter_map(|elimination| hero_ko_share_fraction(elimination, hero_name))
        .reduce(|accumulator, share| accumulator + share)
}

fn hero_ko_share_fraction(
    elimination: &tracker_parser_core::models::HandElimination,
    hero_name: &str,
) -> Option<f64> {
    elimination
        .ko_share_fraction_by_winner
        .iter()
        .find(|share| share.player_name == hero_name)
        .map(|share| share.share_fraction)
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
    tx: &mut impl postgres::GenericClient,
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
    tx: &mut impl postgres::GenericClient,
    source_file_id: Uuid,
    source_file_member_id: Uuid,
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
                source_file_member_id,
                fragment_index,
                external_hand_id,
                kind,
                raw_text,
                sha256
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (source_file_member_id, fragment_index)
            DO UPDATE SET
                external_hand_id = EXCLUDED.external_hand_id,
                kind = EXCLUDED.kind,
                raw_text = EXCLUDED.raw_text,
                sha256 = EXCLUDED.sha256
            RETURNING id",
            &[
                &source_file_id,
                &source_file_member_id,
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
    use mbr_stats_runtime::big_ko::{
        expected_big_ko_bucket_probabilities, expected_hero_mystery_cents,
    };
    use mbr_stats_runtime::{
        CanonicalStatNumericValue, CanonicalStatState, MysteryEnvelope, SeedStatsFilters,
        query_canonical_stats, query_seed_stats,
    };
    use std::{
        io::Write,
        path::PathBuf,
        sync::{Mutex, OnceLock},
    };
    use tracker_query_runtime::{
        FeatureRef, FilterCondition, FilterOperator, FilterValue, HandQueryRequest,
        query_matching_hand_ids,
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
            .start_file(
                "nested/member.hh",
                zip::write::SimpleFileOptions::default(),
            )
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

        let (logical_path, input) = load_ingest_job_input(&job).unwrap();

        assert_eq!(logical_path, "nested/member.hh".to_string());
        assert_eq!(input, "hello from zip member".to_string());

        fs::remove_file(archive_path).unwrap();
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
        hand.parse_issues.push(tracker_parser_core::models::ParseIssue {
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
        assert_eq!(normalized.eliminations[0].ko_winner_set, vec!["Hero".to_string()]);

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
                vec![tracker_parser_core::models::PotSettlementIssue::AmbiguousHiddenShowdown {
                    eligible_players: vec!["Hero".to_string(), "Villain".to_string()],
                }],
                vec![tracker_parser_core::models::PotSettlementIssue::AmbiguousHiddenShowdown {
                    eligible_players: vec!["Hero".to_string(), "Villain".to_string()],
                }],
            ]
        );
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
            assert!(!row.entered_boundary_zone, "no boundary zone in no-FT tournament");
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
            &[
                TournamentFtHelperSourceHand {
                    hand_id: Uuid::from_u128(1),
                    external_hand_id: "ft-1".to_string(),
                    hand_started_at_local: "2026/03/16 10:00:00".to_string(),
                    played_ft_hand: true,
                    played_ft_hand_state: "exact".to_string(),
                    ft_table_size: Some(7),
                    entered_boundary_zone: false,
                    boundary_resolution_state: "exact".to_string(),
                    hero_starting_stack: Some(5000),
                    big_blind: 200,
                },
            ],
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
        let helper_source_hands = canonical_hands
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
        assert_eq!(elimination.get::<_, Option<String>>(6).as_deref(), Some("7"));
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
        assert_eq!(hero_flop_street_strength.get::<_, Option<bool>>(8), Some(false));
        assert_eq!(hero_flop_street_strength.get::<_, Option<bool>>(9), Some(false));
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
        assert_eq!(elimination.get::<_, Option<String>>(6).as_deref(), Some("1"));
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
        let stage_attempt_counts = client
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
        let stage_entry_values = client
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
        let bucket_probabilities = expected_big_ko_bucket_probabilities(&mystery_envelopes);
        let mut expected_ko_money_total = 0.0;
        let mut expected_ko_stage_2_3_money_total = 0.0;
        let mut expected_ko_stage_3_4_money_total = 0.0;
        let mut expected_ko_stage_4_5_money_total = 0.0;
        let mut expected_ko_stage_5_6_money_total = 0.0;
        let mut expected_ko_stage_6_9_money_total = 0.0;
        let mut expected_ko_stage_7_9_money_total = 0.0;
        let mut expected_big_ko_x1_5_count = 0.0;
        let mut expected_big_ko_x2_count = 0.0;
        let mut expected_big_ko_x10_count = 0.0;
        let mut expected_big_ko_x100_count = 0.0;
        let mut expected_big_ko_x1000_count = 0.0;
        let mut expected_big_ko_x10000_count = 0.0;
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
            expected_big_ko_x1_5_count += bucket_probabilities
                .get("big_ko_x1_5_count")
                .copied()
                .unwrap_or(0.0);
            expected_big_ko_x2_count += bucket_probabilities
                .get("big_ko_x2_count")
                .copied()
                .unwrap_or(0.0);
            expected_big_ko_x10_count += bucket_probabilities
                .get("big_ko_x10_count")
                .copied()
                .unwrap_or(0.0);
            expected_big_ko_x100_count += bucket_probabilities
                .get("big_ko_x100_count")
                .copied()
                .unwrap_or(0.0);
            expected_big_ko_x1000_count += bucket_probabilities
                .get("big_ko_x1000_count")
                .copied()
                .unwrap_or(0.0);
            expected_big_ko_x10000_count += bucket_probabilities
                .get("big_ko_x10000_count")
                .copied()
                .unwrap_or(0.0);
        }
        let pre_ft_chip_delta: f64 = client
            .query_one(
                "SELECT
                    COALESCE(pre_ft_snapshot.hero_final_stack, 1000::bigint) - 1000::bigint
                 FROM derived.mbr_tournament_ft_helper helper
                 LEFT JOIN LATERAL (
                    SELECT
                        (resolution.final_stacks ->> hero.player_name)::bigint AS hero_final_stack
                    FROM core.hands h
                    INNER JOIN core.hand_seats hero
                      ON hero.hand_id = h.id
                     AND hero.is_hero IS TRUE
                    INNER JOIN derived.hand_state_resolutions resolution
                      ON resolution.hand_id = h.id
                     AND resolution.resolution_version = $3
                    LEFT JOIN derived.mbr_stage_resolution msr
                      ON msr.hand_id = h.id
                     AND msr.player_profile_id = h.player_profile_id
                    WHERE h.tournament_id = helper.tournament_id
                      AND h.player_profile_id = helper.player_profile_id
                      AND (
                          helper.first_ft_hand_id IS NULL
                          OR (
                              helper.boundary_resolution_state = 'exact'
                              AND h.tournament_hand_order IS NOT NULL
                              AND h.tournament_hand_order < (
                                  SELECT fh.tournament_hand_order
                                  FROM core.hands fh
                                  WHERE fh.id = helper.first_ft_hand_id
                              )
                              AND COALESCE(msr.is_boundary_hand, FALSE) IS FALSE
                          )
                      )
                    ORDER BY
                        h.tournament_hand_order DESC NULLS LAST,
                        h.id DESC
                    LIMIT 1
                 ) AS pre_ft_snapshot
                   ON TRUE
                 WHERE helper.tournament_id = $1
                   AND helper.player_profile_id = $2
                   AND (
                       helper.first_ft_hand_started_local IS NULL
                       OR helper.boundary_resolution_state = 'exact'
                   )",
                &[
                    &ts_report.tournament_id,
                    &player_profile_id,
                    &HAND_RESOLUTION_VERSION,
                ],
            )
            .unwrap()
            .get::<_, i64>(0) as f64;
        let regular_prize_money = economics
            .get::<_, Option<String>>(0)
            .unwrap()
            .parse::<f64>()
            .unwrap();
        let total_payout_money = economics
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
            canonical_stats.values["winnings_from_ko_total"].value,
            Some(CanonicalStatNumericValue::Float(
                economics
                    .get::<_, Option<String>>(2)
                    .unwrap()
                    .parse::<f64>()
                    .unwrap(),
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_contribution_percent"].value,
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
            &canonical_stats.values["ko_contribution_adjusted_percent"].value,
            expected_ko_money_total / total_payout_money * 100.0,
            "ko_contribution_adjusted_percent",
        );
        assert_canonical_float_close(
            &canonical_stats.values["ko_luck_money_delta"].value,
            mystery_money_total - expected_ko_money_total,
            "ko_luck_money_delta",
        );
        assert_canonical_float_close(
            &canonical_stats.values["roi_adj_pct"].value,
            ((regular_prize_money + expected_ko_money_total - buyin_total_money)
                / buyin_total_money)
                * 100.0,
            "roi_adj_pct",
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
            canonical_stats.values["ko_stage_2_3_event_count"].value,
            Some(CanonicalStatNumericValue::Integer(
                stage_event_counts.get::<_, i64>(1) as u64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_2_3_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_2_3_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_3_4_event_count"].value,
            Some(CanonicalStatNumericValue::Integer(
                stage_event_counts.get::<_, i64>(2) as u64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_3_4_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_3_4_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_4_5_event_count"].value,
            Some(CanonicalStatNumericValue::Integer(
                stage_event_counts.get::<_, i64>(3) as u64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_4_5_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_4_5_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_5_6_event_count"].value,
            Some(CanonicalStatNumericValue::Integer(
                stage_event_counts.get::<_, i64>(4) as u64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_5_6_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_5_6_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_6_9_event_count"].value,
            Some(CanonicalStatNumericValue::Integer(
                stage_event_counts.get::<_, i64>(5) as u64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_6_9_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_6_9_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_7_9_event_count"].value,
            Some(CanonicalStatNumericValue::Integer(
                stage_event_counts.get::<_, i64>(6) as u64
            ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_7_9_money_total"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_ko_stage_7_9_money_total,
            ))
        );
        assert_eq!(
            canonical_stats.values["pre_ft_ko_count"].value,
            Some(CanonicalStatNumericValue::Integer(
                stage_event_counts.get::<_, i64>(7) as u64
            ))
        );
        assert_eq!(
            canonical_stats.values["pre_ft_chipev"].value,
            Some(CanonicalStatNumericValue::Float(pre_ft_chip_delta))
        );
        assert_eq!(
            canonical_stats.values["ft_stack_conversion"].value,
            stage_entry_values
                .get::<_, Option<f64>>(4)
                .filter(|denominator| *denominator > 0.0)
                .map(|denominator| {
                    CanonicalStatNumericValue::Float(
                        stage_event_counts.get::<_, i64>(5) as f64 / denominator,
                    )
                })
        );
        assert_eq!(
            canonical_stats.values["avg_ko_attempts_per_ft"].value,
            if stage_entry_values.get::<_, bool>(0) {
                Some(CanonicalStatNumericValue::Float(
                    stage_attempt_counts.get::<_, i64>(4) as f64,
                ))
            } else {
                None
            }
        );
        assert_eq!(
            canonical_stats.values["ko_attempts_success_rate"].value,
            match stage_attempt_counts.get::<_, i64>(4) {
                0 => None,
                attempt_count => Some(CanonicalStatNumericValue::Float(
                    stage_event_counts.get::<_, i64>(5) as f64 / attempt_count as f64,
                )),
            }
        );
        assert_eq!(
            canonical_stats.values["ft_stack_conversion_7_9"].value,
            stage_entry_values
                .get::<_, Option<f64>>(4)
                .filter(|denominator| *denominator > 0.0)
                .map(|denominator| {
                    CanonicalStatNumericValue::Float(
                        stage_event_counts.get::<_, i64>(5) as f64 / denominator,
                    )
                })
        );
        assert_eq!(
            canonical_stats.values["ft_stack_conversion_7_9_attempts"].value,
            if stage_entry_values.get::<_, bool>(0) {
                Some(CanonicalStatNumericValue::Float(
                    stage_attempt_counts.get::<_, i64>(4) as f64,
                ))
            } else {
                None
            }
        );
        assert_eq!(
            canonical_stats.values["ft_stack_conversion_5_6"].value,
            stage_entry_values
                .get::<_, Option<f64>>(5)
                .filter(|denominator| *denominator > 0.0)
                .map(|denominator| {
                    CanonicalStatNumericValue::Float(
                        stage_event_counts.get::<_, i64>(4) as f64 / denominator,
                    )
                })
        );
        assert_eq!(
            canonical_stats.values["ft_stack_conversion_5_6_attempts"].value,
            stage_entry_values
                .get::<_, Option<f64>>(5)
                .map(|_| CanonicalStatNumericValue::Float(
                    stage_attempt_counts.get::<_, i64>(3) as f64
                ))
        );
        assert_eq!(
            canonical_stats.values["ft_stack_conversion_3_4"].value,
            stage_entry_values
                .get::<_, Option<f64>>(6)
                .filter(|denominator| *denominator > 0.0)
                .map(|denominator| {
                    CanonicalStatNumericValue::Float(
                        stage_event_counts.get::<_, i64>(2) as f64 / denominator,
                    )
                })
        );
        assert_eq!(
            canonical_stats.values["ft_stack_conversion_3_4_attempts"].value,
            stage_entry_values
                .get::<_, Option<f64>>(6)
                .map(|_| CanonicalStatNumericValue::Float(
                    stage_attempt_counts.get::<_, i64>(1) as f64
                ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_2_3_attempts_per_tournament"].value,
            if stage_entry_values.get::<_, bool>(1) {
                Some(CanonicalStatNumericValue::Float(
                    stage_attempt_counts.get::<_, i64>(0) as f64,
                ))
            } else {
                None
            }
        );
        assert_eq!(
            canonical_stats.values["ko_stage_3_4_attempts_per_tournament"].value,
            stage_entry_values
                .get::<_, Option<f64>>(6)
                .map(|_| CanonicalStatNumericValue::Float(
                    stage_attempt_counts.get::<_, i64>(1) as f64
                ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_4_5_attempts_per_tournament"].value,
            if stage_entry_values.get::<_, bool>(2) {
                Some(CanonicalStatNumericValue::Float(
                    stage_attempt_counts.get::<_, i64>(2) as f64,
                ))
            } else {
                None
            }
        );
        assert_eq!(
            canonical_stats.values["ko_stage_5_6_attempts_per_tournament"].value,
            stage_entry_values
                .get::<_, Option<f64>>(5)
                .map(|_| CanonicalStatNumericValue::Float(
                    stage_attempt_counts.get::<_, i64>(3) as f64
                ))
        );
        assert_eq!(
            canonical_stats.values["ko_stage_7_9_attempts_per_tournament"].value,
            if stage_entry_values.get::<_, bool>(3) {
                Some(CanonicalStatNumericValue::Float(
                    stage_attempt_counts.get::<_, i64>(5) as f64,
                ))
            } else {
                None
            }
        );
        assert_eq!(
            canonical_stats.values["big_ko_x1_5_count"].value,
            Some(CanonicalStatNumericValue::Float(expected_big_ko_x1_5_count))
        );
        assert_eq!(
            canonical_stats.values["big_ko_x2_count"].value,
            Some(CanonicalStatNumericValue::Float(expected_big_ko_x2_count))
        );
        assert_eq!(
            canonical_stats.values["big_ko_x10_count"].value,
            Some(CanonicalStatNumericValue::Float(expected_big_ko_x10_count))
        );
        assert_eq!(
            canonical_stats.values["big_ko_x100_count"].value,
            Some(CanonicalStatNumericValue::Float(expected_big_ko_x100_count))
        );
        assert_eq!(
            canonical_stats.values["big_ko_x1000_count"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_big_ko_x1000_count
            ))
        );
        assert_eq!(
            canonical_stats.values["big_ko_x10000_count"].value,
            Some(CanonicalStatNumericValue::Float(
                expected_big_ko_x10000_count
            ))
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
    }
}
