//! Read-only existing-season import planning. No process access, leases or writes.
//! Destination IDs/rows must come from the current native-owned read_seasons call,
//! never caller-supplied destination data. Existing save revalidates retained tokens.
use anyhow::{Context, Result, ensure};
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

pub(crate) fn capabilities() -> Value {
    json!({"existingRowsOnly":true,"nonsharedOnly":true,"structure":false,"bulkApply":false,"maxStagedRows":1})
}
fn index(v: &Value) -> Result<u64> {
    let n = v.as_u64().context("Choose a verified destination team.")?;
    ensure!(n < 1024, "Invalid destination team index.");
    Ok(n)
}
fn name(v: &Value) -> Result<&str> {
    let s = v.as_str().context("Season team name is missing.")?;
    ensure!(
        !s.trim().is_empty() && s.chars().count() <= 100 && !s.chars().any(|c| c.is_control()),
        "Invalid season team name."
    );
    Ok(s)
}
/// Source contains only imported values. Destination rows carry private numeric
/// primaryTeamIndex/secondaryTeamIndex enrichment, removed before IPC output.
/// The pure planner never treats display-name equality as an ownership proof.
pub(crate) fn review(
    source: &Value,
    destination_rows: &Value,
    teams: &Value,
    options: &Value,
) -> Result<Value> {
    let source = source
        .as_array()
        .context("Invalid source season history.")?;
    ensure!(
        !source.is_empty() && source.len() <= 25,
        "Choose 1–25 seasons."
    );
    let destination = destination_rows
        .as_array()
        .context("Refresh destination season history.")?;
    ensure!(
        destination.len() <= 25,
        "Invalid destination season history."
    );
    let team_rows = teams
        .as_array()
        .context("Refresh verified destination teams.")?;
    ensure!(
        !team_rows.is_empty() && team_rows.len() <= 1024,
        "Invalid destination team directory."
    );
    let mut directory = HashMap::new();
    for team in team_rows {
        let id = index(&team["index"])?;
        name(&team["name"])?;
        ensure!(
            directory.insert(id, team).is_none(),
            "Ambiguous destination team directory."
        );
    }
    let resolve = |label: &Value, mapped: Option<&Value>| -> Result<Option<u64>> {
        if label.is_null() && mapped.is_none_or(Value::is_null) {
            return Ok(None);
        }
        let id = if let Some(v) = mapped.filter(|v| !v.is_null()) {
            index(v)?
        } else {
            let label = name(label)?;
            let mut candidates = directory
                .iter()
                .filter(|(_, t)| t["name"].as_str() == Some(label));
            let id = *candidates
                .next()
                .context("Choose a verified destination team mapping.")?
                .0;
            ensure!(
                candidates.next().is_none(),
                "Team name is ambiguous; choose its destination team."
            );
            id
        };
        let team = directory
            .get(&id)
            .context("Destination team is not in the current roster.")?;
        ensure!(
            options["normalTeamsOnly"] != true || team["normal"] == true,
            "Choose a normal destination team."
        );
        Ok(Some(id))
    };
    let mut years = HashSet::new();
    let mut plans = Vec::new();
    for source in source {
        let year = source["year"]
            .as_u64()
            .filter(|y| (1900..=2155).contains(y))
            .context("Invalid season year.")?;
        ensure!(
            years.insert(year),
            "Choose one aggregate row per season year."
        );
        name(&source["team"])?;
        let year_key = year.to_string();
        let mapping = &options["teamMappings"][&year_key];
        let primary = resolve(&source["team"], mapping.get("primaryTeamIndex"))?
            .context("Destination primary team is missing.")?;
        let secondary = resolve(&source["secondaryTeam"], mapping.get("secondaryTeamIndex"))?;
        let mut candidates = destination
            .iter()
            .filter(|r| r["year"].as_u64() == Some(year));
        let old = candidates
            .next()
            .context("This season does not exist. Creating seasons is not supported yet.")?;
        ensure!(
            candidates.next().is_none(),
            "Destination season year is ambiguous."
        );
        ensure!(
            old["primaryTeamIndex"].as_u64() == Some(primary)
                && old["secondaryTeamIndex"].as_u64() == secondary,
            "The selected team does not match the existing season. Changing season teams is not supported yet."
        );
        ensure!(
            old["sharedWarning"].is_null() && old["linkedCount"].as_u64() == Some(1),
            "This season is shared by other player entries. Import requires private-copy support; no draft was staged."
        );
        let slot = old["slot"]
            .as_u64()
            .filter(|s| *s < 25)
            .context("Invalid destination season slot.")?;
        let token = old["token"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .context("Refresh destination season tokens.")?;
        let totals = source["totals"]
            .as_object()
            .context("Imported season totals are missing.")?;
        let mut merged = old["totals"]
            .as_object()
            .context("Destination totals are missing.")?
            .clone();
        let mut preserve = HashSet::new();
        if let Some(fields) = options["preserveTotals"].get(&year_key) {
            for field in fields.as_array().context("Invalid missing-field list.")? {
                let field = field.as_str().context("Invalid missing-field name.")?;
                ensure!(
                    crate::player_dna::schema()["seasonLimits"]
                        .get(field)
                        .is_some(),
                    "Unknown missing-field total."
                );
                ensure!(preserve.insert(field), "Duplicate missing-field total.");
            }
        }
        for (key, value) in totals {
            ensure!(
                crate::player_dna::schema()["seasonLimits"]
                    .get(key)
                    .is_some(),
                "Unsupported imported season total."
            );
            if !preserve.contains(key.as_str()) {
                merged.insert(key.clone(), value.clone());
            }
        }
        let mut merged_row = json!({"year":year,"team":old["team"],"secondaryTeam":old["secondaryTeam"],"totals":merged});
        merged_row["awards"] = source.get("awards").unwrap_or(&old["awards"]).clone();
        let valid = crate::player_dna::seasons(&json!([merged_row]))?;
        let valid = &valid[0];
        let mut changes = Map::new();
        for (key, value) in valid["totals"].as_object().unwrap() {
            if old["totals"][key] != *value {
                changes.insert(key.clone(), value.clone());
            }
        }
        for (key, value) in valid["awards"]
            .as_object()
            .context("Destination awards unavailable.")?
        {
            if old["awards"][key] != *value {
                changes.insert(format!("award:{key}"), value.clone());
            }
        }
        plans.push(json!({"slot":slot,"token":token,"changes":changes}));
    }
    let rows: Vec<_> = destination
        .iter()
        .map(|row| {
            let mut row = row.clone();
            if let Some(object) = row.as_object_mut() {
                object.remove("primaryTeamIndex");
                object.remove("secondaryTeamIndex");
                object.remove("linkedCount");
            }
            row
        })
        .collect();
    Ok(
        json!({"capabilities":capabilities(),"rows":rows,"changed":plans.iter().any(|p| p["changes"].as_object().is_some_and(|c| !c.is_empty())),"plans":plans}),
    )
}
#[cfg(test)]
#[path = "season_import_review_tests.rs"]
mod tests;
