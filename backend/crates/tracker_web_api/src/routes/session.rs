use anyhow::Result;
use axum::{Json, extract::State};
use postgres::{Client, GenericClient, NoTls};
use tokio::task;

use crate::dto::{SessionContext, SessionResponse};
use crate::errors::ApiError;
use crate::{AppState, StubSessionSeed};

/// GET /api/session — resolve or create the stub session.
pub(crate) async fn get_session(
    State(state): State<AppState>,
) -> Result<Json<SessionResponse>, ApiError> {
    let session = load_session_context(&state).await?;
    Ok(Json(session.into()))
}

/// Load (or upsert) the stub session from PostgreSQL.
pub(crate) async fn load_session_context(state: &AppState) -> Result<SessionContext, ApiError> {
    let seed = state.config.session_seed.clone();
    run_db_api(state, move |client| {
        let mut tx = client.transaction().map_err(ApiError::internal)?;
        let session = ensure_stub_session_context(&mut tx, &seed).map_err(ApiError::internal)?;
        tx.commit().map_err(ApiError::internal)?;
        Ok(session)
    })
    .await
}

/// Run a synchronous DB operation on a blocking thread.
pub(crate) async fn run_db_api<T, F>(state: &AppState, operation: F) -> Result<T, ApiError>
where
    T: Send + 'static,
    F: FnOnce(&mut Client) -> Result<T, ApiError> + Send + 'static,
{
    let database_url = state.config.database_url.clone();

    task::spawn_blocking(move || {
        let mut client = Client::connect(&database_url, NoTls).map_err(ApiError::internal)?;
        operation(&mut client)
    })
    .await
    .map_err(ApiError::internal)?
}

/// Ensure org/user/profile rows exist for the stub session seed.
fn ensure_stub_session_context(
    client: &mut impl GenericClient,
    seed: &StubSessionSeed,
) -> Result<SessionContext> {
    let organization_id = if let Some(row) = client.query_opt(
        "SELECT id
         FROM org.organizations
         WHERE name = $1",
        &[&seed.organization_name],
    )? {
        row.get(0)
    } else {
        client
            .query_one(
                "INSERT INTO org.organizations (name)
                 VALUES ($1)
                 RETURNING id",
                &[&seed.organization_name],
            )?
            .get(0)
    };

    let user_id = if let Some(row) = client.query_opt(
        "SELECT id
         FROM auth.users
         WHERE email = $1",
        &[&seed.user_email],
    )? {
        row.get(0)
    } else {
        client
            .query_one(
                "INSERT INTO auth.users (email, auth_provider, status)
                 VALUES ($1, 'stub_web', 'active')
                 RETURNING id",
                &[&seed.user_email],
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
        "SELECT id
         FROM core.player_profiles
         WHERE organization_id = $1
           AND room = 'gg'
           AND screen_name = $2",
        &[&organization_id, &seed.player_screen_name],
    )? {
        row.get(0)
    } else {
        client
            .query_one(
                "INSERT INTO core.player_profiles (
                    organization_id,
                    owner_user_id,
                    room,
                    network,
                    screen_name
                )
                VALUES ($1, $2, 'gg', 'gg', $3)
                RETURNING id",
                &[&organization_id, &user_id, &seed.player_screen_name],
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
        VALUES ($1, $2, 'gg', $3, TRUE, 'stub_web_session')
        ON CONFLICT (player_profile_id, room, alias)
        DO UPDATE SET
            is_primary = TRUE,
            source = EXCLUDED.source",
        &[
            &organization_id,
            &player_profile_id,
            &seed.player_screen_name,
        ],
    )?;

    Ok(SessionContext {
        user_id,
        user_email: seed.user_email.clone(),
        organization_id,
        organization_name: seed.organization_name.clone(),
        player_profile_id,
        player_screen_name: seed.player_screen_name.clone(),
    })
}
