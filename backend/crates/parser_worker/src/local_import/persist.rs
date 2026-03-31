use std::collections::BTreeMap;
use std::time::Instant;

use anyhow::{Result, anyhow};
use postgres::{Transaction, binary_copy::BinaryCopyInWriter, types::Type};
use uuid::Uuid;

use super::batch_sql::*;
use super::mbr_domain::*;
use super::profiles::*;
use super::row_models::*;
use super::util::*;

/// Write rows to a PostgreSQL table using binary COPY FROM STDIN protocol.
/// Callback-based API avoids boxing per-row values. Skips COPY if write_fn
/// writes zero rows. Returns the number of rows written.
fn copy_in_binary<F>(
    tx: &mut Transaction<'_>,
    table_name: &str,
    columns: &[&str],
    types: &[Type],
    write_fn: F,
) -> Result<u64>
where
    F: FnOnce(&mut BinaryCopyInWriter<'_>) -> Result<()>,
{
    let col_list = columns.join(", ");
    let copy_stmt = format!("COPY {table_name} ({col_list}) FROM STDIN (FORMAT binary)");
    let writer = tx.copy_in(&copy_stmt)?;
    let mut binary_writer = BinaryCopyInWriter::new(writer, types);
    write_fn(&mut binary_writer)?;
    let count = binary_writer.finish()?;
    Ok(count)
}

pub(crate) fn persist_prepared_tournament_summary_registered(
    tx: &mut impl postgres::GenericClient,
    context: &ImportContext,
    input: &str,
    source_file_id: Uuid,
    source_file_member_id: Uuid,
    import_job_id: Uuid,
    prepared: &PreparedTournamentSummaryImport,
) -> Result<LocalImportReport> {
    let persist_started_at = Instant::now();
    let tournament_entry_economics =
        load_tournament_entry_economics(tx, context, &prepared.summary)?;
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
                CASE
                    WHEN $12::text IS NULL THEN NULL
                    ELSE replace($11, '/', '-')::timestamp AT TIME ZONE $12
                END,
                $11,
                replace($11, '/', '-')::timestamp,
                $13,
                $14
            )
            ON CONFLICT (player_profile_id, room_id, external_tournament_id)
            DO UPDATE SET
                buyin_total = EXCLUDED.buyin_total,
                buyin_prize_component = EXCLUDED.buyin_prize_component,
                buyin_bounty_component = EXCLUDED.buyin_bounty_component,
                fee_component = EXCLUDED.fee_component,
                currency = EXCLUDED.currency,
                max_players = EXCLUDED.max_players,
                started_at = EXCLUDED.started_at,
                started_at_raw = EXCLUDED.started_at_raw,
                started_at_local = EXCLUDED.started_at_local,
                started_at_tz_provenance = EXCLUDED.started_at_tz_provenance,
                source_summary_file_id = COALESCE(EXCLUDED.source_summary_file_id, core.tournaments.source_summary_file_id)
            RETURNING id, (xmax = 0) AS is_new",
            &[
                &context.organization_id,
                &context.player_profile_id,
                &context.room_id,
                &context.format_id,
                &prepared.summary.tournament_id.to_string(),
                &cents_to_f64(
                    prepared.summary.buy_in_cents
                        + prepared.summary.rake_cents
                        + prepared.summary.bounty_cents,
                ),
                &cents_to_f64(prepared.summary.buy_in_cents),
                &cents_to_f64(prepared.summary.bounty_cents),
                &cents_to_f64(prepared.summary.rake_cents),
                &(prepared.summary.entrants as i32),
                &prepared.summary.started_at,
                &context.timezone_name,
                &gg_timestamp_provenance(context.timezone_name.as_deref()),
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
            &(prepared.summary.finish_place as i32),
            &cents_to_f64(tournament_entry_economics.regular_prize_cents),
            &cents_to_f64(prepared.summary.payout_cents),
            &cents_to_f64(tournament_entry_economics.mystery_money_cents),
            &(prepared.summary.finish_place == 1),
        ],
    )?;

    tx.execute(
        "DELETE FROM core.parse_issues
         WHERE source_file_id = $1
           AND fragment_id = $2",
        &[&source_file_id, &fragment_id],
    )?;

    for issue in tournament_summary_parse_issues(&prepared.summary) {
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

    let persist_db_ms = persist_started_at.elapsed().as_millis() as u64;
    Ok(LocalImportReport {
        file_kind: "ts",
        source_file_id,
        import_job_id,
        tournament_id,
        fragments_persisted: 1,
        hands_persisted: 0,
        runtime_profile: ComputeProfile {
            parse_ms: prepared.parse_ms,
            persist_db_ms,
            ..ComputeProfile::default()
        },
        stage_profile: IngestStageProfile {
            parse_ms: prepared.parse_ms,
            persist_ms: persist_db_ms,
            ..IngestStageProfile::default()
        },
    })
}

