use anyhow::{Context, Result, anyhow, bail};
use reqwest::blocking::Client;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const HF_DATASETS_SERVER_URL: &str = "https://datasets-server.huggingface.co/rows";
const HF_DATASET_CONFIG: &str = "default";
const HF_ROWS_PAGE_SIZE: usize = 100;

#[derive(Debug, Clone)]
pub(super) struct PreparationRow {
    pub(super) repo: String,
    pub(super) base_commit: String,
    pub(super) environment_setup_commit: String,
}

pub(super) fn normalize_preparation_row(row: &Value, instance_id: &str) -> Result<PreparationRow> {
    let repo = required_row_string(row, "repo", instance_id)?;
    let base_commit = required_row_string(row, "base_commit", instance_id)?;
    let environment_setup_commit = optional_row_string(row, "environment_setup_commit");
    Ok(PreparationRow {
        repo,
        base_commit,
        environment_setup_commit,
    })
}

pub(super) fn required_row_string(row: &Value, key: &str, instance_id: &str) -> Result<String> {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow!("Dataset row for {instance_id} must include non-empty {key}"))
}

fn optional_row_string(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_default()
}

pub(super) fn read_instance_ids(path: &Path) -> Result<Vec<String>> {
    let mut instance_ids = Vec::new();
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read instance id file {}", path.display()))?;
    for raw_line in contents.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        instance_ids.push(line.to_string());
    }
    if instance_ids.is_empty() {
        bail!("No instance ids found in {}", path.display());
    }
    Ok(instance_ids)
}

pub(super) fn load_dataset_rows(
    dataset_files: &[PathBuf],
    dataset_name: Option<&str>,
    split: &str,
) -> Result<Vec<Value>> {
    let mut dataset_rows = Vec::new();
    for dataset_file in dataset_files {
        dataset_rows.extend(load_rows_from_dataset_file(dataset_file)?);
    }
    if let Some(dataset_name) = dataset_name {
        dataset_rows.extend(load_rows_from_hf_dataset(dataset_name, split)?);
    }
    Ok(dataset_rows)
}

fn load_rows_from_dataset_file(path: &Path) -> Result<Vec<Value>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read dataset file {}", path.display()))?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let first = trimmed.chars().next().unwrap_or_default();
    if matches!(first, '{' | '[') {
        match serde_json::from_str::<Value>(trimmed) {
            Ok(payload) => return load_rows_from_json_payload(payload),
            Err(err) if first == '{' => {
                let _ = err;
            }
            Err(err) => {
                return Err(err)
                    .with_context(|| format!("Failed to parse dataset file {}", path.display()));
            }
        }
    }

    let mut rows = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line).with_context(|| {
            format!(
                "Failed to parse JSONL row in dataset file {}",
                path.display()
            )
        })?;
        rows.push(Value::Object(normalize_row(value)?));
    }
    Ok(rows)
}

fn load_rows_from_json_payload(payload: Value) -> Result<Vec<Value>> {
    match payload {
        Value::Object(map) => {
            if let Some(Value::Array(rows)) = map.get("rows") {
                rows.iter()
                    .cloned()
                    .map(|item| normalize_row(item).map(Value::Object))
                    .collect()
            } else if map.contains_key("instance_id") {
                Ok(vec![Value::Object(normalize_row(Value::Object(map))?)])
            } else {
                bail!("Unsupported JSON object payload; expected rows[] or an instance row")
            }
        }
        Value::Array(items) => items
            .into_iter()
            .map(|item| normalize_row(item).map(Value::Object))
            .collect(),
        other => bail!(
            "Unsupported JSON payload; expected an object or array, got {}",
            value_kind_name(&other)
        ),
    }
}

fn normalize_row(item: Value) -> Result<Map<String, Value>> {
    match item {
        Value::Object(mut map) => {
            if let Some(Value::Object(row)) = map.remove("row") {
                return Ok(row);
            }
            Ok(map)
        }
        other => bail!(
            "Unsupported dataset row payload: {}",
            value_kind_name(&other)
        ),
    }
}

fn load_rows_from_hf_dataset(dataset_name: &str, split: &str) -> Result<Vec<Value>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .context("Failed to build HTTP client for Hugging Face dataset fetch")?;

    let mut rows = Vec::new();
    let mut offset = 0usize;
    loop {
        let url = format!(
            "{base}?dataset={dataset}&config={config}&split={split}&offset={offset}&length={length}",
            base = HF_DATASETS_SERVER_URL,
            dataset = urlencoding::encode(dataset_name),
            config = urlencoding::encode(HF_DATASET_CONFIG),
            split = urlencoding::encode(split),
            offset = offset,
            length = HF_ROWS_PAGE_SIZE,
        );
        let response = client
            .get(url)
            .send()
            .with_context(|| {
                format!(
                    "Failed to fetch dataset rows for {dataset_name} split {split} from Hugging Face"
                )
            })?
            .error_for_status()
            .with_context(|| {
                format!(
                    "Hugging Face datasets-server returned an error for {dataset_name} split {split}"
                )
            })?;
        let payload: Value = response.json().with_context(|| {
            format!("Failed to parse Hugging Face row payload for {dataset_name}")
        })?;
        let page_rows = load_rows_from_json_payload(payload)?;
        let page_count = page_rows.len();
        if page_count == 0 {
            break;
        }
        rows.extend(page_rows);
        if page_count < HF_ROWS_PAGE_SIZE {
            break;
        }
        offset += HF_ROWS_PAGE_SIZE;
    }
    Ok(rows)
}

pub(super) fn build_row_index(rows: Vec<Value>) -> BTreeMap<String, Value> {
    let mut index = BTreeMap::new();
    for row in rows {
        if let Some(instance_id) = row.get("instance_id").and_then(Value::as_str) {
            index.entry(instance_id.to_string()).or_insert(row);
        }
    }
    index
}

fn value_kind_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
