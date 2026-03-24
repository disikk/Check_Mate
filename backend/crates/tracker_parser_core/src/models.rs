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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HandActionEvent {
    pub seq: usize,
    pub street: Street,
    pub player_name: Option<String>,
    pub action_type: ActionType,
    pub is_forced: bool,
    pub is_all_in: bool,
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
    pub collected_amounts: BTreeMap<String, i64>,
    pub raw_hand_text: String,
    pub parse_warnings: Vec<String>,
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
pub struct PotWinner {
    pub pot_no: u8,
    pub seat_no: u8,
    pub player_name: String,
    pub share_amount: i64,
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
pub struct HandElimination {
    pub eliminated_seat_no: u8,
    pub eliminated_player_name: String,
    pub resolved_by_pot_no: Option<u8>,
    pub ko_involved_winner_count: u8,
    pub hero_involved: bool,
    pub hero_share_fraction: Option<f64>,
    pub is_split_ko: bool,
    pub split_n: Option<u8>,
    pub is_sidepot_based: bool,
    pub certainty_state: CertaintyState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NormalizationInvariants {
    pub chip_conservation_ok: bool,
    pub pot_conservation_ok: bool,
    pub invariant_errors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NormalizedHand {
    pub hand_id: String,
    pub player_order: Vec<String>,
    pub snapshot: Option<ResolutionNodeSnapshot>,
    pub final_pots: Vec<FinalPot>,
    pub pot_contributions: Vec<PotContribution>,
    pub pot_winners: Vec<PotWinner>,
    pub returns: Vec<HandReturn>,
    pub actual: HandOutcomeActual,
    pub eliminations: Vec<HandElimination>,
    pub invariants: NormalizationInvariants,
    pub warnings: Vec<String>,
}