pub(crate) fn persist_prepared_hand_history_registered(
    tx: &mut impl postgres::GenericClient,
    context: &ImportContext,
    source_file_id: Uuid,
    source_file_member_id: Uuid,
    import_job_id: Uuid,
    prepared: &PreparedHandHistoryImport,
) -> Result<LocalImportReport> {
    let first_hand = prepared
        .hands
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

    let persist_started_at = Instant::now();

    let t_upsert_roots = Instant::now();
    let fragment_ids =
        bulk_upsert_file_fragments(tx, source_file_id, source_file_member_id, &prepared.hands)?;
    let (hand_ids, is_new_flags) = bulk_upsert_hand_rows(
        tx,
        context,
        tournament_id,
        source_file_id,
        &prepared.canonical_hands,
        &fragment_ids,
    )?;
    let upsert_roots_ms = t_upsert_roots.elapsed().as_millis() as u64;

    let t_delete = Instant::now();
    let updated_fragment_ids = fragment_ids
        .iter()
        .zip(is_new_flags.iter())
        .filter_map(|(fragment_id, is_new)| (!*is_new).then_some(*fragment_id))
        .collect::<Vec<_>>();
    let updated_hand_ids = hand_ids
        .iter()
        .zip(is_new_flags.iter())
        .filter_map(|(hand_id, is_new)| (!*is_new).then_some(*hand_id))
        .collect::<Vec<_>>();
    bulk_delete_hand_children(tx, source_file_id, &updated_fragment_ids, &updated_hand_ids)?;
    let delete_ms = t_delete.elapsed().as_millis() as u64;

    // Nested transaction (savepoint) to access COPY FROM STDIN protocol,
    // which requires Transaction<'_> (not available on GenericClient trait).
    let mut copy_tx = tx.transaction()?;

    let t_canonical = Instant::now();
    bulk_insert_canonical_hand_rows(
        &mut copy_tx,
        context,
        source_file_id,
        &hand_ids,
        &fragment_ids,
        &prepared.hand_local_outputs,
    )?;
    let canonical_ms = t_canonical.elapsed().as_millis() as u64;

    let t_normalized = Instant::now();
    bulk_insert_normalized_hand_rows(&mut copy_tx, &hand_ids, &prepared.hand_local_outputs)?;
    let normalized_ms = t_normalized.elapsed().as_millis() as u64;

    let t_derived = Instant::now();
    bulk_insert_hand_ko_event_rows(
        &mut copy_tx,
        context.player_profile_id,
        &hand_ids,
        &prepared.hand_local_outputs,
    )?;
    bulk_insert_preflop_starting_hand_rows(&mut copy_tx, &hand_ids, &prepared.hand_local_outputs)?;
    bulk_insert_street_hand_strength_rows(&mut copy_tx, &hand_ids, &prepared.hand_local_outputs)?;
    // mbr_stage_resolution uses UPSERT (ON CONFLICT), stays on batched INSERT
    bulk_upsert_mbr_stage_resolution_rows(
        &mut copy_tx,
        &hand_ids,
        &prepared.ordered_stage_resolutions,
    )?;
    let derived_ms = t_derived.elapsed().as_millis() as u64;

    copy_tx.commit()?;

    // tournament_hand_order + FT helper deferred to bundle_finalize to avoid
    // row lock contention when multiple workers process files from the same tournament.

    let persist_db_ms = persist_started_at.elapsed().as_millis() as u64;
    Ok(LocalImportReport {
        file_kind: "hh",
        source_file_id,
        import_job_id,
        tournament_id,
        fragments_persisted: prepared.hands.len(),
        hands_persisted: prepared.hands.len(),
        runtime_profile: ComputeProfile {
            parse_ms: prepared.parse_ms,
            normalize_ms: prepared.normalize_ms,
            derive_hand_local_ms: prepared.derive_hand_local_ms,
            derive_tournament_ms: prepared.derive_tournament_ms,
            persist_db_ms,
            materialize_ms: 0,
            finalize_ms: 0,
            persist_upsert_roots_ms: upsert_roots_ms,
            persist_delete_ms: delete_ms,
            persist_canonical_ms: canonical_ms,
            persist_normalized_ms: normalized_ms,
            persist_derived_ms: derived_ms,
            persist_hand_order_ms: 0,
        },
        stage_profile: IngestStageProfile {
            parse_ms: prepared.parse_ms,
            normalize_ms: prepared.normalize_ms,
            persist_ms: prepared.derive_hand_local_ms
                + prepared.derive_tournament_ms
                + persist_db_ms,
            materialize_ms: 0,
            finalize_ms: 0,
        },
    })
}

pub(crate) fn bulk_upsert_file_fragments(
    tx: &mut impl postgres::GenericClient,
    source_file_id: Uuid,
    source_file_member_id: Uuid,
    hands: &[tracker_parser_core::models::HandRecord],
) -> Result<Vec<Uuid>> {
    const COLUMN_PATTERNS: &[&str] = &["{}", "{}", "{}", "{}", "{}", "{}", "{}"];
    const INSERT_PREFIX: &str = "INSERT INTO import.file_fragments (        source_file_id,        source_file_member_id,        fragment_index,        external_hand_id,        kind,        raw_text,        sha256    )";
    const INSERT_SUFFIX: &str = "ON CONFLICT (source_file_member_id, fragment_index)
                 DO UPDATE SET
                     external_hand_id = EXCLUDED.external_hand_id,
                     kind = EXCLUDED.kind,
                     raw_text = EXCLUDED.raw_text,
                     sha256 = EXCLUDED.sha256
                 RETURNING id, fragment_index";

    if hands.is_empty() {
        return Ok(Vec::new());
    }

    let mut fragment_ids = vec![Uuid::nil(); hands.len()];
    for (chunk_index, chunk) in hands.chunks(PERSIST_BATCH_INSERT_CHUNK_SIZE).enumerate() {
        let mut fragment_indexes = Vec::with_capacity(chunk.len());
        let mut sha256_values = Vec::with_capacity(chunk.len());
        for (index, hand) in chunk.iter().enumerate() {
            fragment_indexes.push((chunk_index * PERSIST_BATCH_INSERT_CHUNK_SIZE + index) as i32);
            sha256_values.push(sha256_hex(&hand.raw_text));
        }

        let mut params: Vec<&(dyn postgres::types::ToSql + Sync)> =
            Vec::with_capacity(chunk.len() * COLUMN_PATTERNS.len());

        for (index, hand) in chunk.iter().enumerate() {
            params.push(&source_file_id);
            params.push(&source_file_member_id);
            params.push(&fragment_indexes[index]);
            params.push(&hand.header.hand_id);
            params.push(&"hand");
            params.push(&hand.raw_text);
            params.push(&sha256_values[index]);
        }

        let rows = execute_batched_query_with_suffix(
            tx,
            INSERT_PREFIX,
            Some(INSERT_SUFFIX),
            COLUMN_PATTERNS,
            chunk.len(),
            &params,
        )?;

        for row in rows {
            let fragment_index: i32 = row.get(1);
            fragment_ids[fragment_index as usize] = row.get(0);
        }
    }

    Ok(fragment_ids)
}

