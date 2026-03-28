use std::{
    collections::VecDeque,
    fs,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::Duration,
};

use futures_util::StreamExt;
use parser_worker::local_import::run_ingest_runner_until_idle;
use postgres::{Client as PgClient, NoTls};
use reqwest::Client;
use serde_json::Value;
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tracker_ingest_runtime::{JobExecutionError, JobExecutor, run_next_job};
use tracker_web_api::{StubSessionSeed, WebApiConfig, serve};
use uuid::Uuid;

fn backend_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("backend root must exist")
        .to_path_buf()
}

fn migrations_dir() -> PathBuf {
    backend_root().join("migrations")
}

fn fixture_path(relative: &str) -> PathBuf {
    backend_root().join(relative)
}

fn apply_all_migrations(client: &mut PgClient) {
    let mut paths = fs::read_dir(migrations_dir())
        .expect("migrations dir must exist")
        .map(|entry| entry.expect("entry must load").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("sql"))
        .collect::<Vec<_>>();
    paths.sort();

    for path in paths {
        let sql = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read migration {}: {error}", path.display()));
        client
            .batch_execute(&sql)
            .unwrap_or_else(|error| panic!("failed to apply {}: {error}", path.display()));
    }
}

fn db_url() -> String {
    std::env::var("CHECK_MATE_DATABASE_URL")
        .expect("CHECK_MATE_DATABASE_URL must exist for tracker_web_api DB tests")
}

fn db_test_guard() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn reset_ingest_runtime_tables(client: &mut PgClient) {
    client
        .batch_execute(
            "DELETE FROM import.ingest_events;
             DELETE FROM import.job_attempts;
             DELETE FROM import.import_jobs;
             DELETE FROM import.ingest_bundle_files;
             DELETE FROM import.ingest_bundles;",
        )
        .unwrap();
}

async fn prepare_database(database_url: String) {
    tokio::task::spawn_blocking(move || {
        let mut db = PgClient::connect(&database_url, NoTls).unwrap();
        apply_all_migrations(&mut db);
        reset_ingest_runtime_tables(&mut db);
    })
    .await
    .unwrap();
}

fn unique_seed(label: &str) -> StubSessionSeed {
    let suffix = Uuid::new_v4();
    StubSessionSeed {
        organization_name: format!("web-org-{label}-{suffix}"),
        user_email: format!("web-{label}-{suffix}@example.com"),
        player_screen_name: format!("Hero-{label}-{suffix}"),
    }
}

fn unique_spool_dir(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("check-mate-web-{label}-{}", Uuid::new_v4()))
}

async fn spawn_test_server(config: WebApiConfig) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let handle = tokio::spawn(async move {
        serve(listener, config).await.unwrap();
    });
    (base_url, handle)
}

async fn upload_bundle(http: &Client, base_url: &str, files: &[PathBuf]) -> Value {
    let mut form = reqwest::multipart::Form::new();
    for path in files {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let bytes = fs::read(path).unwrap();
        form = form.part(
            "files",
            reqwest::multipart::Part::bytes(bytes).file_name(filename),
        );
    }

    let response = http
        .post(format!("{base_url}/api/ingest/bundles"))
        .multipart(form)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    response.json().await.unwrap()
}

async fn run_real_ingest_runner() {
    let database_url = db_url();
    let processed_jobs = tokio::task::spawn_blocking(move || {
        run_ingest_runner_until_idle(&database_url, "tracker_web_api_ft_dashboard_test", 3)
            .expect("real ingest runner")
    })
    .await
    .unwrap();
    assert!(
        processed_jobs > 0,
        "runner must process at least one ingest job"
    );
}

struct SuccessExecutor {
    file_results: VecDeque<Result<(), JobExecutionError>>,
    finalize_calls: usize,
}

