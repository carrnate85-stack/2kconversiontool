//! Validated player-edit requests and bounded, bit-preserving write plans.
//! Pure logic only: no process handle, file chooser, address input or game IO.
use crate::roster_snapshot::{self, PLAYER_STRIDE};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

pub(crate) fn js_space(c: char) -> bool {
    matches!(c,'\u{0009}'..='\u{000d}'|'\u{0020}'|'\u{00a0}'|'\u{1680}'|'\u{2000}'..='\u{200a}'|'\u{2028}'|'\u{2029}'|'\u{202f}'|'\u{205f}'|'\u{3000}'|'\u{feff}')
}
fn numeric(v: &Value) -> Result<f64> {
    if let Some(n) = v.as_f64() {
        return Ok(n);
    }
    let text = v
        .as_str()
        .context("Missing numeric player detail")?
        .trim_matches(js_space);
    ensure!(
        !text.is_empty(),
        "A missing numeric value cannot be treated as zero"
    );
    if let Some(prefix) = text.get(..2) {
        let radix = match prefix {
            "0x" | "0X" => Some(16),
            "0b" | "0B" => Some(2),
            "0o" | "0O" => Some(8),
            _ => None,
        };
        if let Some(radix) = radix {
            return Ok(u64::from_str_radix(&text[2..], radix)? as f64);
        }
    }
    Ok(text.parse()?)
}
fn number(v: &Value, min: f64, max: f64, integer: bool) -> Result<f64> {
    let n = numeric(v)?;
    ensure!(
        n.is_finite() && n >= min && n <= max && (!integer || n.fract() == 0.),
        "Player value is outside its supported range"
    );
    Ok(n)
}
fn int(v: &Value, min: i64, max: i64) -> Result<i64> {
    Ok(number(v, min as f64, max as f64, true)? as i64)
}
fn name(v: &Value) -> Result<String> {
    let s = v
        .as_str()
        .context("Player names need text")?
        .trim_matches(js_space);
    ensure!(
        s.encode_utf16().count() <= 19 && !s.chars().any(|c| c <= '\u{1f}'),
        "Player names are limited to 19 valid characters"
    );
    Ok(s.to_owned())
}
pub(crate) fn date(s: &str) -> Result<(u16, u8, u8)> {
    ensure!(
        s.len() == 10
            && s.as_bytes()[4] == b'-'
            && s.as_bytes()[7] == b'-'
            && s.bytes()
                .enumerate()
                .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit()),
        "Birthday must be YYYY-MM-DD"
    );
    let y: u16 = s[..4].parse()?;
    let m: u8 = s[5..7].parse()?;
    let d: u8 = s[8..].parse()?;
    let days = [
        31,
        if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            29
        } else {
            28
        },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    ensure!(
        (1..=4095).contains(&y) && (1..=12).contains(&m) && d > 0 && d <= days[m as usize - 1],
        "Birthday is not a real supported calendar date"
    );
    Ok((y, m, d))
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FieldEdit {
    pub index: usize,
    pub expected: u32,
    pub value: u32,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Edit {
    pub record_index: usize,
    pub expected_first_name: String,
    pub expected_last_name: String,
    pub expected_cyberface_id: u16,
    pub first_name: String,
    pub last_name: String,
    pub cyberface_id: u16,
    pub position_id: u8,
    pub secondary_position_id: u8,
    pub jersey_number_raw: u8,
    pub weight_lbs: f64,
    pub height_cm: f64,
    pub wingspan_cm: f64,
    pub potential: u8,
    pub rating_edits: Vec<FieldEdit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_index: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_name_id: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_birth_year: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_birth_month: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_birth_day: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub years_pro: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_years_pro: Option<u8>,
}
pub(crate) fn validate(v: &Value) -> Result<Edit> {
    ensure!(v.is_object(), "Expected a player-edit object");
    let first = name(&v["firstName"])?;
    let last = name(&v["lastName"])?;
    ensure!(
        !first.is_empty() || !last.is_empty(),
        "A player needs a first or last name"
    );
    let mut edits = Vec::new();
    let mut seen = HashSet::new();
    if let Some(changes) = v.get("ratingEdits") {
        let changes = changes.as_array().context("Invalid player field changes")?;
        ensure!(
            changes.len() <= roster_snapshot::rating_fields().len(),
            "Too many player field changes"
        );
        for change in changes {
            // Unlike the basic form's numeric inputs, rating values are already
            // typed Numbers in WPF. Strings, nulls and bools are not coerced.
            let integral = |key: &str, max: f64| -> Result<u64> {
                let n = change[key]
                    .as_f64()
                    .context("Player fields require numeric integers")?;
                ensure!(
                    n.is_finite() && n.fract() == 0. && n >= 0. && n <= max,
                    "Player field is out of range"
                );
                Ok(n as u64)
            };
            let index = integral("index", 454.)? as usize;
            ensure!(seen.insert(index), "Duplicate player field change");
            let field = &roster_snapshot::rating_fields()[index];
            ensure!(field.maximum >= 0, "This player field is read-only");
            let value = integral("value", field.maximum as f64)? as u32;
            let expected = integral("expected", ((1u64 << field.bits) - 1) as f64)? as u32;
            edits.push(FieldEdit {
                index,
                expected,
                value,
            });
        }
    }
    let birth = v
        .get("birthDate")
        .map(|v| {
            let s = v.as_str().context("Birthday needs text")?;
            date(s)?;
            Ok::<_, anyhow::Error>(s.to_owned())
        })
        .transpose()?;
    Ok(Edit {
        record_index: int(&v["recordIndex"], 0, 8191)? as usize,
        expected_first_name: name(&v["expectedFirstName"])?,
        expected_last_name: name(&v["expectedLastName"])?,
        expected_cyberface_id: int(&v["expectedCyberfaceId"], 0, 65535)? as u16,
        first_name: first,
        last_name: last,
        cyberface_id: int(&v["cyberfaceId"], 0, 65535)? as u16,
        position_id: int(&v["positionId"], 0, 5)? as u8,
        secondary_position_id: int(&v["secondaryPositionId"], 0, 5)? as u8,
        jersey_number_raw: int(&v["jerseyNumberRaw"], 0, 100)? as u8,
        weight_lbs: number(&v["weightLbs"], 70., 500., false)?,
        height_cm: number(&v["heightCm"], 120., 260., false)?,
        wingspan_cm: number(&v["wingspanCm"], 120., 320., false)?,
        potential: int(&v["potential"], 25, 110)? as u8,
        rating_edits: edits,
        from_index: v
            .get("fromIndex")
            .map(|v| int(v, 0, 4095).map(|n| n as u16))
            .transpose()?,
        full_name_id: v
            .get("fullNameId")
            .map(|v| int(v, -1, 32767).map(|n| n as i16))
            .transpose()?,
        expected_birth_year: if birth.is_some() {
            Some(int(&v["expectedBirthYear"], 0, 4095)? as u16)
        } else {
            None
        },
        expected_birth_month: if birth.is_some() {
            Some(int(&v["expectedBirthMonth"], 0, 15)? as u8)
        } else {
            None
        },
        expected_birth_day: if birth.is_some() {
            Some(int(&v["expectedBirthDay"], 0, 31)? as u8)
        } else {
            None
        },
        birth_date: birth,
        years_pro: v
            .get("yearsPro")
            .map(|v| int(v, 0, 31).map(|n| n as u8))
            .transpose()?,
        expected_years_pro: if v.get("yearsPro").is_some() {
            Some(int(&v["expectedYearsPro"], 0, 31)? as u8)
        } else {
            None
        },
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Area {
    Record,
    Appearance,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Patch {
    pub area: Area,
    pub offset: usize,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub mask: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RawPlayer {
    pub record: Vec<u8>,
    pub appearance: Vec<u8>,
}
impl RawPlayer {
    pub fn bytes(&self, area: Area) -> &[u8] {
        match area {
            Area::Record => &self.record,
            Area::Appearance => &self.appearance,
        }
    }
    pub fn bytes_mut(&mut self, area: Area) -> &mut [u8] {
        match area {
            Area::Record => &mut self.record,
            Area::Appearance => &mut self.appearance,
        }
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.record.len() == PLAYER_STRIDE && self.appearance.len() == 4,
            "Incomplete player buffers"
        );
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    pub record_index: usize,
    pub before: RawPlayer,
    pub after: RawPlayer,
    pub patches: Vec<Patch>,
    pub guards: Vec<(Area, usize, usize)>,
    pub changed_fields: Vec<String>,
}
impl Plan {
    pub fn expected(&self, applied: usize) -> Result<RawPlayer> {
        ensure!(
            applied <= self.patches.len(),
            "Invalid player transaction step"
        );
        let mut raw = self.before.clone();
        for p in self.patches.iter().take(applied) {
            raw.bytes_mut(p.area)[p.offset..p.offset + p.after.len()].copy_from_slice(&p.after);
        }
        Ok(raw)
    }
    pub fn matches(&self, raw: &RawPlayer, expected: &RawPlayer) -> Result<bool> {
        raw.validate()?;
        expected.validate()?;
        Ok(self
            .guards
            .iter()
            .all(|(a, o, n)| raw.bytes(*a)[*o..*o + *n] == expected.bytes(*a)[*o..*o + *n]))
    }
    pub fn view(&self) -> Value {
        json!({"fields":self.changed_fields,"patches":self.patches.len(),"bytes":self.patches.iter().map(|p|p.after.len()).sum::<usize>(),"gameWrites":false})
    }
}
pub(crate) struct Origin {
    pub index: u16,
    pub name: String,
    pub pointer: u64,
    pub selectable: bool,
}
pub(crate) struct Extras<'a> {
    pub origins: Option<&'a [Origin]>,
    pub audio_verified: bool,
}
impl Default for Extras<'_> {
    fn default() -> Self {
        Self {
            origins: None,
            audio_verified: false,
        }
    }
}
fn unsigned(bytes: &[u8], offset: usize, len: usize) -> Result<u64> {
    ensure!(len <= 8, "Invalid player integer width");
    let slice = bytes
        .get(offset..offset.checked_add(len).context("Offset overflow")?)
        .context("Incomplete player field")?;
    let mut b = [0; 8];
    b[..len].copy_from_slice(slice);
    Ok(u64::from_le_bytes(b))
}
fn packed(bytes: &[u8], offset: usize, shift: u32, bits: u32) -> Result<u64> {
    ensure!(
        bits > 0 && bits <= 32 && shift + bits <= 32,
        "Invalid player packed field"
    );
    Ok((unsigned(bytes, offset, 4)? >> shift) & ((1u64 << bits) - 1))
}
fn decoded_name(bytes: &[u8], offset: usize) -> Result<String> {
    let s = bytes
        .get(offset..offset + 40)
        .context("Incomplete player name")?;
    let words: Vec<_> = s
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|n| *n != 0)
        .collect();
    Ok(String::from_utf16_lossy(&words).trim().to_owned())
}
fn fixed_name(s: &str) -> Vec<u8> {
    let mut result = vec![0; 40];
    for (i, w) in s.encode_utf16().enumerate() {
        result[i * 2..i * 2 + 2].copy_from_slice(&w.to_le_bytes());
    }
    result
}
fn cached_int(cached: &Value, key: &str, max: i64) -> Result<u64> {
    let n = cached[key]
        .as_f64()
        .context("The original player detail is unavailable")?;
    ensure!(
        n.fract() == 0. && n >= 0. && n <= max as f64,
        "The original player detail is invalid"
    );
    Ok(n as u64)
}
struct Builder {
    record_index: usize,
    before: RawPlayer,
    after: RawPlayer,
    masks: BTreeMap<(Area, usize), u8>,
    fields: Vec<String>,
    spans: Vec<(Area, usize, usize)>,
}
impl Builder {
    fn set(
        &mut self,
        area: Area,
        offset: usize,
        bytes: &[u8],
        mask: &[u8],
        field: &str,
    ) -> Result<()> {
        ensure!(
            bytes.len() == mask.len()
                && offset
                    .checked_add(bytes.len())
                    .is_some_and(|end| end <= self.before.bytes(area).len()),
            "Invalid player patch span"
        );
        let mut changed = false;
        for (i, (&value, &bits)) in bytes.iter().zip(mask).enumerate() {
            let at = offset + i;
            let old = self.before.bytes(area)[at];
            let key = (area, at);
            let used = self.masks.get(&key).copied().unwrap_or(0);
            let current = self.after.bytes(area)[at];
            ensure!(
                ((current ^ value) & used & bits) == 0,
                "Conflicting player field masks"
            );
            if ((old ^ value) & bits) == 0 {
                continue;
            }
            self.after.bytes_mut(area)[at] = (current & !bits) | (value & bits);
            self.masks.insert(key, used | bits);
            changed = true;
        }
        if changed {
            let first = mask
                .iter()
                .position(|b| *b != 0)
                .context("Empty player field mask")?;
            let last = mask.iter().rposition(|b| *b != 0).unwrap();
            self.spans.push((area, offset + first, last - first + 1));
            if !self.fields.iter().any(|s| s == field) {
                self.fields.push(field.to_owned());
            }
        }
        Ok(())
    }
    fn full(&mut self, area: Area, offset: usize, bytes: &[u8], field: &str) -> Result<()> {
        self.set(area, offset, bytes, &vec![255; bytes.len()], field)
    }
    fn bits(
        &mut self,
        offset: usize,
        shift: u32,
        width: u32,
        value: u64,
        field: &str,
    ) -> Result<()> {
        let mask = (((1u64 << width) - 1) << shift) as u32;
        let after = (value << shift) as u32;
        self.set(
            Area::Record,
            offset,
            &after.to_le_bytes(),
            &mask.to_le_bytes(),
            field,
        )
    }
    fn finish(mut self) -> Result<Plan> {
        // Keep UTF-16 names and other logical field spans together, including
        // unchanged bytes inside them. Never split a surrogate/name into many
        // byte writes just because its high bytes happen to stay unchanged.
        self.spans.sort();
        let mut spans: Vec<(Area, usize, usize)> = Vec::new();
        for (area, start, len) in self.spans {
            if let Some((a, s, n)) = spans.last_mut() {
                if *a == area && start <= *s + *n {
                    let total = (start + len).max(*s + *n) - *s;
                    if total <= 40 {
                        *n = total;
                        continue;
                    }
                    ensure!(
                        start == *s + *n,
                        "Overlapping player patch exceeds its bounded span"
                    );
                }
            }
            spans.push((area, start, len));
        }
        let patches: Vec<_> = spans
            .into_iter()
            .map(|(area, offset, len)| Patch {
                area,
                offset,
                before: self.before.bytes(area)[offset..offset + len].to_vec(),
                after: self.after.bytes(area)[offset..offset + len].to_vec(),
                mask: (offset..offset + len)
                    .map(|at| self.masks.get(&(area, at)).copied().unwrap_or(0))
                    .collect(),
            })
            .collect();
        let mut guards = vec![
            (Area::Record, 0, 80),
            (Area::Record, 0x60, 8),
            (Area::Record, 0x78, 8),
            (Area::Record, 0x128, 2),
            (Area::Record, 0x108, 4),
            (Area::Record, 0x110, 2),
            (Area::Record, 0x115, 2),
            (Area::Record, 0x12D, 1),
            (Area::Record, 0x45D, 1),
            (Area::Appearance, 0, 4),
        ];
        guards.extend(patches.iter().map(|p| (p.area, p.offset, p.after.len())));
        Ok(Plan {
            record_index: self.record_index,
            before: self.before,
            after: self.after,
            patches,
            guards,
            changed_fields: self.fields,
        })
    }
}
pub(crate) fn plan(
    edit: &Edit,
    cached: &Value,
    raw: &RawPlayer,
    extras: Extras<'_>,
) -> Result<Plan> {
    let validated = validate(&serde_json::to_value(edit)?)?;
    let edit = &validated;
    raw.validate()?;
    ensure!(
        cached["source"] == "live-roster"
            && cached["recordIndex"].as_u64() == Some(edit.record_index as u64)
            && cached["firstName"] == edit.expected_first_name
            && cached["lastName"] == edit.expected_last_name
            && cached["cyberfaceId"].as_u64() == Some(edit.expected_cyberface_id as u64),
        "The selected player no longer matches this editor snapshot"
    );
    ensure!(
        decoded_name(&raw.record, 40)? == edit.expected_first_name
            && decoded_name(&raw.record, 0)? == edit.expected_last_name
            && unsigned(&raw.record, 0x110, 2)? == edit.expected_cyberface_id as u64,
        "The live player identity changed before Save"
    );
    for (key, offset, shift, width) in [
        ("positionId", 0x12D, 0, 3),
        ("secondaryPositionId", 0x12D, 3, 3),
        ("jerseyNumberRaw", 0x115, 3, 7),
        ("potentialRaw", 0x45D, 0, 8),
    ] {
        ensure!(
            packed(&raw.record, offset, shift, width)?
                == cached_int(cached, key, (1i64 << width) - 1)?,
            "The live {key} changed before Save"
        );
    }
    let old_height = cached_int(cached, "heightRaw", 26000)?;
    let old_wings = cached_int(cached, "wingspanRaw", 32000)?;
    ensure!(
        old_height >= 12000
            && old_wings >= 12000
            && unsigned(&raw.appearance, 0, 2)? == old_height
            && unsigned(&raw.appearance, 2, 2)? == old_wings,
        "The live measurements changed before Save"
    );
    let weight = f32::from_le_bytes(raw.record[0x108..0x10C].try_into()?);
    let weight_display = (weight as f64 * 100.).round() / 100.;
    ensure!(
        weight.is_finite()
            && (70.0..=500.0).contains(&weight)
            && number(&cached["weightLbs"], 70., 500., false)? == weight_display,
        "The live weight changed before Save"
    );
    let mut b = Builder {
        record_index: edit.record_index,
        before: raw.clone(),
        after: raw.clone(),
        masks: BTreeMap::new(),
        fields: vec![],
        spans: vec![],
    };
    if edit.first_name != edit.expected_first_name {
        b.full(Area::Record, 40, &fixed_name(&edit.first_name), "firstName")?;
    }
    if edit.last_name != edit.expected_last_name {
        b.full(Area::Record, 0, &fixed_name(&edit.last_name), "lastName")?;
    }
    b.full(
        Area::Record,
        0x110,
        &edit.cyberface_id.to_le_bytes(),
        "cyberfaceId",
    )?;
    b.bits(0x12D, 0, 3, edit.position_id as u64, "positionId")?;
    b.bits(
        0x12D,
        3,
        3,
        edit.secondary_position_id as u64,
        "secondaryPositionId",
    )?;
    b.bits(
        0x115,
        3,
        7,
        edit.jersey_number_raw as u64,
        "jerseyNumberRaw",
    )?;
    // Retain the exact original float when the unchanged UI display was rounded.
    if edit.weight_lbs != weight_display {
        b.full(
            Area::Record,
            0x108,
            &(edit.weight_lbs as f32).to_le_bytes(),
            "weightLbs",
        )?;
    }
    b.full(
        Area::Appearance,
        0,
        &((edit.height_cm * 100.).round() as u16).to_le_bytes(),
        "heightCm",
    )?;
    b.full(
        Area::Appearance,
        2,
        &((edit.wingspan_cm * 100.).round() as u16).to_le_bytes(),
        "wingspanCm",
    )?;
    let potential = if edit.potential as u64 == cached_int(cached, "potential", 110)? {
        cached_int(cached, "potentialRaw", 255)?
    } else if edit.potential <= 25 {
        0
    } else {
        255u64.min((edit.potential as u64 - 25) * 3 - 1)
    };
    b.bits(0x45D, 0, 8, potential, "potential")?;
    if !edit.rating_edits.is_empty() {
        ensure!(
            cached["ratingsRaw"]
                .as_array()
                .is_some_and(|a| a.len() == 455),
            "The original rating layout is unavailable"
        );
    }
    for change in &edit.rating_edits {
        ensure!(
            change.expected <= i32::MAX as u32,
            "Original player field exceeds the verified setter's signed input range"
        );
        let f = &roster_snapshot::rating_fields()[change.index];
        ensure!(
            cached["ratingsRaw"][change.index].as_u64() == Some(change.expected as u64)
                && packed(&raw.record, f.offset, f.shift, f.bits)? == change.expected as u64,
            "A selected player field changed before Save"
        );
        b.bits(f.offset, f.shift, f.bits, change.value as u64, &f.key)?;
    }
    if let Some(birthday) = &edit.birth_date {
        ensure!(
            cached["bioLayoutVerified"] == true,
            "The birthday layout is unavailable"
        );
        let (year, month, day) = date(birthday)?;
        for (key, offset, shift, width, expected) in [
            (
                "birthYear",
                0x112,
                0,
                12,
                edit.expected_birth_year.map(|v| v as u64),
            ),
            (
                "birthMonth",
                0x36B,
                4,
                4,
                edit.expected_birth_month.map(|v| v as u64),
            ),
            (
                "birthDay",
                0x114,
                1,
                5,
                edit.expected_birth_day.map(|v| v as u64),
            ),
        ] {
            let expected = expected.context("Missing original birthday")?;
            ensure!(
                cached[key].as_u64() == Some(expected)
                    && packed(&raw.record, offset, shift, width)? == expected,
                "The original birthday changed before Save"
            );
        }
        b.bits(0x112, 0, 12, year as u64, "birthDate")?;
        b.bits(0x36B, 4, 4, month as u64, "birthDate")?;
        b.bits(0x114, 1, 5, day as u64, "birthDate")?;
    }
    if let Some(years) = edit.years_pro {
        let expected = edit
            .expected_years_pro
            .context("Missing original years pro")?;
        ensure!(
            cached["bioLayoutVerified"] == true
                && cached["yearsPro"].as_u64() == Some(expected as u64)
                && packed(&raw.record, 0x3E2, 1, 5)? == expected as u64,
            "The original years pro changed before Save"
        );
        b.bits(0x3E2, 1, 5, years as u64, "yearsPro")?;
    }
    if let Some(audio) = edit.full_name_id {
        let old = cached["audio"]["fullNameId"]
            .as_i64()
            .context("The original audio mapping is unavailable")?;
        if audio as i64 != old {
            ensure!(
                extras.audio_verified
                    && i16::from_le_bytes(raw.record[0x2E8..0x2EA].try_into()?) as i64 == old,
                "The audio owner or full-name ID changed"
            );
            b.full(Area::Record, 0x2E8, &audio.to_le_bytes(), "fullNameId")?;
        }
    }
    if let Some(from) = edit.from_index {
        let old = cached["fromIndex"]
            .as_i64()
            .filter(|v| *v >= -1)
            .context("The original From mapping is unavailable")?;
        if from as i64 != old {
            let directory = extras
                .origins
                .context("The verified From directory is unavailable")?;
            let candidates: Vec<_> = directory
                .iter()
                .filter(|o| o.index == from && o.selectable && !o.name.is_empty())
                .collect();
            ensure!(
                candidates.len() == 1,
                "The selected From entry is unavailable or ambiguous"
            );
            let current = unsigned(&raw.record, 0xB0, 8)?;
            let old_matches = if old == -1 {
                current == 0
            } else {
                directory
                    .iter()
                    .filter(|o| o.index as i64 == old && o.pointer == current)
                    .count()
                    == 1
            };
            ensure!(old_matches, "The player's original From entry changed");
            let pointer = roster_snapshot::checked_pointer(candidates[0].pointer)?;
            b.full(Area::Record, 0xB0, &pointer.to_le_bytes(), "fromIndex")?;
        }
    }
    b.finish()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn fixture() -> Value {
        serde_json::from_str(include_str!("../assets/player-write-regressions.json")).unwrap()
    }
    pub fn bytes(hex: &Value) -> Vec<u8> {
        let s = hex.as_str().unwrap();
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }
    pub fn raw(v: &Value) -> RawPlayer {
        RawPlayer {
            record: bytes(&v["before"]),
            appearance: bytes(&v["appearance"]),
        }
    }
    fn same(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
            (Value::Array(a), Value::Array(b)) => {
                a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same(a, b))
            }
            (Value::Object(a), Value::Object(b)) => {
                a.len() == b.len() && a.iter().all(|(k, a)| b.get(k).is_some_and(|b| same(a, b)))
            }
            _ => a == b,
        }
    }
    #[test]
    fn request_validation_matches_original_wpf() {
        let cases: Vec<Value> = serde_json::from_str(include_str!(
            "../assets/player-write-validation-regressions.json"
        ))
        .unwrap();
        assert_eq!(cases.len(), 345);
        for (i, case) in cases.iter().enumerate() {
            let result = validate(&case["input"]);
            assert_eq!(
                result.is_ok(),
                case["ok"] == true,
                "Request case {i}: {result:?}"
            );
            if let Ok(v) = result {
                assert!(
                    same(&serde_json::to_value(v).unwrap(), &case["expected"]),
                    "Normalized request differs in case {i}"
                );
            }
        }
    }
    #[test]
    fn basic_plans_match_real_unchanged_helper_writes_to_its_own_process() {
        let f = fixture();
        for case in f["basic"].as_array().unwrap() {
            assert_eq!(case["report"]["readBackVerified"], true);
            let original = raw(case);
            let p = plan(
                &validate(&case["input"]).unwrap(),
                &case["cached"],
                &original,
                Extras::default(),
            )
            .unwrap();
            let mut expected = bytes(&case["after"]);
            if case["preserveUnchangedFloat"] == true {
                assert_ne!(
                    &expected[0x108..0x10C],
                    &original.record[0x108..0x10C],
                    "Original helper did not reproduce weight display quantization"
                );
                expected[0x108..0x10C].copy_from_slice(&original.record[0x108..0x10C]);
                assert!(p.patches.is_empty());
            }
            assert_eq!(p.after.record, expected);
            assert_eq!(p.after.appearance, bytes(&case["afterAppearance"]));
            assert_eq!(p.expected(p.patches.len()).unwrap(), p.after);
            for patch in &p.patches {
                assert!(patch.after.len() <= 40);
                for i in 0..patch.after.len() {
                    assert_eq!((patch.before[i] ^ patch.after[i]) & !patch.mask[i], 0);
                }
            }
        }
    }
    #[test]
    fn every_rating_field_patch_matches_original_cpp_layout_helper() {
        let f = fixture();
        let original = raw(&f);
        assert_eq!(f["fields"].as_array().unwrap().len(), 910);
        for case in f["fields"].as_array().unwrap() {
            let c: Vec<_> = case["changes"]
                .as_str()
                .unwrap()
                .split(':')
                .map(|s| s.parse::<u64>().unwrap())
                .collect();
            let mut input = f["input"].clone();
            input["ratingEdits"] = json!([{"index":c[0],"expected":c[1],"value":c[2]}]);
            let result =
                validate(&input).and_then(|e| plan(&e, &f["cached"], &original, Extras::default()));
            assert_eq!(
                result.is_ok(),
                case["ok"] == true,
                "Field case {}",
                case["changes"]
            );
            if let Ok(p) = result {
                assert_eq!(
                    p.after.record,
                    bytes(&case["after"]),
                    "Field {}",
                    case["changes"]
                );
                assert_eq!(p.after.appearance, original.appearance);
            }
        }
    }
    #[test]
    fn birthday_years_pro_patches_match_original_pure_planner() {
        let f = fixture();
        for case in f["bio"].as_array().unwrap() {
            let mut input = f["input"].clone();
            input["birthDate"] = case["birthday"].clone();
            input["yearsPro"] = case["years"].clone();
            input["expectedBirthYear"] = json!(1988);
            input["expectedBirthMonth"] = json!(3);
            input["expectedBirthDay"] = json!(14);
            input["expectedYearsPro"] = json!(17);
            let result =
                validate(&input).and_then(|e| plan(&e, &f["cached"], &raw(&f), Extras::default()));
            assert_eq!(result.is_ok(), case["ok"] == true);
            if let Ok(p) = result {
                assert_eq!(p.after.record, bytes(&case["after"]));
            }
        }
    }
    #[test]
    fn disjoint_peak_age_and_jersey_bits_merge_names_are_whole_and_stale_values_fail() {
        let f = fixture();
        let original = raw(&f);
        let index = roster_snapshot::rating_fields()
            .iter()
            .position(|field| field.key == "PeakAge")
            .unwrap();
        let field = &roster_snapshot::rating_fields()[index];
        let expected = f["cached"]["ratingsRaw"][index].as_u64().unwrap();
        let value = if expected == 63 { 0 } else { 63 };
        let mut input = f["input"].clone();
        input["jerseyNumberRaw"] = json!(100);
        input["firstName"] = json!("🏀 Player");
        input["ratingEdits"] = json!([{"index":index,"expected":expected,"value":value}]);
        let p = plan(
            &validate(&input).unwrap(),
            &f["cached"],
            &original,
            Extras::default(),
        )
        .unwrap();
        assert_eq!(packed(&p.after.record, 0x115, 3, 7).unwrap(), 100);
        assert_eq!(
            packed(&p.after.record, field.offset, field.shift, field.bits).unwrap(),
            value
        );
        assert!(
            p.patches
                .iter()
                .any(|p| p.area == Area::Record && p.offset == 40 && p.after.len() == 40)
        );
        let mut stale = original.clone();
        stale.record[0x110] ^= 1;
        assert!(
            plan(
                &validate(&input).unwrap(),
                &f["cached"],
                &stale,
                Extras::default()
            )
            .is_err()
        );
        let mut stale = f["cached"].clone();
        stale["yearsPro"] = json!(18);
        input["yearsPro"] = json!(0);
        input["expectedYearsPro"] = json!(17);
        assert!(
            plan(
                &validate(&input).unwrap(),
                &stale,
                &original,
                Extras::default()
            )
            .is_err()
        );
    }
    #[test]
    fn from_audio_and_request_shapes_require_native_verification() {
        let f = fixture();
        let mut input = f["input"].clone();
        input["fullNameId"] = json!(123);
        let edit = validate(&input).unwrap();
        assert!(plan(&edit, &f["cached"], &raw(&f), Extras::default()).is_err());
        let p = plan(
            &edit,
            &f["cached"],
            &raw(&f),
            Extras {
                audio_verified: true,
                origins: None,
            },
        )
        .unwrap();
        assert_eq!(
            i16::from_le_bytes(p.after.record[0x2E8..0x2EA].try_into().unwrap()),
            123
        );
        input = f["input"].clone();
        input["fromIndex"] = json!(1);
        let edit = validate(&input).unwrap();
        assert!(plan(&edit, &f["cached"], &raw(&f), Extras::default()).is_err());
        let entries = [Origin {
            index: 1,
            name: "Duke".into(),
            pointer: 0x50048,
            selectable: true,
        }];
        let p = plan(
            &edit,
            &f["cached"],
            &raw(&f),
            Extras {
                audio_verified: false,
                origins: Some(&entries),
            },
        )
        .unwrap();
        assert_eq!(unsigned(&p.after.record, 0xB0, 8).unwrap(), 0x50048);
        let mut invalid = edit;
        invalid.rating_edits.push(FieldEdit {
            index: usize::MAX,
            expected: 0,
            value: 1,
        });
        assert!(plan(&invalid, &f["cached"], &raw(&f), Extras::default()).is_err());
    }
}