pub(crate) fn bulk_upsert_hand_rows(
    tx: &mut impl postgres::GenericClient,
    context: &ImportContext,
    tournament_id: Uuid,
    source_file_id: Uuid,
    canonical_hands: &[tracker_parser_core::models::CanonicalParsedHand],
    fragment_ids: &[Uuid],
) -> Result<(Vec<Uuid>, Vec<bool>)> {
    if canonical_hands.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let mut hand_ids = vec![Uuid::nil(); canonical_hands.len()];
    let mut is_new_flags = vec![false; canonical_hands.len()];
    let timestamp_provenance = gg_timestamp_provenance(context.timezone_name.as_deref());
    for (chunk_index, chunk) in canonical_hands
        .chunks(PERSIST_BATCH_INSERT_CHUNK_SIZE)
        .enumerate()
    {
        let offset = chunk_index * PERSIST_BATCH_INSERT_CHUNK_SIZE;
        let mut values_sql = Vec::with_capacity(chunk.len());
        let mut max_players = Vec::with_capacity(chunk.len());
        let mut button_seats = Vec::with_capacity(chunk.len());
        let mut small_blinds = Vec::with_capacity(chunk.len());
        let mut big_blinds = Vec::with_capacity(chunk.len());
        let mut antes = Vec::with_capacity(chunk.len());
        for hand in chunk {
            max_players.push(hand.header.max_players as i32);
            button_seats.push(hand.header.button_seat as i32);
            small_blinds.push(hand.header.small_blind as i64);
            big_blinds.push(hand.header.big_blind as i64);
            antes.push(hand.header.ante as i64);
        }
        let mut params: Vec<&(dyn postgres::types::ToSql + Sync)> =
            Vec::with_capacity(chunk.len() * 15);

        for (index, hand) in chunk.iter().enumerate() {
            let bind = index * 15;
            values_sql.push(format!(
                "(${organization_id}, ${player_profile_id}, ${tournament_id}, ${source_file_id}, ${external_hand_id}, CASE WHEN ${timezone_name}::text IS NULL THEN NULL ELSE replace(${played_at}, '/', '-')::timestamp AT TIME ZONE ${timezone_name} END, ${played_at}, replace(${played_at}, '/', '-')::timestamp, ${timestamp_provenance}, ${table_name}, ${table_max_seats}, ${dealer_seat_no}, ${small_blind}, ${big_blind}, ${ante}, 'USD', ${raw_fragment_id})",
                organization_id = bind + 1,
                player_profile_id = bind + 2,
                tournament_id = bind + 3,
                source_file_id = bind + 4,
                external_hand_id = bind + 5,
                played_at = bind + 6,
                timezone_name = bind + 7,
                timestamp_provenance = bind + 8,
                table_name = bind + 9,
                table_max_seats = bind + 10,
                dealer_seat_no = bind + 11,
                small_blind = bind + 12,
                big_blind = bind + 13,
                ante = bind + 14,
                raw_fragment_id = bind + 15,
            ));

            params.push(&context.organization_id);
            params.push(&context.player_profile_id);
            params.push(&tournament_id);
            params.push(&source_file_id);
            params.push(&hand.header.hand_id);
            params.push(&hand.header.played_at);
            params.push(&context.timezone_name);
            params.push(&timestamp_provenance);
            params.push(&hand.header.table_name);
            params.push(&max_players[index]);
            params.push(&button_seats[index]);
            params.push(&small_blinds[index]);
            params.push(&big_blinds[index]);
            params.push(&antes[index]);
            params.push(&fragment_ids[offset + index]);
        }

        let rows = tx.query(
            &format!(
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
                VALUES {}
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
                RETURNING id, raw_fragment_id, (xmax = 0) AS is_new",
                values_sql.join(", ")
            ),
            &params,
        )?;

        let fragment_index_by_id = fragment_ids[offset..offset + chunk.len()]
            .iter()
            .enumerate()
            .map(|(index, fragment_id)| (*fragment_id, offset + index))
            .collect::<BTreeMap<_, _>>();
        for row in rows {
            let fragment_id: Uuid = row.get(1);
            let hand_index = *fragment_index_by_id
                .get(&fragment_id)
                .expect("raw_fragment_id must map back to hand index");
            hand_ids[hand_index] = row.get(0);
            is_new_flags[hand_index] = row.get(2);
        }
    }

    Ok((hand_ids, is_new_flags))
}

pub(crate) fn bulk_delete_hand_children(
    tx: &mut impl postgres::GenericClient,
    source_file_id: Uuid,
    fragment_ids: &[Uuid],
    hand_ids: &[Uuid],
) -> Result<()> {
    if hand_ids.is_empty() {
        return Ok(());
    }

    tx.execute(
        "DELETE FROM core.parse_issues
         WHERE source_file_id = $1
           AND fragment_id = ANY($2::uuid[])",
        &[&source_file_id, &fragment_ids],
    )?;
    for table in [
        "core.hand_showdowns",
        "core.hand_summary_results",
        "core.hand_positions",
        "core.hand_hole_cards",
        "core.hand_actions",
        "core.hand_returns",
        "core.hand_pot_eligibility",
        "core.hand_pot_winners",
        "core.hand_pot_contributions",
        "core.hand_pots",
        "core.hand_boards",
        "core.hand_seats",
        "derived.hand_eliminations",
        "derived.hand_ko_attempts",
        "derived.hand_ko_opportunities",
        "derived.preflop_starting_hands",
        "derived.street_hand_strength",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE hand_id = ANY($1::uuid[])"),
            &[&hand_ids],
        )?;
    }

    Ok(())
}

