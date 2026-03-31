use axum::{Json, extract::{Query, State}, http::StatusCode};
use mbr_stats_runtime::{FtDashboardFilters, query_ft_dashboard};
use postgres::GenericClient;
use uuid::Uuid;

use crate::AppState;
use crate::dto::{FtDashboardQuery, SessionContext};
use crate::errors::ApiError;
use crate::routes::session::{load_session_context, run_db_api};

/// GET /api/ft/dashboard — page-specific MBR/FT snapshot.
pub(crate) async fn get_ft_dashboard(
    State(state): State<AppState>,
    Query(query): Query<FtDashboardQuery>,
) -> Result<Json<mbr_stats_runtime::FtDashboardSnapshot>, ApiError> {
    let session = load_session_context(&state).await?;
    let timezone_name = query.timezone.ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "timezone query parameter is required",
        )
    })?;
    let buyin_total_cents = parse_buyin_filter(query.buyin.as_deref())?;
    validate_local_datetime_range(query.date_from.as_deref(), query.date_to.as_deref())?;

    let snapshot = run_db_api(&state, move |client| {
        validate_timezone_name(client, &timezone_name)?;
        if let Some(bundle_id) = query.bundle_id {
            ensure_bundle_access(client, bundle_id, &session)?;
        }
        validate_local_datetime(client, query.date_from.as_deref(), "date_from")?;
        validate_local_datetime(client, query.date_to.as_deref(), "date_to")?;

        query_ft_dashboard(
            client,
            FtDashboardFilters {
                organization_id: session.organization_id,
                player_profile_id: session.player_profile_id,
                buyin_total_cents,
                bundle_id: query.bundle_id,
                date_from_local: query.date_from,
                date_to_local: query.date_to,
                timezone_name,
            },
        )
        .map_err(ApiError::internal)
    })
    .await?;

    Ok(Json(snapshot))
}

/// Verify that a bundle belongs to the current session.
pub(crate) fn ensure_bundle_access(
    client: &mut impl GenericClient,
    bundle_id: Uuid,
    session: &SessionContext,
) -> Result<(), ApiError> {
    let Some(row) = client
        .query_opt(
            "SELECT organization_id, player_profile_id
             FROM import.ingest_bundles
             WHERE id = $1",
            &[&bundle_id],
        )
        .map_err(ApiError::internal)?
    else {
        return Err(ApiError::new(
            StatusCode::NOT_FOUND,
            format!("bundle `{bundle_id}` was not found"),
        ));
    };

    let organization_id: Uuid = row.get(0);
    let player_profile_id: Uuid = row.get(1);
    if organization_id != session.organization_id || player_profile_id != session.player_profile_id
    {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "bundle is outside the current session scope",
        ));
    }

    Ok(())
}

fn parse_buyin_filter(buyin: Option<&str>) -> Result<Option<Vec<i64>>, ApiError> {
    match buyin {
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => value
            .parse::<i64>()
            .map(|parsed| Some(vec![parsed]))
            .map_err(|_| {
                ApiError::new(
                    StatusCode::BAD_REQUEST,
                    "buyin must be an integer amount in cents",
                )
            }),
        None => Ok(None),
    }
}

fn validate_local_datetime_range(
    date_from: Option<&str>,
    date_to: Option<&str>,
) -> Result<(), ApiError> {
    if let (Some(date_from), Some(date_to)) = (date_from, date_to)
        && date_from > date_to
    {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "date_from must be less than or equal to date_to",
        ));
    }

    Ok(())
}

fn validate_timezone_name(
    client: &mut impl GenericClient,
    timezone_name: &str,
) -> Result<(), ApiError> {
    client
        .query_one("SELECT now() AT TIME ZONE $1", &[&timezone_name])
        .map_err(|_| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("invalid IANA timezone `{timezone_name}`"),
            )
        })?;
    Ok(())
}

fn validate_local_datetime(
    client: &mut impl GenericClient,
    value: Option<&str>,
    field_name: &str,
) -> Result<(), ApiError> {
    let Some(value) = value else {
        return Ok(());
    };

    client
        .query_one("SELECT replace($1, 'T', ' ')::timestamp", &[&value])
        .map_err(|_| {
            ApiError::new(
                StatusCode::BAD_REQUEST,
                format!("{field_name} must be a valid datetime-local value"),
            )
        })?;
    Ok(())
}
