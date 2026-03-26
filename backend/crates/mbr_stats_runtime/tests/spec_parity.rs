//! Parity test: frozen spec (`mbr_stats_spec_v1.yml`) ↔ runtime canonical keys.
//!
//! Этот тест гарантирует, что:
//! 1. Frozen spec содержит ровно EXPECTED_MODULE_COUNT модулей.
//! 2. Frozen spec содержит ровно EXPECTED_KEY_COUNT уникальных stat keys.
//! 3. Нет дубликатов ключей внутри spec.
//! 4. Runtime `CANONICAL_STAT_KEYS` совпадает с frozen spec без пропусков и лишних ключей.
//! 5. `CANONICAL_STAT_KEYS` не содержит внутренних дубликатов.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Deserialize;

use mbr_stats_runtime::{CANONICAL_STAT_KEYS, EXPECTED_KEY_COUNT, EXPECTED_MODULE_COUNT};

// --- YAML model (только нужные поля) ---

#[derive(Deserialize)]
struct SpecRoot {
    stat_modules: Vec<SpecModule>,
}

#[derive(Deserialize)]
struct SpecModule {
    legacy_module_id: String,
    new_stat_keys: Vec<String>,
}

// --- Helpers ---

/// Путь к frozen spec относительно корня репозитория.
fn spec_path() -> PathBuf {
    // Тест запускается из backend/crates/mbr_stats_runtime/ через `cargo test`,
    // поэтому ищем корень репо через CARGO_MANIFEST_DIR → три уровня вверх.
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent() // crates/
        .and_then(|p| p.parent()) // backend/
        .and_then(|p| p.parent()) // repo root
        .expect("не удалось определить корень репо из CARGO_MANIFEST_DIR");
    repo_root.join("docs/stat_catalog/mbr_stats_spec_v1.yml")
}

fn load_spec() -> SpecRoot {
    let path = spec_path();
    let content = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("не удалось прочитать {}: {}", path.display(), e));
    serde_yaml::from_str(&content)
        .unwrap_or_else(|e| panic!("не удалось распарсить YAML {}: {}", path.display(), e))
}

// --- Tests ---

#[test]
fn frozen_spec_module_count_matches_expected() {
    let spec = load_spec();
    assert_eq!(
        spec.stat_modules.len(),
        EXPECTED_MODULE_COUNT,
        "Количество модулей в frozen spec ({}) не совпадает с EXPECTED_MODULE_COUNT ({}). \
         Если число модулей изменилось осознанно, обнови EXPECTED_MODULE_COUNT.",
        spec.stat_modules.len(),
        EXPECTED_MODULE_COUNT,
    );
}

#[test]
fn frozen_spec_unique_key_count_matches_expected() {
    let spec = load_spec();
    let unique_keys: BTreeSet<&str> = spec
        .stat_modules
        .iter()
        .flat_map(|m| m.new_stat_keys.iter().map(|k| k.as_str()))
        .collect();
    assert_eq!(
        unique_keys.len(),
        EXPECTED_KEY_COUNT,
        "Количество уникальных ключей в frozen spec ({}) не совпадает с EXPECTED_KEY_COUNT ({}). \
         Если число ключей изменилось осознанно, обнови EXPECTED_KEY_COUNT.",
        unique_keys.len(),
        EXPECTED_KEY_COUNT,
    );
}

#[test]
fn frozen_spec_has_no_duplicate_keys() {
    let spec = load_spec();
    let mut seen = BTreeSet::new();
    let mut duplicates = Vec::new();
    for module in &spec.stat_modules {
        for key in &module.new_stat_keys {
            if !seen.insert(key.as_str()) {
                duplicates.push(format!(
                    "ключ '{}' дублируется (найден в модуле '{}')",
                    key, module.legacy_module_id
                ));
            }
        }
    }
    assert!(
        duplicates.is_empty(),
        "В frozen spec найдены дубликаты ключей:\n{}",
        duplicates.join("\n")
    );
}

#[test]
fn canonical_stat_keys_constant_has_no_duplicates() {
    let mut seen = BTreeSet::new();
    let mut duplicates = Vec::new();
    for key in CANONICAL_STAT_KEYS {
        if !seen.insert(*key) {
            duplicates.push(format!("дубликат в CANONICAL_STAT_KEYS: '{}'", key));
        }
    }
    assert!(
        duplicates.is_empty(),
        "В CANONICAL_STAT_KEYS найдены дубликаты:\n{}",
        duplicates.join("\n")
    );
}

#[test]
fn canonical_stat_keys_count_matches_expected() {
    assert_eq!(
        CANONICAL_STAT_KEYS.len(),
        EXPECTED_KEY_COUNT,
        "Длина CANONICAL_STAT_KEYS ({}) не совпадает с EXPECTED_KEY_COUNT ({})",
        CANONICAL_STAT_KEYS.len(),
        EXPECTED_KEY_COUNT,
    );
}

#[test]
fn spec_keys_match_runtime_keys_exactly() {
    let spec = load_spec();
    let spec_keys: BTreeSet<&str> = spec
        .stat_modules
        .iter()
        .flat_map(|m| m.new_stat_keys.iter().map(|k| k.as_str()))
        .collect();
    let runtime_keys: BTreeSet<&str> = CANONICAL_STAT_KEYS.iter().copied().collect();

    let missing_in_runtime: Vec<&&str> = spec_keys.difference(&runtime_keys).collect();
    let extra_in_runtime: Vec<&&str> = runtime_keys.difference(&spec_keys).collect();

    let mut errors = Vec::new();
    if !missing_in_runtime.is_empty() {
        errors.push(format!(
            "Ключи есть в frozen spec, но отсутствуют в CANONICAL_STAT_KEYS: {:?}",
            missing_in_runtime
        ));
    }
    if !extra_in_runtime.is_empty() {
        errors.push(format!(
            "Ключи есть в CANONICAL_STAT_KEYS, но отсутствуют в frozen spec: {:?}",
            extra_in_runtime
        ));
    }
    assert!(
        errors.is_empty(),
        "Расхождение между frozen spec и CANONICAL_STAT_KEYS:\n{}",
        errors.join("\n")
    );
}