pub(crate) fn bulk_insert_canonical_hand_rows(
    tx: &mut Transaction<'_>,
    context: &ImportContext,
    source_file_id: Uuid,
    hand_ids: &[Uuid],
    fragment_ids: &[Uuid],
    outputs: &[HandLocalComputeOutput],
) -> Result<()> {
    debug_assert_eq!(hand_ids.len(), outputs.len());
    debug_assert_eq!(fragment_ids.len(), outputs.len());

    // core.hand_seats — 8 columns, requires player_profile_id alias resolution
    copy_in_binary(
        tx,
        "core.hand_seats",
        &[
            "hand_id",
            "seat_no",
            "player_name",
            "player_profile_id",
            "starting_stack",
            "is_hero",
            "is_button",
            "is_sitting_out",
        ],
        &[
            Type::UUID,
            Type::INT4,
            Type::TEXT,
            Type::UUID,
            Type::INT8,
            Type::BOOL,
            Type::BOOL,
            Type::BOOL,
        ],
        |writer| {
            for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) {
                for seat in &output.canonical_persistence.seats {
                    let mapped_profile_id: Option<Uuid> = context
                        .player_aliases
                        .iter()
                        .any(|alias| alias == &seat.player_name)
                        .then_some(context.player_profile_id);
                    writer.write(&[
                        hand_id as &(dyn postgres::types::ToSql + Sync),
                        &seat.seat_no,
                        &seat.player_name,
                        &mapped_profile_id,
                        &seat.starting_stack,
                        &seat.is_hero,
                        &seat.is_button,
                        &seat.is_sitting_out,
                    ])?;
                }
            }
            Ok(())
        },
    )?;

    // core.hand_positions — 6 columns
    copy_in_binary(
        tx,
        "core.hand_positions",
        &[
            "hand_id",
            "seat_no",
            "position_index",
            "position_label",
            "preflop_act_order_index",
            "postflop_act_order_index",
        ],
        &[
            Type::UUID,
            Type::INT4,
            Type::INT4,
            Type::TEXT,
            Type::INT4,
            Type::INT4,
        ],
        |writer| {
            for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) {
                for row in &output.canonical_persistence.positions {
                    writer.write(&[
                        hand_id as &(dyn postgres::types::ToSql + Sync),
                        &row.seat_no,
                        &row.position_index,
                        &row.position_label,
                        &row.preflop_act_order_index,
                        &row.postflop_act_order_index,
                    ])?;
                }
            }
            Ok(())
        },
    )?;

    // core.hand_hole_cards — 6 columns
    copy_in_binary(
        tx,
        "core.hand_hole_cards",
        &[
            "hand_id",
            "seat_no",
            "card1",
            "card2",
            "known_to_hero",
            "known_at_showdown",
        ],
        &[
            Type::UUID,
            Type::INT4,
            Type::TEXT,
            Type::TEXT,
            Type::BOOL,
            Type::BOOL,
        ],
        |writer| {
            for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) {
                for row in &output.canonical_persistence.hole_cards {
                    writer.write(&[
                        hand_id as &(dyn postgres::types::ToSql + Sync),
                        &row.seat_no,
                        &row.card1,
                        &row.card2,
                        &row.known_to_hero,
                        &row.known_at_showdown,
                    ])?;
                }
            }
            Ok(())
        },
    )?;

    // core.hand_actions — 12 columns, largest child table (~15-30 rows/hand)
    copy_in_binary(
        tx,
        "core.hand_actions",
        &[
            "hand_id",
            "sequence_no",
            "street",
            "seat_no",
            "action_type",
            "raw_amount",
            "to_amount",
            "is_all_in",
            "all_in_reason",
            "forced_all_in_preflop",
            "references_previous_bet",
            "raw_line",
        ],
        &[
            Type::UUID,
            Type::INT4,
            Type::TEXT,
            Type::INT4,
            Type::TEXT,
            Type::INT8,
            Type::INT8,
            Type::BOOL,
            Type::TEXT,
            Type::BOOL,
            Type::BOOL,
            Type::TEXT,
        ],
        |writer| {
            for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) {
                for row in &output.canonical_persistence.actions {
                    writer.write(&[
                        hand_id as &(dyn postgres::types::ToSql + Sync),
                        &row.sequence_no,
                        &row.street,
                        &row.seat_no,
                        &row.action_type,
                        &row.raw_amount,
                        &row.to_amount,
                        &row.is_all_in,
                        &row.all_in_reason,
                        &row.forced_all_in_preflop,
                        &row.references_previous_bet,
                        &row.raw_line,
                    ])?;
                }
            }
            Ok(())
        },
    )?;

    // core.hand_boards — 6 columns, optional (only hands that reach flop+)
    copy_in_binary(
        tx,
        "core.hand_boards",
        &["hand_id", "flop1", "flop2", "flop3", "turn", "river"],
        &[
            Type::UUID,
            Type::TEXT,
            Type::TEXT,
            Type::TEXT,
            Type::TEXT,
            Type::TEXT,
        ],
        |writer| {
            for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) {
                if let Some(board) = &output.canonical_persistence.board {
                    writer.write(&[
                        hand_id as &(dyn postgres::types::ToSql + Sync),
                        &board.flop1,
                        &board.flop2,
                        &board.flop3,
                        &board.turn,
                        &board.river,
                    ])?;
                }
            }
            Ok(())
        },
    )?;

    // core.hand_showdowns — 3 columns
    copy_in_binary(
        tx,
        "core.hand_showdowns",
        &["hand_id", "seat_no", "shown_cards"],
        &[Type::UUID, Type::INT4, Type::TEXT_ARRAY],
        |writer| {
            for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) {
                for row in &output.canonical_persistence.showdowns {
                    writer.write(&[
                        hand_id as &(dyn postgres::types::ToSql + Sync),
                        &row.seat_no,
                        &row.shown_cards,
                    ])?;
                }
            }
            Ok(())
        },
    )?;

    // core.hand_summary_results — 10 columns
    copy_in_binary(
        tx,
        "core.hand_summary_results",
        &[
            "hand_id",
            "seat_no",
            "player_name",
            "position_marker",
            "outcome_kind",
            "folded_street",
            "shown_cards",
            "won_amount",
            "hand_class",
            "raw_line",
        ],
        &[
            Type::UUID,
            Type::INT4,
            Type::TEXT,
            Type::TEXT,
            Type::TEXT,
            Type::TEXT,
            Type::TEXT_ARRAY,
            Type::INT8,
            Type::TEXT,
            Type::TEXT,
        ],
        |writer| {
            for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) {
                for row in &output.canonical_persistence.summary_seat_outcomes {
                    writer.write(&[
                        hand_id as &(dyn postgres::types::ToSql + Sync),
                        &row.seat_no,
                        &row.player_name,
                        &row.position_marker,
                        &row.outcome_kind,
                        &row.folded_street,
                        &row.shown_cards,
                        &row.won_amount,
                        &row.hand_class,
                        &row.raw_line,
                    ])?;
                }
            }
            Ok(())
        },
    )?;

    // core.parse_issues — 8 columns, payload is JSONB
    copy_in_binary(
        tx,
        "core.parse_issues",
        &[
            "source_file_id",
            "fragment_id",
            "hand_id",
            "severity",
            "code",
            "message",
            "raw_line",
            "payload",
        ],
        &[
            Type::UUID,
            Type::UUID,
            Type::UUID,
            Type::TEXT,
            Type::TEXT,
            Type::TEXT,
            Type::TEXT,
            Type::JSONB,
        ],
        |writer| {
            for ((hand_id, fragment_id), output) in
                hand_ids.iter().zip(fragment_ids.iter()).zip(outputs.iter())
            {
                for row in &output.canonical_persistence.parse_issues {
                    writer.write(&[
                        &source_file_id as &(dyn postgres::types::ToSql + Sync),
                        fragment_id,
                        hand_id,
                        &row.severity,
                        &row.code,
                        &row.message,
                        &row.raw_line,
                        &row.payload,
                    ])?;
                }
            }
            Ok(())
        },
    )?;

    Ok(())
}