impl JobExecutor for SuccessExecutor {
    fn execute_file_job<C: postgres::GenericClient>(
        &mut self,
        _client: &mut C,
        _job: &tracker_ingest_runtime::ClaimedJob,
    ) -> Result<(), JobExecutionError> {
        self.file_results.pop_front().unwrap_or(Ok(()))
    }

    fn finalize_bundle<C: postgres::GenericClient>(
        &mut self,
        _client: &mut C,
        _job: &tracker_ingest_runtime::ClaimedJob,
    ) -> Result<(), JobExecutionError> {
        self.finalize_calls += 1;
        Ok(())
    }
}

#[tokio::test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
async fn session_upload_and_snapshot_endpoints_work_on_real_backend_contract() {
    let _guard = db_test_guard();
    prepare_database(db_url()).await;

    let spool_dir = unique_spool_dir("session-upload");
    fs::create_dir_all(&spool_dir).unwrap();
    let config = WebApiConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        database_url: db_url(),
        spool_dir: spool_dir.clone(),
        session_seed: unique_seed("session-upload"),
        ws_poll_interval: Duration::from_millis(50),
    };
    let (base_url, handle) = spawn_test_server(config.clone()).await;
    let http = Client::new();

    let session_response = http
        .get(format!("{base_url}/api/session"))
        .send()
        .await
        .unwrap();
    assert_eq!(session_response.status(), 200);
    let session_json: Value = session_response.json().await.unwrap();
    assert_eq!(
        session_json
            .get("organization_name")
            .and_then(Value::as_str),
        Some(config.session_seed.organization_name.as_str())
    );
    assert_eq!(
        session_json
            .get("player_screen_name")
            .and_then(Value::as_str),
        Some(config.session_seed.player_screen_name.as_str())
    );

    let ts_path = fixture_path(
        "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
    );
    let ts_bytes = fs::read(&ts_path).unwrap();
    let ts_filename = ts_path.file_name().unwrap().to_string_lossy().to_string();
    let upload_response = http
        .post(format!("{base_url}/api/ingest/bundles"))
        .multipart(reqwest::multipart::Form::new().part(
            "files",
            reqwest::multipart::Part::bytes(ts_bytes).file_name(ts_filename.clone()),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(upload_response.status(), 200);
    let upload_json: Value = upload_response.json().await.unwrap();

    let bundle_id = upload_json
        .get("bundle_id")
        .and_then(Value::as_str)
        .expect("bundle_id must exist")
        .to_string();
    let snapshot = upload_json.get("snapshot").expect("snapshot must exist");
    assert_eq!(
        snapshot.get("status").and_then(Value::as_str),
        Some("queued")
    );
    assert_eq!(snapshot.get("total_files").and_then(Value::as_i64), Some(1));
    assert_eq!(
        snapshot
            .get("files")
            .and_then(Value::as_array)
            .and_then(|files| files.first())
            .and_then(|file| file.get("member_path"))
            .and_then(Value::as_str),
        Some(ts_filename.as_str())
    );
    assert_eq!(
        snapshot
            .get("files")
            .and_then(Value::as_array)
            .and_then(|files| files.first())
            .and_then(|file| file.get("stage_label"))
            .and_then(Value::as_str),
        Some("Проверка структуры")
    );

    let spool_entries = fs::read_dir(&spool_dir).unwrap().count();
    assert_eq!(spool_entries, 1);

    let snapshot_response = http
        .get(format!("{base_url}/api/ingest/bundles/{bundle_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(snapshot_response.status(), 200);
    let snapshot_json: Value = snapshot_response.json().await.unwrap();
    assert_eq!(
        snapshot_json.get("bundle_id").and_then(Value::as_str),
        Some(bundle_id.as_str())
    );
    assert_eq!(
        snapshot_json.get("total_files").and_then(Value::as_i64),
        Some(1)
    );

    handle.abort();
    let _ = fs::remove_dir_all(spool_dir);
}

#[tokio::test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
async fn websocket_streams_initial_snapshot_and_ordered_runtime_updates() {
    let _guard = db_test_guard();
    prepare_database(db_url()).await;

    let spool_dir = unique_spool_dir("ws-stream");
    fs::create_dir_all(&spool_dir).unwrap();
    let config = WebApiConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        database_url: db_url(),
        spool_dir: spool_dir.clone(),
        session_seed: unique_seed("ws-stream"),
        ws_poll_interval: Duration::from_millis(25),
    };
    let (base_url, handle) = spawn_test_server(config).await;
    let http = Client::new();

    let ts_path = fixture_path(
        "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
    );
    let ts_bytes = fs::read(&ts_path).unwrap();
    let ts_name = ts_path.file_name().unwrap().to_string_lossy().to_string();

    let archive_path = spool_dir.join("upload-source.zip");
    let archive_file = fs::File::create(&archive_path).unwrap();
    let mut writer = zip::ZipWriter::new(archive_file);
    writer
        .start_file(ts_name.clone(), zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(&ts_bytes).unwrap();
    writer
        .start_file("notes/readme.md", zip::write::SimpleFileOptions::default())
        .unwrap();
    writer.write_all(b"unsupported").unwrap();
    writer.finish().unwrap();
    let zip_bytes = fs::read(&archive_path).unwrap();

    let upload_response = http
        .post(format!("{base_url}/api/ingest/bundles"))
        .multipart(reqwest::multipart::Form::new().part(
            "files",
            reqwest::multipart::Part::bytes(zip_bytes).file_name("bundle.zip"),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(upload_response.status(), 200);
    let upload_json: Value = upload_response.json().await.unwrap();
    let bundle_id = upload_json
        .get("bundle_id")
        .and_then(Value::as_str)
        .expect("bundle_id must exist")
        .to_string();

    let ws_url = format!(
        "{}/api/ingest/bundles/{bundle_id}/ws",
        base_url.replacen("http://", "ws://", 1)
    );
    let (mut ws_stream, _) = connect_async(ws_url).await.unwrap();
    let initial_message = ws_stream.next().await.unwrap().unwrap();
    let initial_json: Value =
        serde_json::from_str(initial_message.into_text().unwrap().as_str()).unwrap();
    assert_eq!(
        initial_json.get("type").and_then(Value::as_str),
        Some("bundle_snapshot")
    );
    assert_eq!(
        initial_json
            .get("data")
            .and_then(|data| data.get("total_files"))
            .and_then(Value::as_i64),
        Some(1)
    );

    let bundle_id_for_runtime = Uuid::parse_str(&bundle_id).unwrap();
    let db_url_for_runtime = db_url();
    let runtime_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(75)).await;
        tokio::task::spawn_blocking(move || {
            let mut client = PgClient::connect(&db_url_for_runtime, NoTls).unwrap();
            let mut tx = client.transaction().unwrap();
            let mut executor = SuccessExecutor {
                file_results: VecDeque::from(vec![Ok(())]),
                finalize_calls: 0,
            };

            let first = run_next_job(&mut tx, "api-smoke-ws", 3, &mut executor).unwrap();
            assert!(first.is_some());
            let second = run_next_job(&mut tx, "api-smoke-ws", 3, &mut executor).unwrap();
            assert!(second.is_some());
            assert_eq!(executor.finalize_calls, 1);
            tx.commit().unwrap();
            bundle_id_for_runtime
        })
        .await
        .unwrap()
    });

    let mut message_types = Vec::new();
    while let Some(message) = ws_stream.next().await {
        let text = message.unwrap().into_text().unwrap();
        let json: Value = serde_json::from_str(text.as_str()).unwrap();
        let message_type = json
            .get("type")
            .and_then(Value::as_str)
            .unwrap()
            .to_string();
        message_types.push(message_type.clone());
        if message_type == "bundle_terminal" {
            break;
        }
    }

    assert_eq!(
        message_types,
        vec![
            "file_updated".to_string(),
            "bundle_updated".to_string(),
            "file_updated".to_string(),
            "bundle_updated".to_string(),
            "bundle_terminal".to_string(),
        ]
    );

    runtime_task.await.unwrap();
    handle.abort();
    let _ = fs::remove_dir_all(spool_dir);
}

#[tokio::test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
async fn ft_dashboard_endpoint_returns_live_snapshot_and_respects_filters() {
    let _guard = db_test_guard();
    prepare_database(db_url()).await;

    let spool_dir = unique_spool_dir("ft-dashboard");
    fs::create_dir_all(&spool_dir).unwrap();
    let config = WebApiConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        database_url: db_url(),
        spool_dir: spool_dir.clone(),
        session_seed: unique_seed("ft-dashboard"),
        ws_poll_interval: Duration::from_millis(25),
    };
    let (base_url, handle) = spawn_test_server(config).await;
    let http = Client::new();

    let first_bundle_json = upload_bundle(
        &http,
        &base_url,
        &[
            fixture_path(
                "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
            ),
            fixture_path("fixtures/mbr/hh/GG20260316-0344 - Mystery Battle Royale 25.txt"),
        ],
    )
    .await;
    let second_bundle_json = upload_bundle(
        &http,
        &base_url,
        &[
            fixture_path(
                "fixtures/mbr/ts/GG20260316 - Tournament #271769772 - Mystery Battle Royale 25.txt",
            ),
            fixture_path("fixtures/mbr/hh/GG20260316-0342 - Mystery Battle Royale 25.txt"),
        ],
    )
    .await;

    run_real_ingest_runner().await;

    let first_bundle_id = first_bundle_json
        .get("bundle_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();
    let second_bundle_id = second_bundle_json
        .get("bundle_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();

    let response = http
        .get(format!(
            "{base_url}/api/ft/dashboard?timezone=Asia/Krasnoyarsk"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let dashboard: Value = response.json().await.unwrap();

    assert_eq!(
        dashboard.get("data_state").and_then(Value::as_str),
        Some("ready")
    );
    assert_eq!(
        dashboard
            .get("coverage")
            .and_then(|coverage| coverage.get("summary_tournament_count"))
            .and_then(Value::as_u64),
        Some(2)
    );
    assert_eq!(
        dashboard
            .get("coverage")
            .and_then(|coverage| coverage.get("hand_tournament_count"))
            .and_then(Value::as_u64),
        Some(2)
    );
    let bundle_ids = dashboard
        .get("filter_options")
        .and_then(|options| options.get("bundle_options"))
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .filter_map(|option| option.get("bundle_id").and_then(Value::as_str))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        bundle_ids,
        std::collections::BTreeSet::from([first_bundle_id.as_str(), second_bundle_id.as_str(),])
    );
    assert_eq!(
        dashboard
            .get("selected_filters")
            .and_then(|filters| filters.get("timezone_name"))
            .and_then(Value::as_str),
        Some("Asia/Krasnoyarsk")
    );

    let first_bundle_response = http
        .get(format!(
            "{base_url}/api/ft/dashboard?timezone=Asia/Krasnoyarsk&bundle_id={first_bundle_id}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(first_bundle_response.status(), 200);
    let first_bundle_dashboard: Value = first_bundle_response.json().await.unwrap();
    assert_eq!(
        first_bundle_dashboard
            .get("coverage")
            .and_then(|coverage| coverage.get("summary_tournament_count"))
            .and_then(Value::as_u64),
        Some(1)
    );

    let empty_response = http
        .get(format!(
            "{base_url}/api/ft/dashboard?timezone=Asia/Krasnoyarsk&buyin=999999"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(empty_response.status(), 200);
    let empty_dashboard: Value = empty_response.json().await.unwrap();
    assert_eq!(
        empty_dashboard.get("data_state").and_then(Value::as_str),
        Some("empty")
    );

    let date_response = http
        .get(format!(
            "{base_url}/api/ft/dashboard?timezone=Asia/Krasnoyarsk&date_from=2026-03-17T00:00"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(date_response.status(), 200);
    let date_dashboard: Value = date_response.json().await.unwrap();
    assert_eq!(
        date_dashboard.get("data_state").and_then(Value::as_str),
        Some("empty")
    );

    handle.abort();
    let _ = fs::remove_dir_all(spool_dir);
}

#[tokio::test]
#[ignore = "requires CHECK_MATE_DATABASE_URL and local PostgreSQL"]
async fn ft_dashboard_endpoint_rejects_invalid_filters_and_preserves_session_scope() {
    let _guard = db_test_guard();
    prepare_database(db_url()).await;

    let session_one_spool = unique_spool_dir("ft-dashboard-scope-a");
    fs::create_dir_all(&session_one_spool).unwrap();
    let session_one_config = WebApiConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        database_url: db_url(),
        spool_dir: session_one_spool.clone(),
        session_seed: unique_seed("ft-dashboard-scope-a"),
        ws_poll_interval: Duration::from_millis(25),
    };
    let (session_one_base_url, session_one_handle) = spawn_test_server(session_one_config).await;
    let http = Client::new();

    let ts_only_bundle = upload_bundle(
        &http,
        &session_one_base_url,
        &[fixture_path(
            "fixtures/mbr/ts/GG20260316 - Tournament #271770266 - Mystery Battle Royale 25.txt",
        )],
    )
    .await;
    run_real_ingest_runner().await;

    let ts_only_bundle_id = ts_only_bundle
        .get("bundle_id")
        .and_then(Value::as_str)
        .unwrap()
        .to_string();

    let partial_response = http
        .get(format!(
            "{session_one_base_url}/api/ft/dashboard?timezone=Asia/Krasnoyarsk&bundle_id={ts_only_bundle_id}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(partial_response.status(), 200);
    let partial_dashboard: Value = partial_response.json().await.unwrap();
    assert_eq!(
        partial_dashboard.get("data_state").and_then(Value::as_str),
        Some("partial")
    );
    assert_eq!(
        partial_dashboard
            .get("stat_cards")
            .and_then(|cards| cards.get("avgFtStack"))
            .and_then(|card| card.get("state"))
            .and_then(Value::as_str),
        Some("blocked")
    );
    assert_eq!(
        partial_dashboard
            .get("charts")
            .and_then(|charts| charts.get("ft_stack"))
            .and_then(|chart| chart.get("state"))
            .and_then(Value::as_str),
        Some("blocked")
    );

    let invalid_timezone_response = http
        .get(format!(
            "{session_one_base_url}/api/ft/dashboard?timezone=Mars/OlympusMons"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_timezone_response.status(), 400);

    let invalid_date_response = http
        .get(format!(
            "{session_one_base_url}/api/ft/dashboard?timezone=Asia/Krasnoyarsk&date_from=broken-date"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_date_response.status(), 400);

    let session_two_spool = unique_spool_dir("ft-dashboard-scope-b");
    fs::create_dir_all(&session_two_spool).unwrap();
    let session_two_config = WebApiConfig {
        bind_addr: "127.0.0.1:0".parse().unwrap(),
        database_url: db_url(),
        spool_dir: session_two_spool.clone(),
        session_seed: unique_seed("ft-dashboard-scope-b"),
        ws_poll_interval: Duration::from_millis(25),
    };
    let (session_two_base_url, session_two_handle) = spawn_test_server(session_two_config).await;

    let forbidden_response = http
        .get(format!(
            "{session_two_base_url}/api/ft/dashboard?timezone=Asia/Krasnoyarsk&bundle_id={ts_only_bundle_id}"
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(forbidden_response.status(), 403);

    session_two_handle.abort();
    session_one_handle.abort();
    let _ = fs::remove_dir_all(session_one_spool);
    let _ = fs::remove_dir_all(session_two_spool);
}
