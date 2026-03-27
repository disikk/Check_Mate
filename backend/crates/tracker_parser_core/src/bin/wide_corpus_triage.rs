use std::{env, fs, path::PathBuf};

use tracker_parser_core::wide_corpus_triage::{
    WideCorpusTriageConfig, default_allowed_parse_issue_codes,
    default_committed_quarantine_sample_root, default_local_quarantine_root,
    run_wide_corpus_triage,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args().skip(1);
    let mut json_out = default_json_output_path();
    let mut local_root: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json-out" => {
                let Some(path) = args.next() else {
                    return Err("--json-out requires a path".into());
                };
                json_out = PathBuf::from(path);
            }
            "--local-root" => {
                let Some(path) = args.next() else {
                    return Err("--local-root requires a path".into());
                };
                local_root = Some(PathBuf::from(path));
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }

    let mut roots = vec![default_committed_quarantine_sample_root()];
    if let Some(local_root) = local_root {
        roots.push(local_root);
    } else {
        let default_local_root = default_local_quarantine_root();
        if default_local_root.exists() {
            roots.push(default_local_root);
        }
    }

    let report = run_wide_corpus_triage(WideCorpusTriageConfig {
        roots: roots.clone(),
        allowed_issue_codes: default_allowed_parse_issue_codes(),
        example_limit: 5,
    })?;

    if let Some(parent) = json_out.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&json_out, serde_json::to_string_pretty(&report)?)?;

    println!("wide corpus triage completed");
    println!("roots:");
    for root in roots {
        println!("- {}", root.display());
    }
    println!("source_files_total={}", report.source_files_total);
    println!("source_files_parsed_ok={}", report.source_files_parsed_ok);
    println!("source_files_failed={}", report.source_files_failed);
    println!("hh_files_total={}", report.hh_files_total);
    println!("ts_files_total={}", report.ts_files_total);
    println!("hands_total={}", report.hands_total);
    println!("hands_normalized_exact={}", report.hands_normalized_exact);
    println!(
        "hands_normalized_uncertain={}",
        report.hands_normalized_uncertain
    );
    println!(
        "hands_normalized_inconsistent={}",
        report.hands_normalized_inconsistent
    );
    println!("allowed_issue_count={}", report.allowed_issue_count);
    println!("unexpected_issue_count={}", report.unexpected_issue_count);
    println!(
        "hands_with_unexpected_parse_issues={}",
        report.hands_with_unexpected_parse_issues
    );
    println!("json_report={}", json_out.display());

    Ok(())
}

fn default_json_output_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/wide_corpus_triage/latest_report.json")
}