pub(crate) fn bulk_insert_normalized_hand_rows(
    tx: &mut Transaction<'_>,
    hand_ids: &[Uuid],
    outputs: &[HandLocalComputeOutput],
) -> Result<()> {
    debug_assert_eq!(hand_ids.len(), outputs.len());

    // hand_state_resolutions — UPSERT (ON CONFLICT), stays on batched INSERT
    {
        const COLUMN_PATTERNS: &[&str] = &["{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}"];
        const INSERT_PREFIX: &str = "INSERT INTO derived.hand_state_resolutions (        hand_id,        resolution_version,        chip_conservation_ok,        pot_conservation_ok,        settlement_state,        rake_amount,        final_stacks,        settlement,        invariant_issues    )";
        const INSERT_SUFFIX: &str = "ON CONFLICT (hand_id, resolution_version) DO UPDATE SET chip_conservation_ok = EXCLUDED.chip_conservation_ok, pot_conservation_ok = EXCLUDED.pot_conservation_ok, settlement_state = EXCLUDED.settlement_state, rake_amount = EXCLUDED.rake_amount, final_stacks = EXCLUDED.final_stacks, settlement = EXCLUDED.settlement, invariant_issues = EXCLUDED.invariant_issues";
        let mut buffered_rows: Vec<(
            Uuid,
            HandStateResolutionRow,
            serde_json::Value,
            serde_json::Value,
            serde_json::Value,
        )> = Vec::with_capacity(PERSIST_BATCH_INSERT_CHUNK_SIZE);
        for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) {
            if buffered_rows.len() == PERSIST_BATCH_INSERT_CHUNK_SIZE {
                let mut params: Vec<&(dyn postgres::types::ToSql + Sync)> =
                    Vec::with_capacity(buffered_rows.len() * COLUMN_PATTERNS.len());
                for (
                    buffered_hand_id,
                    resolution,
                    final_stacks_json,
                    settlement_json,
                    invariant_issues_json,
                ) in &buffered_rows
                {
                    params.push(buffered_hand_id);
                    params.push(&resolution.resolution_version);
                    params.push(&resolution.chip_conservation_ok);
                    params.push(&resolution.pot_conservation_ok);
                    params.push(&resolution.settlement_state);
                    params.push(&resolution.rake_amount);
                    params.push(final_stacks_json);
                    params.push(settlement_json);
                    params.push(invariant_issues_json);
                }
                execute_batched_insert_with_suffix(
                    tx,
                    INSERT_PREFIX,
                    Some(INSERT_SUFFIX),
                    COLUMN_PATTERNS,
                    buffered_rows.len(),
                    &params,
                )?;
                buffered_rows.clear();
            }
            let resolution = &output.normalized_persistence.state_resolution;
            buffered_rows.push((
                *hand_id,
                resolution.clone(),
                serde_json::to_value(&resolution.final_stacks)?,
                serde_json::to_value(&resolution.settlement)?,
                serde_json::to_value(&resolution.invariant_issues)?,
            ));
        }
        if !buffered_rows.is_empty() {
            let mut params: Vec<&(dyn postgres::types::ToSql + Sync)> =
                Vec::with_capacity(buffered_rows.len() * COLUMN_PATTERNS.len());
            for (
                buffered_hand_id,
                resolution,
                final_stacks_json,
                settlement_json,
                invariant_issues_json,
            ) in &buffered_rows
            {
                params.push(buffered_hand_id);
                params.push(&resolution.resolution_version);
                params.push(&resolution.chip_conservation_ok);
                params.push(&resolution.pot_conservation_ok);
                params.push(&resolution.settlement_state);
                params.push(&resolution.rake_amount);
                params.push(final_stacks_json);
                params.push(settlement_json);
                params.push(invariant_issues_json);
            }
            execute_batched_insert_with_suffix(
                tx,
                INSERT_PREFIX,
                Some(INSERT_SUFFIX),
                COLUMN_PATTERNS,
                buffered_rows.len(),
                &params,
            )?;
        }
    }

    // core.hand_pots — binary COPY
    copy_in_binary(tx, "core.hand_pots", &["hand_id", "pot_no", "pot_type", "amount"], &[Type::UUID, Type::INT4, Type::TEXT, Type::INT8], |writer| { for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) { for row in &output.normalized_persistence.pot_rows { writer.write(&[hand_id as &(dyn postgres::types::ToSql + Sync), &row.pot_no, &row.pot_type, &row.amount])?; } } Ok(()) })?;

    // core.hand_pot_eligibility — binary COPY
    copy_in_binary(tx, "core.hand_pot_eligibility", &["hand_id", "pot_no", "seat_no"], &[Type::UUID, Type::INT4, Type::INT4], |writer| { for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) { for row in &output.normalized_persistence.eligibility_rows { writer.write(&[hand_id as &(dyn postgres::types::ToSql + Sync), &row.pot_no, &row.seat_no])?; } } Ok(()) })?;

    // core.hand_pot_contributions — binary COPY
    copy_in_binary(tx, "core.hand_pot_contributions", &["hand_id", "pot_no", "seat_no", "amount"], &[Type::UUID, Type::INT4, Type::INT4, Type::INT8], |writer| { for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) { for row in &output.normalized_persistence.contribution_rows { writer.write(&[hand_id as &(dyn postgres::types::ToSql + Sync), &row.pot_no, &row.seat_no, &row.amount])?; } } Ok(()) })?;

    // core.hand_pot_winners — binary COPY
    copy_in_binary(tx, "core.hand_pot_winners", &["hand_id", "pot_no", "seat_no", "share_amount"], &[Type::UUID, Type::INT4, Type::INT4, Type::INT8], |writer| { for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) { for row in &output.normalized_persistence.winner_rows { writer.write(&[hand_id as &(dyn postgres::types::ToSql + Sync), &row.pot_no, &row.seat_no, &row.share_amount])?; } } Ok(()) })?;

    // core.hand_returns — binary COPY
    copy_in_binary(tx, "core.hand_returns", &["hand_id", "seat_no", "amount", "reason"], &[Type::UUID, Type::INT4, Type::INT8, Type::TEXT], |writer| { for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) { for row in &output.normalized_persistence.return_rows { writer.write(&[hand_id as &(dyn postgres::types::ToSql + Sync), &row.seat_no, &row.amount, &row.reason])?; } } Ok(()) })?;

    // derived.hand_eliminations — binary COPY (was per-row INSERT loop!)
    // ko_share_fraction_by_winner must be pre-serialized to serde_json::Value for JSONB COPY.
    let elimination_jsons: Vec<(Uuid, &HandEliminationRow, serde_json::Value)> = hand_ids
        .iter()
        .zip(outputs.iter())
        .flat_map(|(hand_id, output)| {
            output
                .normalized_persistence
                .elimination_rows
                .iter()
                .map(move |row| {
                    (
                        *hand_id,
                        row,
                        serde_json::to_value(&row.ko_share_fraction_by_winner)
                            .unwrap_or(serde_json::Value::Null),
                    )
                })
        })
        .collect();
    copy_in_binary(
        tx,
        "derived.hand_eliminations",
        &[
            "hand_id",
            "eliminated_seat_no",
            "eliminated_player_name",
            "pots_participated_by_busted",
            "pots_causing_bust",
            "last_busting_pot_no",
            "ko_winner_set",
            "ko_share_fraction_by_winner",
            "elimination_certainty_state",
            "ko_certainty_state",
        ],
        &[
            Type::UUID, Type::INT4, Type::TEXT, Type::INT4_ARRAY, Type::INT4_ARRAY,
            Type::INT4, Type::TEXT_ARRAY, Type::JSONB, Type::TEXT, Type::TEXT,
        ],
        |writer| {
            for (hand_id, row, share_json) in &elimination_jsons {
                writer.write(&[
                    hand_id as &(dyn postgres::types::ToSql + Sync),
                    &row.eliminated_seat_no, &row.eliminated_player_name,
                    &row.pots_participated_by_busted, &row.pots_causing_bust,
                    &row.last_busting_pot_no, &row.ko_winner_set, share_json,
                    &row.elimination_certainty_state, &row.ko_certainty_state,
                ])?;
            }
            Ok(())
        },
    )?;

    Ok(())
}

