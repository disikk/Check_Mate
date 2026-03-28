use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TournamentSummary {
    pub tournament_id: u64,
    pub tournament_name: String,
    pub game_name: String,
    pub buy_in_cents: i64,
    pub rake_cents: i64,
    pub bounty_cents: i64,
    pub entrants: u32,
    pub total_prize_pool_cents: i64,
    pub started_at: String,
    pub hero_name: String,
    pub finish_place: u32,
    pub payout_cents: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_finish_place: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmed_payout_cents: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parse_issues: Vec<ParseIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandHeader {
    pub hand_id: String,
    pub tournament_id: u64,
    pub game_name: String,
    pub level_name: String,
    pub small_blind: u32,
    pub big_blind: u32,
    pub ante: u32,
    pub played_at: String,
    pub table_name: String,
    pub max_players: u8,
    pub button_seat: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandRecord {
    pub header: HandHeader,
    pub raw_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Street {
    Preflop,
    Flop,
    Turn,
    River,
    Showdown,
    Summary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionType {
    PostAnte,
    PostSb,
    PostBb,
    PostDead,
    Fold,
    Check,
    Call,
    Bet,
    RaiseTo,
    ReturnUncalled,
    Collect,
    Show,
    Muck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedHandSeat {
    pub seat_no: u8,
    pub player_name: String,
    pub starting_stack: i64,
    pub is_sitting_out: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AllInReason {
    Voluntary,
    CallExhausted,
    RaiseExhausted,
    BlindExhausted,
    AnteExhausted,
}

impl AllInReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Voluntary => "voluntary",
            Self::CallExhausted => "call_exhausted",
            Self::RaiseExhausted => "raise_exhausted",
            Self::BlindExhausted => "blind_exhausted",
            Self::AnteExhausted => "ante_exhausted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PositionLabel {
    #[serde(rename = "BTN")]
    Btn,
    #[serde(rename = "SB")]
    Sb,
    #[serde(rename = "BB")]
    Bb,
    #[serde(rename = "UTG")]
    Utg,
    #[serde(rename = "UTG+1")]
    UtgPlus1,
    #[serde(rename = "UTG+2")]
    UtgPlus2,
    #[serde(rename = "MP")]
    Mp,
    #[serde(rename = "MP+1")]
    MpPlus1,
    #[serde(rename = "LJ")]
    Lj,
    #[serde(rename = "HJ")]
    Hj,
    #[serde(rename = "CO")]
    Co,
}

impl PositionLabel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Btn => "BTN",
            Self::Sb => "SB",
            Self::Bb => "BB",
            Self::Utg => "UTG",
            Self::UtgPlus1 => "UTG+1",
            Self::UtgPlus2 => "UTG+2",
            Self::Mp => "MP",
            Self::MpPlus1 => "MP+1",
            Self::Lj => "LJ",
            Self::Hj => "HJ",
            Self::Co => "CO",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandPosition {
    pub seat_no: u8,
    pub position_index: u8,
    pub position_label: PositionLabel,
    pub preflop_act_order_index: u8,
    pub postflop_act_order_index: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SummarySeatMarker {
    Button,
    SmallBlind,
    BigBlind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SummarySeatOutcomeKind {
    Folded,
    ShowedWon,
    ShowedLost,
    Lost,
    Mucked,
    Won,
    Collected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SummarySeatOutcome {
    pub seat_no: u8,
    pub player_name: String,
    pub position_marker: Option<SummarySeatMarker>,
    pub outcome_kind: SummarySeatOutcomeKind,
    pub folded_at: Option<Street>,
    pub shown_cards: Option<Vec<String>>,
    pub won_amount: Option<i64>,
    pub hand_class: Option<String>,
    pub raw_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandActionEvent {
    pub seq: usize,
    pub street: Street,
    pub player_name: Option<String>,
    pub action_type: ActionType,
    pub is_forced: bool,
    pub is_all_in: bool,
    pub all_in_reason: Option<AllInReason>,
    pub forced_all_in_preflop: bool,
    pub amount: Option<i64>,
    pub to_amount: Option<i64>,
    pub cards: Option<Vec<String>>,
    pub raw_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CanonicalParsedHand {
    pub header: HandHeader,
    pub hero_name: Option<String>,
    pub seats: Vec<ParsedHandSeat>,
    pub actions: Vec<HandActionEvent>,
    pub board_final: Vec<String>,
    pub summary_total_pot: Option<i64>,
    pub summary_rake_amount: Option<i64>,
    pub summary_board: Vec<String>,
    pub hero_hole_cards: Option<Vec<String>>,
    pub showdown_hands: BTreeMap<String, Vec<String>>,
    pub summary_seat_outcomes: Vec<SummarySeatOutcome>,
    pub collected_amounts: BTreeMap<String, i64>,
    pub raw_hand_text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parse_issues: Vec<ParseIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseIssueSeverity {
    Info,
    Warning,
    Error,
}

impl ParseIssueSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseIssueCode {
    UnparsedLine,
    UnparsedSummarySeatLine,
    UnparsedSummarySeatTail,
    UnsupportedNoShowLine,
    PartialRevealShowLine,
    PartialRevealSummaryShowSurface,
    TsTailFinishPlaceMismatch,
    TsTailTotalReceivedMismatch,
    ParserWarning,
    HeroCardsMissingSeat,
    ShowdownPlayerMissingSeat,
    SummarySeatOutcomeSeatMismatch,
    SummarySeatOutcomeMissingSeat,
    ActionPlayerMissingSeat,
}

impl ParseIssueCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UnparsedLine => "unparsed_line",
            Self::UnparsedSummarySeatLine => "unparsed_summary_seat_line",
            Self::UnparsedSummarySeatTail => "unparsed_summary_seat_tail",
            Self::UnsupportedNoShowLine => "unsupported_no_show_line",
            Self::PartialRevealShowLine => "partial_reveal_show_line",
            Self::PartialRevealSummaryShowSurface => "partial_reveal_summary_show_surface",
            Self::TsTailFinishPlaceMismatch => "ts_tail_finish_place_mismatch",
            Self::TsTailTotalReceivedMismatch => "ts_tail_total_received_mismatch",
            Self::ParserWarning => "parser_warning",
            Self::HeroCardsMissingSeat => "hero_cards_missing_seat",
            Self::ShowdownPlayerMissingSeat => "showdown_player_missing_seat",
            Self::SummarySeatOutcomeSeatMismatch => "summary_seat_outcome_seat_mismatch",
            Self::SummarySeatOutcomeMissingSeat => "summary_seat_outcome_missing_seat",
            Self::ActionPlayerMissingSeat => "action_player_missing_seat",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ParseIssuePayload {
    RawLine {
        raw_line: String,
    },
    TsTailFinishPlaceMismatch {
        result_finish_place: u32,
        tail_finish_place: u32,
    },
    TsTailTotalReceivedMismatch {
        result_payout_cents: i64,
        tail_payout_cents: i64,
    },
    HeroCardsMissingSeat {
        hero_name: String,
    },
    ShowdownPlayerMissingSeat {
        player_name: String,
    },
    SummarySeatOutcomeSeatMismatch {
        seat_no: u8,
        player_name: String,
        canonical_player_name: String,
    },
    SummarySeatOutcomeMissingSeat {
        seat_no: u8,
        player_name: String,
    },
    ActionPlayerMissingSeat {
        player_name: String,
        raw_line: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParseIssue {
    pub severity: ParseIssueSeverity,
    pub code: ParseIssueCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_line: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<ParseIssuePayload>,
}

impl ParseIssue {
    pub fn new(
        severity: ParseIssueSeverity,
        code: ParseIssueCode,
        message: String,
        raw_line: Option<String>,
        payload: Option<ParseIssuePayload>,
    ) -> Self {
        Self {
            severity,
            code,
            message,
            raw_line,
            payload,
        }
    }

    pub fn warning(
        code: ParseIssueCode,
        message: String,
        raw_line: Option<String>,
        payload: Option<ParseIssuePayload>,
    ) -> Self {
        Self::new(
            ParseIssueSeverity::Warning,
            code,
            message,
            raw_line,
            payload,
        )
    }

    pub fn error(
        code: ParseIssueCode,
        message: String,
        raw_line: Option<String>,
        payload: Option<ParseIssuePayload>,
    ) -> Self {
        Self::new(ParseIssueSeverity::Error, code, message, raw_line, payload)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayerStatus {
    Live,
    Folded,
    AllIn,
    Eliminated,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlayerNodeState {
    pub seat_no: u8,
    pub player_name: String,
    pub stack_before_hand: i64,
    pub stack_at_snapshot: i64,
    pub committed_total: i64,
    pub committed_by_street: BTreeMap<String, i64>,
    pub status: PlayerStatus,
    pub is_hero: bool,
    pub hole_cards_known: bool,
    pub hole_cards: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PotSlice {
    pub pot_index: usize,
    pub amount: i64,
    pub eligible_players: Vec<String>,
    pub is_main: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CertaintyState {
    Exact,
    Estimated,
    Uncertain,
    Inconsistent,
}

impl CertaintyState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Estimated => "estimated",
            Self::Uncertain => "uncertain",
            Self::Inconsistent => "inconsistent",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinalPot {
    pub pot_no: u8,
    pub amount: i64,
    pub is_main: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PotContribution {
    pub pot_no: u8,
    pub seat_no: u8,
    pub player_name: String,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PotEligibility {
    pub pot_no: u8,
    pub seat_no: u8,
    pub player_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PotWinner {
    pub pot_no: u8,
    pub seat_no: u8,
    pub player_name: String,
    pub share_amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettlementCollectEvent {
    pub seq: usize,
    pub street: Street,
    pub seat_no: u8,
    pub player_name: String,
    pub amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettlementShowHand {
    pub seq: usize,
    pub street: Street,
    pub seat_no: u8,
    pub player_name: String,
    pub cards: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettlementSummaryOutcome {
    pub seat_no: u8,
    pub player_name: String,
    pub position_marker: Option<SummarySeatMarker>,
    pub outcome_kind: SummarySeatOutcomeKind,
    pub folded_at: Option<Street>,
    pub shown_cards: Option<Vec<String>>,
    pub won_amount: Option<i64>,
    pub hand_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandSettlementEvidence {
    pub collect_events_seen: Vec<SettlementCollectEvent>,
    pub summary_outcomes_seen: Vec<SettlementSummaryOutcome>,
    pub show_hands_seen: Vec<SettlementShowHand>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementAllocationSource {
    SingleContender,
    ShowdownRank,
    SinglePotCollectedAmounts,
    SingleCollectorFallback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettlementShare {
    pub seat_no: u8,
    pub player_name: String,
    pub share_amount: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettlementAllocation {
    pub source: SettlementAllocationSource,
    pub shares: Vec<SettlementShare>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum PotSettlementIssue {
    AmbiguousHiddenShowdown { eligible_players: Vec<String> },
    AmbiguousPartialReveal { eligible_players: Vec<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum SettlementIssue {
    CollectEventsWithoutPots,
    MissingCollections,
    MultipleExactAllocations,
    CollectConflictNoExactSettlementMatchesCollectedAmounts,
    ReplayStateInvalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettlementPot {
    pub pot_no: u8,
    pub amount: i64,
    pub is_main: bool,
    pub contributions: Vec<PotContribution>,
    pub eligibilities: Vec<PotEligibility>,
    pub contenders: Vec<String>,
    pub candidate_allocations: Vec<SettlementAllocation>,
    pub selected_allocation: Option<SettlementAllocation>,
    pub issues: Vec<PotSettlementIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandSettlement {
    pub certainty_state: CertaintyState,
    pub issues: Vec<SettlementIssue>,
    pub evidence: HandSettlementEvidence,
    pub pots: Vec<SettlementPot>,
}

impl HandSettlement {
    pub fn final_pots(&self) -> Vec<FinalPot> {
        self.pots
            .iter()
            .map(|pot| FinalPot {
                pot_no: pot.pot_no,
                amount: pot.amount,
                is_main: pot.is_main,
            })
            .collect()
    }

    pub fn pot_contributions(&self) -> Vec<PotContribution> {
        self.pots
            .iter()
            .flat_map(|pot| pot.contributions.iter().cloned())
            .collect()
    }

    pub fn pot_eligibilities(&self) -> Vec<PotEligibility> {
        self.pots
            .iter()
            .flat_map(|pot| pot.eligibilities.iter().cloned())
            .collect()
    }

    pub fn pot_winners(&self) -> Vec<PotWinner> {
        self.pots
            .iter()
            .flat_map(|pot| {
                pot.selected_allocation
                    .iter()
                    .flat_map(|allocation| allocation.shares.iter())
                    .map(|share| PotWinner {
                        pot_no: pot.pot_no,
                        seat_no: share.seat_no,
                        player_name: share.player_name.clone(),
                        share_amount: share.share_amount,
                    })
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandReturn {
    pub seat_no: u8,
    pub player_name: String,
    pub amount: i64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolutionNodeSnapshot {
    pub hand_id: String,
    pub snapshot_street: Street,
    pub snapshot_event_seq: usize,
    pub known_board_cards: Vec<String>,
    pub future_board_cards_count: u8,
    pub players: Vec<PlayerNodeState>,
    pub pots: Vec<PotSlice>,
    pub hero_name: String,
    pub terminal_allin_node: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandOutcomeActual {
    pub committed_total_by_player: BTreeMap<String, i64>,
    pub stacks_after_actual: BTreeMap<String, i64>,
    pub winner_collections: BTreeMap<String, i64>,
    pub final_board_cards: Vec<String>,
    pub rake_amount: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HandEliminationKoShare {
    pub seat_no: u8,
    pub player_name: String,
    pub share_fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HandElimination {
    pub eliminated_seat_no: u8,
    pub eliminated_player_name: String,
    pub pots_participated_by_busted: Vec<u8>,
    pub pots_causing_bust: Vec<u8>,
    pub last_busting_pot_no: Option<u8>,
    pub ko_winner_set: Vec<String>,
    pub ko_share_fraction_by_winner: Vec<HandEliminationKoShare>,
    pub elimination_certainty_state: CertaintyState,
    pub ko_certainty_state: CertaintyState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum InvariantIssue {
    ChipConservationMismatch {
        starting_sum: i64,
        final_sum: i64,
    },
    PotConservationMismatch {
        committed_total: i64,
        collected_total: i64,
        rake_amount: i64,
    },
    SummaryTotalPotMismatch {
        summary_total_pot: i64,
        collected_plus_rake: i64,
    },
    PrematureStreetClose {
        street: Street,
        pending_players: Vec<String>,
    },
    IllegalActorOrder {
        street: Street,
        seq: usize,
        expected_actor: String,
        actual_actor: String,
    },
    IllegalSmallBlindActor {
        seq: usize,
        expected_actor: String,
        actual_actor: String,
    },
    IllegalBigBlindActor {
        seq: usize,
        expected_actor: String,
        actual_actor: String,
    },
    UncalledReturnActorMismatch {
        seq: usize,
        player_name: String,
    },
    UncalledReturnAmountMismatch {
        seq: usize,
        player_name: String,
        allowed_refund: i64,
        actual_refund: i64,
    },
    ActionAmountExceedsStack {
        street: Street,
        seq: usize,
        player_name: String,
        available_stack: i64,
        attempted_amount: i64,
    },
    RefundExceedsCommitted {
        street: Street,
        seq: usize,
        player_name: String,
        committed_total: i64,
        actual_refund: i64,
    },
    RefundExceedsBettingRoundContrib {
        street: Street,
        seq: usize,
        player_name: String,
        betting_round_contrib: i64,
        actual_refund: i64,
    },
    IllegalCheck {
        street: Street,
        seq: usize,
        player_name: String,
        required_call: i64,
    },
    IllegalCallAmount {
        street: Street,
        seq: usize,
        player_name: String,
        expected_call: i64,
        actual_amount: i64,
    },
    UndercallInconsistency {
        street: Street,
        seq: usize,
        player_name: String,
        expected_call: i64,
        actual_amount: i64,
    },
    OvercallInconsistency {
        street: Street,
        seq: usize,
        player_name: String,
        expected_call: i64,
        actual_amount: i64,
    },
    IllegalBetFacingOpenBet {
        street: Street,
        seq: usize,
        player_name: String,
        required_call: i64,
    },
    ActionNotReopenedAfterShortAllIn {
        street: Street,
        seq: usize,
        player_name: String,
    },
    IncompleteRaiseToCall {
        street: Street,
        seq: usize,
        player_name: String,
        current_to_call: i64,
        attempted_to: i64,
    },
    IncompleteRaiseSize {
        street: Street,
        seq: usize,
        player_name: String,
        min_raise: i64,
        actual_raise: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandInvariants {
    pub chip_conservation_ok: bool,
    pub pot_conservation_ok: bool,
    pub issues: Vec<InvariantIssue>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NormalizedHand {
    pub hand_id: String,
    pub player_order: Vec<String>,
    pub snapshot: Option<ResolutionNodeSnapshot>,
    pub settlement: HandSettlement,
    pub returns: Vec<HandReturn>,
    pub actual: HandOutcomeActual,
    pub eliminations: Vec<HandElimination>,
    pub invariants: HandInvariants,
}
