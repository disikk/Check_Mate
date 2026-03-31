use std::env;

use anyhow::{Context, Result, anyhow};
use uuid::Uuid;

use super::row_models::ImportContext;

pub(crate) fn database_url_from_env() -> Result<String> {
    env::var("CHECK_MATE_DATABASE_URL")
        .context("CHECK_MATE_DATABASE_URL is required for parser_worker database operations")
}

pub(crate) fn load_import_context(
    client: &mut impl postgres::GenericClient,
    player_profile_id: Uuid,
) -> Result<ImportContext> {
    let row = client
        .query_opt(
            "SELECT
                player_profiles.organization_id,
                player_profiles.owner_user_id,
                player_profiles.screen_name,
                users.timezone_name
             FROM core.player_profiles AS player_profiles
             INNER JOIN auth.users AS users
                ON users.id = player_profiles.owner_user_id
             WHERE player_profiles.id = $1
               AND player_profiles.room = 'gg'",
            &[&player_profile_id],
        )?
        .ok_or_else(|| {
            anyhow!("player profile `{player_profile_id}` does not exist for room `gg`")
        })?;

    let organization_id: Uuid = row.get(0);
    let user_id: Uuid = row.get(1);
    let player_screen_name: String = row.get(2);
    let timezone_name: Option<String> = row.get(3);
    let mut player_aliases = client
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
        .map(|alias_row| alias_row.get::<_, String>(0))
        .collect::<Vec<_>>();
    if !player_aliases
        .iter()
        .any(|alias| alias == &player_screen_name)
    {
        player_aliases.push(player_screen_name.clone());
    }

    let room_id = client
        .query_one("SELECT id FROM core.rooms WHERE code = 'gg'", &[])?
        .get(0);
    let format_id = client
        .query_one("SELECT id FROM core.formats WHERE code = 'mbr'", &[])?
        .get(0);

    Ok(ImportContext {
        organization_id,
        user_id,
        player_profile_id,
        player_aliases,
        timezone_name,
        room_id,
        format_id,
    })
}

pub(crate) fn load_existing_context(
    client: &mut impl postgres::GenericClient,
    organization_id: Uuid,
    player_profile_id: Uuid,
) -> Result<ImportContext> {
    let context = load_import_context(client, player_profile_id)?;

    if context.organization_id != organization_id {
        return Err(anyhow!(
            "player profile {} is missing in organization {}",
            player_profile_id,
            organization_id
        ));
    }

    Ok(context)
}