pub(crate) fn bulk_insert_hand_ko_event_rows(
    tx: &mut Transaction<'_>,
    player_profile_id: Uuid,
    hand_ids: &[Uuid],
    outputs: &[HandLocalComputeOutput],
) -> Result<()> {
    debug_assert_eq!(hand_ids.len(), outputs.len());

    // derived.hand_ko_attempts — binary COPY
    copy_in_binary(tx, "derived.hand_ko_attempts",
        &["hand_id", "player_profile_id", "hero_seat_no", "target_seat_no", "target_player_name", "attempt_kind", "street", "source_sequence_no", "is_forced_all_in"],
        &[Type::UUID, Type::UUID, Type::INT4, Type::INT4, Type::TEXT, Type::TEXT, Type::TEXT, Type::INT4, Type::BOOL],
        |writer| { for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) { for row in &output.ko_attempt_rows { writer.write(&[hand_id as &(dyn postgres::types::ToSql + Sync), &player_profile_id, &row.hero_seat_no, &row.target_seat_no, &row.target_player_name, &row.attempt_kind, &row.street, &row.source_sequence_no, &row.is_forced_all_in])?; } } Ok(()) })?;

    // derived.hand_ko_opportunities — binary COPY
    copy_in_binary(tx, "derived.hand_ko_opportunities",
        &["hand_id", "player_profile_id", "hero_seat_no", "target_seat_no", "target_player_name", "opportunity_kind", "street", "source_sequence_no", "is_forced_all_in"],
        &[Type::UUID, Type::UUID, Type::INT4, Type::INT4, Type::TEXT, Type::TEXT, Type::TEXT, Type::INT4, Type::BOOL],
        |writer| { for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) { for row in &output.ko_opportunity_rows { writer.write(&[hand_id as &(dyn postgres::types::ToSql + Sync), &player_profile_id, &row.hero_seat_no, &row.target_seat_no, &row.target_player_name, &row.opportunity_kind, &row.street, &row.source_sequence_no, &row.is_forced_all_in])?; } } Ok(()) })?;

    Ok(())
}

