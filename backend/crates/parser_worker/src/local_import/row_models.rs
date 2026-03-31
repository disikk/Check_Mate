use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use tracker_ingest_runtime::{
    FailureDisposition, IngestFileInput, JobExecutionError,
};
use tracker_parser_core::models::{
    CanonicalParsedHand, HandSettlement, InvariantIssue, TournamentSummary,
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedTournamentSummaryImport {
    pub(crate) summary: TournamentSummary,
    pub(crate) parse_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedHandHistoryImport {
    pub(crate) hands: Vec<tracker_parser_core::models::HandRecord>,
    pub(crate) canonical_hands: Vec<CanonicalParsedHand>,
    pub(crate) hand_local_outputs: Vec<HandLocalComputeOutput>,
    pub(crate) ordered_stage_resolutions: Vec<MbrStageResolutionRow>,
    pub(crate) parse_ms: u64,
    pub(crate) normalize_ms: u64,
    pub(crate) derive_hand_local_ms: u64,
    pub(crate) derive_tournament_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PreparedRuntimeFileJob {
    TournamentSummary {
        input: String,
        prepared: PreparedTournamentSummaryImport,
    },
    HandHistory(PreparedHandHistoryImport),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionFailure {
    pub(crate) disposition: FailureDisposition,
    pub(crate) error_code: String,
}

impl ExecutionFailure {
    pub(crate) fn terminal(error_code: impl Into<String>) -> Self {
        Self {
            disposition: FailureDisposition::Terminal,
            error_code: error_code.into(),
        }
    }

    pub(crate) fn retriable(error_code: impl Into<String>) -> Self {
        Self {
            disposition: FailureDisposition::Retriable,
            error_code: error_code.into(),
        }
    }
}

impl From<JobExecutionError> for ExecutionFailure {
    fn from(error: JobExecutionError) -> Self {
        match error.disposition() {
            FailureDisposition::Retriable => Self::retriable(error.error_code()),
            FailureDisposition::Terminal => Self::terminal(error.error_code()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MaterializedPreparedArchive {
    pub(crate) archive_path: std::path::PathBuf,
    pub(crate) ingest_file: IngestFileInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportContext {
    pub(crate) organization_id: Uuid,
    pub(crate) user_id: Uuid,
    pub(crate) player_profile_id: Uuid,
    pub(crate) player_aliases: Vec<String>,
    pub(crate) timezone_name: Option<String>,
    pub(crate) room_id: Uuid,
    pub(crate) format_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CanonicalHandPersistence {
    pub(crate) seats: Vec<HandSeatRow>,
    pub(crate) positions: Vec<HandPositionRow>,
    pub(crate) hole_cards: Vec<HandHoleCardsRow>,
    pub(crate) actions: Vec<HandActionRow>,
    pub(crate) board: Option<HandBoardRow>,
    pub(crate) showdowns: Vec<HandShowdownRow>,
    pub(crate) summary_seat_outcomes: Vec<HandSummarySeatOutcomeRow>,
    pub(crate) parse_issues: Vec<ParseIssueRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedHandPersistence {
    pub(crate) state_resolution: HandStateResolutionRow,
    pub(crate) pot_rows: Vec<HandPotRow>,
    pub(crate) eligibility_rows: Vec<HandPotEligibilityRow>,
    pub(crate) contribution_rows: Vec<HandPotContributionRow>,
    pub(crate) winner_rows: Vec<HandPotWinnerRow>,
    pub(crate) return_rows: Vec<HandReturnRow>,
    pub(crate) elimination_rows: Vec<HandEliminationRow>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HandLocalComputeOutput {
    pub(crate) canonical_persistence: CanonicalHandPersistence,
    pub(crate) normalized_persistence: NormalizedHandPersistence,
    pub(crate) ko_attempt_rows: Vec<HandKoAttemptRow>,
    pub(crate) ko_opportunity_rows: Vec<HandKoOpportunityRow>,
    pub(crate) preflop_starting_hand_rows: Vec<PreflopStartingHandRow>,
    pub(crate) street_strength_rows: Vec<StreetHandStrengthRow>,
    pub(crate) stage_fact: StageHandFact,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandSeatRow {
    pub(crate) seat_no: i32,
    pub(crate) player_name: String,
    pub(crate) starting_stack: i64,
    pub(crate) is_hero: bool,
    pub(crate) is_button: bool,
    pub(crate) is_sitting_out: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandPositionRow {
    pub(crate) seat_no: i32,
    pub(crate) position_index: i32,
    pub(crate) position_label: String,
    pub(crate) preflop_act_order_index: i32,
    pub(crate) postflop_act_order_index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandHoleCardsRow {
    pub(crate) seat_no: i32,
    pub(crate) card1: Option<String>,
    pub(crate) card2: Option<String>,
    pub(crate) known_to_hero: bool,
    pub(crate) known_at_showdown: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandActionRow {
    pub(crate) sequence_no: i32,
    pub(crate) street: String,
    pub(crate) seat_no: Option<i32>,
    pub(crate) action_type: String,
    pub(crate) raw_amount: Option<i64>,
    pub(crate) to_amount: Option<i64>,
    pub(crate) is_all_in: bool,
    pub(crate) all_in_reason: Option<String>,
    pub(crate) forced_all_in_preflop: bool,
    pub(crate) references_previous_bet: bool,
    pub(crate) raw_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandBoardRow {
    pub(crate) flop1: Option<String>,
    pub(crate) flop2: Option<String>,
    pub(crate) flop3: Option<String>,
    pub(crate) turn: Option<String>,
    pub(crate) river: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandShowdownRow {
    pub(crate) seat_no: i32,
    pub(crate) shown_cards: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandSummarySeatOutcomeRow {
    pub(crate) seat_no: i32,
    pub(crate) player_name: String,
    pub(crate) position_marker: Option<String>,
    pub(crate) outcome_kind: String,
    pub(crate) folded_street: Option<String>,
    pub(crate) shown_cards: Option<Vec<String>>,
    pub(crate) won_amount: Option<i64>,
    pub(crate) hand_class: Option<String>,
    pub(crate) raw_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParseIssueRow {
    pub(crate) severity: String,
    pub(crate) code: String,
    pub(crate) message: String,
    pub(crate) raw_line: Option<String>,
    pub(crate) payload: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandStateResolutionRow {
    pub(crate) resolution_version: String,
    pub(crate) chip_conservation_ok: bool,
    pub(crate) pot_conservation_ok: bool,
    pub(crate) settlement_state: String,
    pub(crate) rake_amount: i64,
    pub(crate) final_stacks: BTreeMap<String, i64>,
    pub(crate) settlement: HandSettlement,
    pub(crate) invariant_issues: Vec<InvariantIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandPotRow {
    pub(crate) pot_no: i32,
    pub(crate) pot_type: String,
    pub(crate) amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandPotEligibilityRow {
    pub(crate) pot_no: i32,
    pub(crate) seat_no: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandPotContributionRow {
    pub(crate) pot_no: i32,
    pub(crate) seat_no: i32,
    pub(crate) amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandPotWinnerRow {
    pub(crate) pot_no: i32,
    pub(crate) seat_no: i32,
    pub(crate) share_amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandReturnRow {
    pub(crate) seat_no: i32,
    pub(crate) amount: i64,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct HandEliminationKoShareRow {
    pub(crate) seat_no: i32,
    pub(crate) player_name: String,
    pub(crate) share_fraction: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandEliminationRow {
    pub(crate) eliminated_seat_no: i32,
    pub(crate) eliminated_player_name: String,
    pub(crate) pots_participated_by_busted: Vec<i32>,
    pub(crate) pots_causing_bust: Vec<i32>,
    pub(crate) last_busting_pot_no: Option<i32>,
    pub(crate) ko_winner_set: Vec<String>,
    pub(crate) ko_share_fraction_by_winner: Vec<HandEliminationKoShareRow>,
    pub(crate) elimination_certainty_state: String,
    pub(crate) ko_certainty_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandKoAttemptRow {
    pub(crate) hero_seat_no: i32,
    pub(crate) target_seat_no: i32,
    pub(crate) target_player_name: String,
    pub(crate) attempt_kind: String,
    pub(crate) street: String,
    pub(crate) source_sequence_no: i32,
    pub(crate) is_forced_all_in: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandKoOpportunityRow {
    pub(crate) hero_seat_no: i32,
    pub(crate) target_seat_no: i32,
    pub(crate) target_player_name: String,
    pub(crate) opportunity_kind: String,
    pub(crate) street: String,
    pub(crate) source_sequence_no: i32,
    pub(crate) is_forced_all_in: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MbrStageResolutionRow {
    pub(crate) player_profile_id: Uuid,
    pub(crate) played_ft_hand: bool,
    pub(crate) played_ft_hand_state: String,
    pub(crate) is_ft_hand: bool,
    pub(crate) ft_players_remaining_exact: Option<i32>,
    pub(crate) is_stage_2: bool,
    pub(crate) is_stage_3_4: bool,
    pub(crate) is_stage_4_5: bool,
    pub(crate) is_stage_5_6: bool,
    pub(crate) is_stage_6_9: bool,
    pub(crate) is_boundary_hand: bool,
    pub(crate) entered_boundary_zone: bool,
    pub(crate) entered_boundary_zone_state: String,
    pub(crate) boundary_resolution_state: String,
    pub(crate) boundary_candidate_count: i32,
    pub(crate) boundary_resolution_method: String,
    pub(crate) boundary_confidence_class: String,
    pub(crate) ft_table_size: Option<i32>,
    pub(crate) boundary_ko_ev: Option<String>,
    pub(crate) boundary_ko_min: Option<String>,
    pub(crate) boundary_ko_max: Option<String>,
    pub(crate) boundary_ko_method: Option<String>,
    pub(crate) boundary_ko_certainty: Option<String>,
    pub(crate) boundary_ko_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MbrTournamentFtHelperRow {
    pub(crate) tournament_id: Uuid,
    pub(crate) player_profile_id: Uuid,
    pub(crate) reached_ft_exact: bool,
    pub(crate) first_ft_hand_id: Option<Uuid>,
    pub(crate) first_ft_hand_started_local: Option<String>,
    pub(crate) first_ft_table_size: Option<i32>,
    pub(crate) ft_started_incomplete: Option<bool>,
    pub(crate) deepest_ft_size_reached: Option<i32>,
    pub(crate) hero_ft_entry_stack_chips: Option<i64>,
    pub(crate) hero_ft_entry_stack_bb: Option<String>,
    pub(crate) entered_boundary_zone: bool,
    pub(crate) boundary_resolution_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TournamentEntryEconomics {
    pub(crate) regular_prize_cents: i64,
    pub(crate) mystery_money_cents: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StageHandFact {
    pub(crate) hand_id: String,
    pub(crate) played_at: String,
    pub(crate) max_players: u8,
    pub(crate) seat_count: usize,
    pub(crate) exact_hero_boundary_ko_share: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundaryResolution {
    pub(crate) candidate_hand_ids: BTreeSet<String>,
    pub(crate) resolution_state: String,
    pub(crate) resolution_method: String,
    pub(crate) confidence_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TournamentFtHelperSourceHand {
    pub(crate) hand_id: Uuid,
    pub(crate) tournament_hand_order: i32,
    pub(crate) external_hand_id: String,
    pub(crate) hand_started_at_local: String,
    pub(crate) played_ft_hand: bool,
    pub(crate) played_ft_hand_state: String,
    pub(crate) ft_table_size: Option<i32>,
    pub(crate) entered_boundary_zone: bool,
    pub(crate) boundary_resolution_state: String,
    pub(crate) hero_starting_stack: Option<i64>,
    pub(crate) big_blind: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StreetHandStrengthRow {
    pub(crate) seat_no: i32,
    pub(crate) street: String,
    pub(crate) best_hand_class: String,
    pub(crate) best_hand_rank_value: i64,
    pub(crate) made_hand_category: String,
    pub(crate) draw_category: String,
    pub(crate) overcards_count: i32,
    pub(crate) has_air: bool,
    pub(crate) missed_flush_draw: bool,
    pub(crate) missed_straight_draw: bool,
    pub(crate) is_nut_hand: Option<bool>,
    pub(crate) is_nut_draw: Option<bool>,
    pub(crate) certainty_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreflopStartingHandRow {
    pub(crate) seat_no: i32,
    pub(crate) starter_hand_class: String,
    pub(crate) certainty_state: String,
}
