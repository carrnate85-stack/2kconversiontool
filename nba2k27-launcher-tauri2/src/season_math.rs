//! WPF-equivalent local season editing; no packed game memory or process ownership.
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
const AVERAGES: &[(&str, &str)] = &[
    ("ppg", "points"),
    ("mpg", "minutes"),
    ("orpg", "oreb"),
    ("apg", "assists"),
    ("spg", "steals"),
    ("bpg", "blocks"),
    ("topg", "turnovers"),
    ("fpg", "fouls"),
    ("plusMinusAverage", "plusMinus"),
];
const PERCENTAGES: &[(&str, &str, &str)] = &[
    ("fgPercent", "fgm", "fga"),
    ("threePercent", "threeMade", "threeAttempted"),
    ("ftPercent", "ftm", "fta"),
];
fn validate(t: &Value) -> Result<()> {
    let limits = crate::player_dna::schema()["seasonLimits"]
        .as_object()
        .unwrap();
    let obj = t.as_object().context("Season totals required")?;
    ensure!(
        obj.len() == limits.len() && obj.keys().all(|k| limits.contains_key(k)),
        "Complete supported season totals required"
    );
    for (k, max) in limits {
        let n = t[k].as_f64().context("Totals must be whole numbers")?;
        ensure!(
            n.is_finite()
                && n.fract() == 0.
                && n >= if k == "plusMinus" { -16384. } else { 0. }
                && n <= max.as_f64().unwrap(),
            "Outside the stored range: {k}"
        );
    }
    Ok(())
}
fn n(t: &Value, k: &str) -> f64 {
    t[k].as_f64().unwrap()
}
fn put(t: &mut Value, k: &str, v: f64) {
    t[k] = json!(v as i64);
}
fn round(v: f64) -> f64 {
    (v + 0.5).floor()
} // Math.round ties toward +infinity, including negative plus/minus.
fn scoring(t: &Value) -> f64 {
    2. * n(t, "fgm") + n(t, "threeMade") + n(t, "ftm")
}
pub fn display(t: &Value) -> Result<Value> {
    validate(t)?;
    let mut out = t.clone();
    let gp = n(t, "gp");
    put(&mut out, "rebounds", n(t, "oreb") + n(t, "dreb"));
    for &(key, total) in AVERAGES {
        out[key] = if gp > 0. {
            json!(n(t, total) / gp)
        } else {
            Value::Null
        };
    }
    out["rpg"] = if gp > 0. {
        json!((n(t, "oreb") + n(t, "dreb")) / gp)
    } else {
        Value::Null
    };
    for &(key, made, attempted) in PERCENTAGES {
        out[key] = if n(t, attempted) > 0. {
            json!(100. * n(t, made) / n(t, attempted))
        } else {
            Value::Null
        };
    }
    Ok(out)
}
fn score_to(t: &Value, target: f64) -> Result<Value> {
    let two_attempts = n(t, "fga") - n(t, "threeAttempted");
    let three_attempts = n(t, "threeAttempted");
    let free_attempts = n(t, "fta");
    ensure!(
        two_attempts >= 0.,
        "Three-point attempts cannot exceed field-goal attempts."
    );
    let maximum = 2. * two_attempts + 3. * three_attempts + free_attempts;
    let target = round(target).clamp(0., maximum.min(16383.));
    let old = scoring(t);
    let factor = if old != 0. {
        target / old
    } else if maximum != 0. {
        target / maximum
    } else {
        0.
    };
    let ideal_two = if old != 0. {
        n(t, "fgm") - n(t, "threeMade")
    } else {
        two_attempts
    } * factor;
    let ideal_three = if old != 0. {
        n(t, "threeMade")
    } else {
        three_attempts
    } * factor;
    let ideal_free = if old != 0. {
        n(t, "ftm")
    } else {
        free_attempts
    } * factor;
    let mut best: Option<(f64, f64, f64, f64, f64, f64)> = None;
    for three in 0..=three_attempts as u32 {
        let three = three as f64;
        let residual = target - 3. * three;
        let two_max = two_attempts.min(8191. - three);
        let low = ((residual - free_attempts) / 2.).ceil().clamp(0., two_max);
        let high = (residual / 2.).floor().clamp(0., two_max);
        let optimum = (ideal_two + 2. * (residual - ideal_free)) / 5.;
        let mut candidates = Vec::new();
        for two in [
            low,
            high,
            optimum.floor().clamp(low.min(high), low.max(high)),
            optimum.ceil().clamp(low.min(high), low.max(high)),
        ] {
            if !candidates.contains(&two) {
                candidates.push(two);
            }
        }
        for two in candidates {
            let free = round(residual - 2. * two).clamp(0., free_attempts);
            let points = 2. * two + 3. * three + free;
            let error = (points - target).abs();
            let cost = (two - ideal_two).powi(2)
                + (three - ideal_three).powi(2)
                + (free - ideal_free).powi(2);
            if best.is_none_or(|b| error < b.4 || (error == b.4 && cost < b.5)) {
                best = Some((two, three, free, points, error, cost));
            }
        }
    }
    let b = best.context("No attainable scoring solution")?;
    let mut next = t.clone();
    put(&mut next, "fgm", b.0 + b.1);
    put(&mut next, "threeMade", b.1);
    put(&mut next, "ftm", b.2);
    put(&mut next, "points", b.3);
    Ok(next)
}
pub fn change(t: &Value, key: &str, value: f64) -> Result<Value> {
    validate(t)?;
    ensure!(value.is_finite(), "Enter a valid number.");
    ensure!(
        value >= 0. || matches!(key, "plusMinus" | "plusMinusAverage"),
        "This value cannot be negative."
    );
    let mut next = t.clone();
    if let Some(&(_, total)) = AVERAGES.iter().find(|&&(k, _)| k == key) {
        ensure!(n(t, "gp") > 0., "Set games played before editing averages.");
        if key == "ppg" {
            next = score_to(t, value * n(t, "gp"))?;
        } else {
            put(&mut next, total, round(value * n(t, "gp")));
        }
    } else if matches!(key, "rpg" | "rebounds") {
        ensure!(
            key != "rpg" || n(t, "gp") > 0.,
            "Set games played before editing averages."
        );
        let target = round(if key == "rpg" {
            value * n(t, "gp")
        } else {
            value
        });
        let before = n(t, "oreb") + n(t, "dreb");
        let orb = if before != 0. {
            round(target * n(t, "oreb") / before)
        } else {
            0.
        };
        put(&mut next, "oreb", orb);
        put(&mut next, "dreb", target - orb);
    } else if let Some(&(_, made, attempted)) = PERCENTAGES.iter().find(|&&(k, _, _)| k == key) {
        ensure!(value <= 100., "Shooting percentages must be 0–100.");
        ensure!(
            n(t, attempted) > 0.,
            "Set shot attempts before editing this percentage."
        );
        return change(
            t,
            made,
            round(value * n(t, attempted) / 100.).min(
                crate::player_dna::schema()["seasonLimits"][made]
                    .as_f64()
                    .unwrap(),
            ),
        );
    } else {
        ensure!(
            crate::player_dna::schema()["seasonLimits"]
                .get(key)
                .is_some(),
            "Unsupported season field."
        );
        ensure!(value.fract() == 0., "Totals must be whole numbers.");
        // Avoid a costly loop or float-to-integer saturation on an invalid total.
        ensure!(
            value >= if key == "plusMinus" { -16384. } else { 0. }
                && value
                    <= crate::player_dna::schema()["seasonLimits"][key]
                        .as_f64()
                        .unwrap(),
            "Outside the stored range: {key}"
        );
        put(&mut next, key, value);
        match key {
            "fgm" => {
                ensure!(value <= n(t, "fga"), "Made shots cannot exceed attempts.");
                let low = 0f64.max(value - (n(t, "fga") - n(t, "threeAttempted")));
                let high = value.min(n(t, "threeAttempted"));
                put(&mut next, "threeMade", n(t, "threeMade").min(high).max(low));
            }
            "threeMade" => {
                ensure!(
                    value <= n(t, "threeAttempted"),
                    "Made shots cannot exceed attempts."
                );
                put(&mut next, "fgm", n(t, "fgm") - n(t, "threeMade") + value);
            }
            "ftm" => ensure!(value <= n(t, "fta"), "Made shots cannot exceed attempts."),
            _ => {}
        }
        if matches!(key, "fga" | "threeAttempted" | "fta") {
            ensure!(
                n(&next, "threeAttempted") <= n(&next, "fga"),
                "Three-point attempts cannot exceed field-goal attempts."
            );
            let two =
                (n(t, "fgm") - n(t, "threeMade")).min(n(&next, "fga") - n(&next, "threeAttempted"));
            let three = n(t, "threeMade").min(n(&next, "threeAttempted"));
            let free = n(t, "ftm").min(n(&next, "fta"));
            put(&mut next, "threeMade", three);
            put(&mut next, "fgm", two + three);
            put(&mut next, "ftm", free);
        }
        if matches!(
            key,
            "fgm" | "threeMade" | "ftm" | "fga" | "threeAttempted" | "fta"
        ) {
            let points = scoring(&next);
            put(&mut next, "points", points);
        }
    }
    validate(&next)?;
    Ok(next)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_native_edit_matches_wpf() {
        let cases: Value =
            serde_json::from_str(include_str!("../assets/season-math-regressions.json")).unwrap();
        for (i, c) in cases.as_array().unwrap().iter().enumerate() {
            assert_eq!(
                change(
                    &c["before"],
                    c["key"].as_str().unwrap(),
                    c["value"].as_f64().unwrap()
                )
                .ok(),
                if c["expected"].is_null() {
                    None
                } else {
                    Some(c["expected"].clone())
                },
                "Numeric edit case {i}: {} {}",
                c["key"],
                c["value"]
            );
        }
    }
    #[test]
    fn negative_half_rounds_like_javascript() {
        assert_eq!(round(-10.5), -10.);
        assert_eq!(round(-3.5), -3.);
        assert_eq!(round(-0.5), 0.);
    }
}
