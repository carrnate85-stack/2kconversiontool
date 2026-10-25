use serde_json::{Value, json};
use std::sync::OnceLock;

pub fn describe(asset: &str) -> Option<(String, String)> {
    let rest = asset.strip_prefix("shoe_")?;
    let (id, colorway) = rest.split_once('_')?;
    if id.len() != 4
        || !id.bytes().all(|b| b.is_ascii_digit())
        || colorway.is_empty()
        || !colorway
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return None;
    }
    static MODELS: OnceLock<Value> = OnceLock::new();
    let models = MODELS.get_or_init(|| {
        serde_json::from_str(include_str!("../assets/shoe-model-display.json"))
            .expect("generated shoe display metadata")
    });
    let name = models[id]["name"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Unidentified shoe {id}"));
    let brand = models[id]["brand"]
        .as_str()
        .unwrap_or("Unidentified")
        .to_string();
    if id == "0000" && colorway == "default" {
        return Some((name, brand));
    }
    Some((format!("{name} · {}", colorway.replace('_', " ")), brand))
}
pub fn enrich(row: &mut Value) {
    if row["category"] == "shoes" {
        if let Some((label, brand)) = row["asset"].as_str().and_then(describe) {
            row["label"] = json!(label);
            row["group"] = json!(brand);
        }
    }
}