pub(crate) fn bulk_insert_preflop_starting_hand_rows(
    tx: &mut Transaction<'_>,
    hand_ids: &[Uuid],
    outputs: &[HandLocalComputeOutput],
) -> Result<()> {
    debug_assert_eq!(hand_ids.len(), outputs.len());
    copy_in_binary(tx, "derived.preflop_starting_hands",
        &["hand_id", "seat_no", "starter_hand_class", "certainty_state"],
        &[Type::UUID, Type::INT4, Type::TEXT, Type::TEXT],
        |writer| { for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) { for row in &output.preflop_starting_hand_rows { writer.write(&[hand_id as &(dyn postgres::types::ToSql + Sync), &row.seat_no, &row.starter_hand_class, &row.certainty_state])?; } } Ok(()) })?;
    Ok(())
}

pub(crate) fn bulk_insert_street_hand_strength_rows(
    tx: &mut Transaction<'_>,
    hand_ids: &[Uuid],
    outputs: &[HandLocalComputeOutput],
) -> Result<()> {
    debug_assert_eq!(hand_ids.len(), outputs.len());
    copy_in_binary(tx, "derived.street_hand_strength",
        &["hand_id", "seat_no", "street", "best_hand_class", "best_hand_rank_value", "made_hand_category", "draw_category", "overcards_count", "has_air", "missed_flush_draw", "missed_straight_draw", "is_nut_hand", "is_nut_draw", "certainty_state"],
        &[Type::UUID, Type::INT4, Type::TEXT, Type::TEXT, Type::INT8, Type::TEXT, Type::TEXT, Type::INT4, Type::BOOL, Type::BOOL, Type::BOOL, Type::BOOL, Type::BOOL, Type::TEXT],
        |writer| { for (hand_id, output) in hand_ids.iter().zip(outputs.iter()) { for row in &output.street_strength_rows { writer.write(&[hand_id as &(dyn postgres::types::ToSql + Sync), &row.seat_no, &row.street, &row.best_hand_class, &row.best_hand_rank_value, &row.made_hand_category, &row.draw_category, &row.overcards_count, &row.has_air, &row.missed_flush_draw, &row.missed_straight_draw, &row.is_nut_hand, &row.is_nut_draw, &row.certainty_state])?; } } Ok(()) })?;
    Ok(())
}

