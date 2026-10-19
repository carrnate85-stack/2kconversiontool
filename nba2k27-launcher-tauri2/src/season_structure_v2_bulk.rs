//! Bounded native-derived multi-season plan. The operation adapter must supply
//! retained team membership, source review identities and actual owned freeze.
//! Prefix equivalence is never replay/undo authority or native quiescence.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Choice {
    pub year: u16,
    pub primary_team_index: usize,
    pub secondary_team_index: Option<usize>,
    /// Only selected scalar/award values; omissions preserve an existing row or
    /// the canonical native free-row base for a newly added season.
    pub changes: Value,
}
fn fields_valid(changes: &Value) -> bool {
    changes.as_object().is_some_and(|m| {
        m.len() <= 31
            && m.keys().all(|k| match k.strip_prefix("award:") {
                Some(a) => crate::roster_v2_services::AWARDS.iter().any(|f| f.0 == a),
                None => crate::roster_v2_services::SEASON_FIELDS
                    .iter()
                    .any(|f| f.0 == k),
            })
    })
}
fn team(snapshot: &Snapshot, index: usize) -> Result<u64, Refusal> {
    if index >= snapshot.teams.count {
        return Err(Refusal::TeamPointer);
    }
    snapshot
        .teams
        .base
        .checked_add(index as u64 * 0x1550)
        .ok_or(Refusal::TeamPointer)
}
fn encoded(
    source: &[u8; STAT_BYTES],
    choice: &Choice,
    snapshot: &Snapshot,
) -> Result<[u8; STAT_BYTES], Refusal> {
    if !fields_valid(&choice.changes) {
        return Err(Refusal::Changes);
    }
    let (values, _) = crate::roster_v2_services::plan_season_record(source, &choice.changes)
        .map_err(|_| Refusal::Changes)?;
    crate::roster_v2_services::validate_season_patch(source, &values)
        .map_err(|_| Refusal::Changes)?;
    let mut row: [u8; STAT_BYTES] = values.try_into().map_err(|_| Refusal::Shape)?;
    row[..8].copy_from_slice(&team(snapshot, choice.primary_team_index)?.to_le_bytes());
    row[8..16].copy_from_slice(
        &choice
            .secondary_team_index
            .map(|i| team(snapshot, i))
            .transpose()?
            .unwrap_or(0)
            .to_le_bytes(),
    );
    row[17] = (choice.year - 1900) as u8;
    Ok(row)
}
fn copy_bits_valid(row: &[u8; STAT_BYTES], destination: &[u8; STAT_BYTES]) -> Result<(), Refusal> {
    let mut masks = crate::roster_v2_services::season_masks();
    masks[..16].fill(255);
    masks[17..20].fill(255);
    for (i, mask) in REF_MASK.to_le_bytes().iter().enumerate() {
        masks[60 + i] |= *mask;
    }
    for i in 0..STAT_BYTES {
        if (row[i] ^ destination[i]) & !masks[i] != 0 {
            return Err(Refusal::UnprovedCopyBit(i));
        }
    }
    Ok(())
}
/// No existing year is evicted. New/COW rows reserve one exact native free-chain
/// prefix, then become initialized before the single directory publication.
/// Source counts decrease only after that publication; reverse order reconnects
/// original pool head last. Native transaction must keep the game frozen.
pub(crate) fn plan(
    snapshot: &Snapshot,
    player: usize,
    choices: &[Choice],
) -> Result<Outcome, Refusal> {
    let valid = snapshot.validate()?;
    let player_row = snapshot.players.get(player).ok_or(Refusal::Player)?;
    if choices.is_empty() || choices.len() > REGULAR_SLOTS {
        return Err(Refusal::Changes);
    }
    let mut existing = BTreeMap::new();
    let mut origin = None;
    for s in 0..REGULAR_SLOTS {
        let r = slot(player_row, s);
        if r == EMPTY {
            continue;
        }
        let year = 1900usize + snapshot.rows[r as usize][17] as usize;
        if origin.is_some_and(|o| o != year + s) || existing.insert(year, r as usize).is_some() {
            return Err(Refusal::Calendar);
        }
        origin = Some(year + s);
    }
    let mut imported = BTreeMap::new();
    for choice in choices {
        if !(1900..=2155).contains(&choice.year)
            || !fields_valid(&choice.changes)
            || imported.insert(choice.year as usize, choice).is_some()
        {
            return Err(Refusal::Changes);
        }
        team(snapshot, choice.primary_team_index)?;
        if let Some(i) = choice.secondary_team_index {
            team(snapshot, i)?;
        }
        origin = Some(origin.unwrap_or(0).max(choice.year as usize));
    }
    let origin = origin.ok_or(Refusal::Calendar)?;
    if existing
        .keys()
        .chain(imported.keys())
        .any(|y| origin - y >= REGULAR_SLOTS)
    {
        return Err(Refusal::FullHistory);
    }
    let mut directory = [255u8; REGULAR_SLOTS * 2];
    for (&year, &row) in &existing {
        put_word(&mut directory, (origin - year) * 2, row as u16);
    }
    let mut allocated = 0;
    let mut initializations = Vec::new();
    let mut updates = Vec::new();
    let mut decrements = Vec::new();
    // Canonical descending-year order makes the same choice set deterministic.
    for (&year, choice) in imported.iter().rev() {
        let old = existing.get(&year).copied();
        let mut row = encoded(
            old.map(|i| &snapshot.rows[i]).unwrap_or(&[0; STAT_BYTES]),
            choice,
            snapshot,
        )?;
        let changed = old.is_none_or(|i| row != snapshot.rows[i]);
        let index = if !changed {
            old.unwrap()
        } else if old.is_none_or(|i| valid.references[i].len() > 1) {
            let &index = valid.free.get(allocated).ok_or(Refusal::NoFreeRow)?;
            allocated += 1;
            if old.is_none() {
                row = encoded(&snapshot.rows[index], choice, snapshot)?;
            }
            put_word(&mut row, 18, EMPTY);
            set_refcount(&mut row, 1);
            copy_bits_valid(&row, &snapshot.rows[index])?;
            initializations.push((index, row));
            if let Some(i) = old {
                let mut after = snapshot.rows[i];
                set_refcount(&mut after, valid.references[i].len() - 1);
                decrements.push((i, after[60..64].to_vec()));
            }
            index
        } else {
            let index = old.unwrap();
            updates.push((index, row));
            index
        };
        put_word(&mut directory, (origin - year) * 2, index as u16);
    }
    let mut out = Plan::new(snapshot);
    if allocated > 0 {
        let next = valid
            .free
            .get(allocated)
            .map(|&i| i as u16)
            .unwrap_or(EMPTY);
        out.push(Phase::ReserveHead, Target::Manager, 0, &next.to_le_bytes());
    }
    for (index, row) in initializations {
        out.push(Phase::InitializeRow, Target::Row(index), 0, &row);
    }
    for (index, row) in updates {
        out.push(Phase::UpdateRow, Target::Row(index), 0, &row);
    }
    if directory.as_slice() != &player_row[0x130..0x130 + REGULAR_SLOTS * 2] {
        out.push(
            Phase::PublishDirectory,
            Target::Player(player),
            0x130,
            &directory,
        );
    }
    for (index, bytes) in decrements {
        out.push(Phase::DecrementSourceCount, Target::Row(index), 60, &bytes);
    }
    out.after.validate()?;
    if out.after == out.before {
        Ok(Outcome::Unchanged)
    } else {
        Ok(Outcome::Planned(out))
    }
}
#[cfg(test)]
#[path = "season_structure_v2_bulk_tests.rs"]
mod tests;
