use anyhow::{Context, Result, anyhow};
use mbr_stats_runtime::materialize_player_hand_features;
use postgres::{Client, NoTls, Transaction};
use uuid::Uuid;

use super::context::database_url_from_env;
use super::profiles::TimezoneUpdateReport;
use super::{GG_TIMESTAMP_PROVENANCE_MISSING, GG_TIMESTAMP_PROVENANCE_PRESENT};

pub fn set_user_timezone(user_id: Uuid, timezone_name: &str) -> Result<TimezoneUpdateReport> {
    let database_url = database_url_from_env()?;
    let mut client =
        Client::connect(&database_url, NoTls).context("failed to connect to PostgreSQL")?;
    let mut tx = client
        .transaction()
        .context("failed to start timezone update transaction")?;
    validate_timezone_name(&mut tx, timezone_name)?;

    let updated_rows = tx.execute(
        "UPDATE auth.users
         SET timezone_name = $2
         WHERE id = $1",
        &[&user_id, &timezone_name],
    )?;
    if updated_rows == 0 {
        return Err(anyhow!("user `{user_id}` does not exist"));
    }

    let report = recompute_user_timezone_contract(&mut tx, user_id, Some(timezone_name))?;
    tx.commit()
        .context("failed to commit timezone update transaction")?;
    Ok(report)
}

pub fn clear_user_timezone(user_id: Uuid) -> Result<TimezoneUpdateReport> {
    let database_url = database_url_from_env()?;
    let mut client =
        Client::connect(&database_url, NoTls).context("failed to connect to PostgreSQL")?;
    let mut tx = client
        .transaction()
        .context("failed to start timezone clear transaction")?;

    let updated_rows = tx.execute(
        "UPDATE auth.users
         SET timezone_name = NULL
         WHERE id = $1",
        &[&user_id],
    )?;
    if updated_rows == 0 {
        return Err(anyhow!("user `{user_id}` does not exist"));
    }

    let report = recompute_user_timezone_contract(&mut tx, user_id, None)?;
    tx.commit()
        .context("failed to commit timezone clear transaction")?;
    Ok(report)
}

fn validate_timezone_name(
    client: &mut impl postgres::GenericClient,
    timezone_name: &str,
) -> Result<()> {
    client
        .query_one("SELECT now() AT TIME ZONE $1", &[&timezone_name])
        .with_context(|| format!("invalid IANA timezone `{timezone_name}`"))?;
    Ok(())
}

fn recompute_user_timezone_contract(
    tx: &mut Transaction<'_>,
    user_id: Uuid,
    timezone_name: Option<&str>,
) -> Result<TimezoneUpdateReport> {
    let tournaments_recomputed = tx.execute(
        "UPDATE core.tournaments AS tournaments
         SET started_at = CASE
                 WHEN users.timezone_name IS NULL OR tournaments.started_at_local IS NULL THEN NULL
                 ELSE tournaments.started_at_local AT TIME ZONE users.timezone_name
             END,
             started_at_tz_provenance = CASE
                 WHEN users.timezone_name IS NULL THEN $2
                 ELSE $3
             END
         FROM core.player_profiles AS player_profiles
         INNER JOIN auth.users AS users
             ON users.id = player_profiles.owner_user_id
         WHERE tournaments.player_profile_id = player_profiles.id
           AND player_profiles.owner_user_id = $1
           AND player_profiles.room = 'gg'
           AND tournaments.started_at_raw IS NOT NULL",
        &[
            &user_id,
            &GG_TIMESTAMP_PROVENANCE_MISSING,
            &GG_TIMESTAMP_PROVENANCE_PRESENT,
        ],
    )?;
    let hands_recomputed = tx.execute(
        "UPDATE core.hands AS hands
         SET hand_started_at = CASE
                 WHEN users.timezone_name IS NULL OR hands.hand_started_at_local IS NULL THEN NULL
                 ELSE hands.hand_started_at_local AT TIME ZONE users.timezone_name
             END,
             hand_started_at_tz_provenance = CASE
                 WHEN users.timezone_name IS NULL THEN $2
                 ELSE $3
             END
         FROM core.player_profiles AS player_profiles
         INNER JOIN auth.users AS users
             ON users.id = player_profiles.owner_user_id
         WHERE hands.player_profile_id = player_profiles.id
           AND player_profiles.owner_user_id = $1
           AND player_profiles.room = 'gg'
           AND hands.hand_started_at_raw IS NOT NULL",
        &[
            &user_id,
            &GG_TIMESTAMP_PROVENANCE_MISSING,
            &GG_TIMESTAMP_PROVENANCE_PRESENT,
        ],
    )?;

    let affected_profiles = tx
        .query(
            "SELECT id, organization_id
             FROM core.player_profiles
             WHERE owner_user_id = $1
               AND room = 'gg'
             ORDER BY created_at, id",
            &[&user_id],
        )?
        .into_iter()
        .map(|row| (row.get::<_, Uuid>(0), row.get::<_, Uuid>(1)))
        .collect::<Vec<_>>();

    for (player_profile_id, organization_id) in &affected_profiles {
        materialize_player_hand_features(tx, *organization_id, *player_profile_id)?;
    }

    Ok(TimezoneUpdateReport {
        user_id,
        timezone_name: timezone_name.map(str::to_string),
        affected_profiles: affected_profiles.len(),
        tournaments_recomputed,
        hands_recomputed,
    })
}