pub(crate) fn bulk_upsert_mbr_stage_resolution_rows(
    tx: &mut impl postgres::GenericClient,
    hand_ids: &[Uuid],
    rows: &[MbrStageResolutionRow],
) -> Result<()> {
    debug_assert_eq!(hand_ids.len(), rows.len());

    const COLUMN_PATTERNS: &[&str] = &[
        "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}", "{}",
        "({}::text)::numeric(12,6)", "({}::text)::numeric(12,6)", "({}::text)::numeric(12,6)",
        "{}", "{}", "{}",
    ];
    const INSERT_PREFIX: &str = "INSERT INTO derived.mbr_stage_resolution (        hand_id,        player_profile_id,        played_ft_hand,        played_ft_hand_state,        is_ft_hand,        ft_players_remaining_exact,        is_stage_2,        is_stage_3_4,        is_stage_4_5,        is_stage_5_6,        is_stage_6_9,        is_boundary_hand,        entered_boundary_zone,        entered_boundary_zone_state,        boundary_resolution_state,        boundary_candidate_count,        boundary_resolution_method,        boundary_confidence_class,        ft_table_size,        boundary_ko_ev,        boundary_ko_min,        boundary_ko_max,        boundary_ko_method,        boundary_ko_certainty,        boundary_ko_state    )";
    const INSERT_SUFFIX: &str = "ON CONFLICT (hand_id, player_profile_id) DO UPDATE SET played_ft_hand = EXCLUDED.played_ft_hand, played_ft_hand_state = EXCLUDED.played_ft_hand_state, is_ft_hand = EXCLUDED.is_ft_hand, ft_players_remaining_exact = EXCLUDED.ft_players_remaining_exact, is_stage_2 = EXCLUDED.is_stage_2, is_stage_3_4 = EXCLUDED.is_stage_3_4, is_stage_4_5 = EXCLUDED.is_stage_4_5, is_stage_5_6 = EXCLUDED.is_stage_5_6, is_stage_6_9 = EXCLUDED.is_stage_6_9, is_boundary_hand = EXCLUDED.is_boundary_hand, entered_boundary_zone = EXCLUDED.entered_boundary_zone, entered_boundary_zone_state = EXCLUDED.entered_boundary_zone_state, boundary_resolution_state = EXCLUDED.boundary_resolution_state, boundary_candidate_count = EXCLUDED.boundary_candidate_count, boundary_resolution_method = EXCLUDED.boundary_resolution_method, boundary_confidence_class = EXCLUDED.boundary_confidence_class, ft_table_size = EXCLUDED.ft_table_size, boundary_ko_ev = EXCLUDED.boundary_ko_ev, boundary_ko_min = EXCLUDED.boundary_ko_min, boundary_ko_max = EXCLUDED.boundary_ko_max, boundary_ko_method = EXCLUDED.boundary_ko_method, boundary_ko_certainty = EXCLUDED.boundary_ko_certainty, boundary_ko_state = EXCLUDED.boundary_ko_state";
    let mut params: Vec<&(dyn postgres::types::ToSql + Sync)> =
        Vec::with_capacity(PERSIST_BATCH_INSERT_CHUNK_SIZE * COLUMN_PATTERNS.len());
    let mut row_count = 0usize;
    for (hand_id, row) in hand_ids.iter().zip(rows.iter()) {
        if row_count == PERSIST_BATCH_INSERT_CHUNK_SIZE {
            execute_batched_insert_with_suffix(tx, INSERT_PREFIX, Some(INSERT_SUFFIX), COLUMN_PATTERNS, row_count, &params)?;
            params.clear();
            row_count = 0;
        }
        params.push(hand_id); params.push(&row.player_profile_id);
        params.push(&row.played_ft_hand); params.push(&row.played_ft_hand_state);
        params.push(&row.is_ft_hand); params.push(&row.ft_players_remaining_exact);
        params.push(&row.is_stage_2); params.push(&row.is_stage_3_4);
        params.push(&row.is_stage_4_5); params.push(&row.is_stage_5_6);
        params.push(&row.is_stage_6_9); params.push(&row.is_boundary_hand);
        params.push(&row.entered_boundary_zone); params.push(&row.entered_boundary_zone_state);
        params.push(&row.boundary_resolution_state); params.push(&row.boundary_candidate_count);
        params.push(&row.boundary_resolution_method); params.push(&row.boundary_confidence_class);
        params.push(&row.ft_table_size);
        params.push(&row.boundary_ko_ev); params.push(&row.boundary_ko_min);
        params.push(&row.boundary_ko_max); params.push(&row.boundary_ko_method);
        params.push(&row.boundary_ko_certainty); params.push(&row.boundary_ko_state);
        row_count += 1;
    }
    execute_batched_insert_with_suffix(tx, INSERT_PREFIX, Some(INSERT_SUFFIX), COLUMN_PATTERNS, row_count, &params)?;
    Ok(())
}

// ===== Test-only persistence helpers =====

#[cfg(test)]
pub(crate) fn insert_source_file(
    tx: &mut Transaction<'_>,
    context: &ImportContext,
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

#[cfg(test)]
pub(crate) fn insert_source_file_member(
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

#[cfg(test)]
pub(crate) fn insert_import_job(
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

#[cfg(test)]
pub(crate) fn insert_job_attempt(tx: &mut Transaction<'_>, import_job_id: Uuid) -> Result<Uuid> {
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

pub(crate) fn insert_file_fragment(
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

// Per-hand upsert helper used by tests (upsert_hand_row_reports_fresh_insert_vs_reimport_conflict).

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn upsert_hand_row(
    tx: &mut impl postgres::GenericClient,
    context: &ImportContext,
    tournament_id: Uuid,
    source_file_id: Uuid,
    fragment_id: Uuid,
    hand: &tracker_parser_core::models::CanonicalParsedHand,
) -> Result<(Uuid, bool)> {
    let row = tx.query_one(
        "INSERT INTO core.hands (
                organization_id, player_profile_id, tournament_id, source_file_id, external_hand_id,
                hand_started_at, hand_started_at_raw, hand_started_at_local, hand_started_at_tz_provenance,
                table_name, table_max_seats, dealer_seat_no, small_blind, big_blind, ante, currency, raw_fragment_id
            )
            VALUES (
                $1, $2, $3, $4, $5,
                CASE WHEN $7::text IS NULL THEN NULL ELSE replace($6, '/', '-')::timestamp AT TIME ZONE $7 END,
                $6, replace($6, '/', '-')::timestamp, $8, $9, $10, $11, $12, $13, $14, 'USD', $15
            )
            ON CONFLICT (player_profile_id, external_hand_id)
            DO UPDATE SET
                tournament_id = EXCLUDED.tournament_id, source_file_id = EXCLUDED.source_file_id,
                hand_started_at = EXCLUDED.hand_started_at, hand_started_at_raw = EXCLUDED.hand_started_at_raw,
                hand_started_at_local = EXCLUDED.hand_started_at_local, hand_started_at_tz_provenance = EXCLUDED.hand_started_at_tz_provenance,
                table_name = EXCLUDED.table_name, table_max_seats = EXCLUDED.table_max_seats,
                dealer_seat_no = EXCLUDED.dealer_seat_no, small_blind = EXCLUDED.small_blind,
                big_blind = EXCLUDED.big_blind, ante = EXCLUDED.ante, currency = EXCLUDED.currency,
                raw_fragment_id = EXCLUDED.raw_fragment_id
            RETURNING id, (xmax = 0) AS is_new",
        &[
            &context.organization_id, &context.player_profile_id, &tournament_id, &source_file_id,
            &hand.header.hand_id, &hand.header.played_at, &context.timezone_name,
            &gg_timestamp_provenance(context.timezone_name.as_deref()),
            &hand.header.table_name, &(hand.header.max_players as i32), &(hand.header.button_seat as i32),
            &(hand.header.small_blind as i64), &(hand.header.big_blind as i64), &(hand.header.ante as i64),
            &fragment_id,
        ],
    )?;
    Ok((row.get(0), row.get(1)))
}

