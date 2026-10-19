//! Pure WPF-compatible shared-size policy. No process discovery or writes.
#![allow(dead_code)]
use anyhow::{Result, ensure};

pub(crate) const ORIGINAL: u32 = 0x3f7a_e148;

pub(crate) fn target_bits(percent: u8) -> Result<u32> {
    ensure!((95..=100).contains(&percent), "Player size must be 95–100%");
    // Keep the original conversion order: float baseline -> double arithmetic
    // -> one float rounding. 100 is the exact captured stock bit pattern.
    Ok(if percent == 100 {
        ORIGINAL
    } else {
        ((f32::from_bits(ORIGINAL) as f64 * f64::from(percent) / 100.0) as f32).to_bits()
    })
}

pub(crate) fn valid_target(bits: u32) -> bool {
    (95..=100).any(|p| target_bits(p).ok() == Some(bits))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ownership {
    pub applied: u32,
    pub requested: u32,
}
impl Ownership {
    pub(crate) fn validate(self) -> Result<()> {
        ensure!(
            valid_target(self.applied) && valid_target(self.requested),
            "Invalid scaler ownership record"
        );
        Ok(())
    }
    pub(crate) fn contains(self, current: u32) -> bool {
        current == ORIGINAL || current == self.applied || current == self.requested
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Change {
    pub before: u32,
    pub after: u32,
}

pub(crate) fn plan(
    current: u32,
    percent: u8,
    ownership: Option<Ownership>,
) -> Result<Option<Change>> {
    let after = target_bits(percent)?;
    if let Some(owned) = ownership {
        owned.validate()?;
        ensure!(
            owned.contains(current),
            "Player size changed outside this session"
        );
    } else {
        ensure!(current != 0, "Waiting for the original player size input");
        ensure!(
            current == ORIGINAL,
            "Player size was changed outside this session"
        );
    }
    Ok((current != after).then_some(Change {
        before: current,
        after,
    }))
}

pub(crate) fn restore(current: u32, ownership: Option<Ownership>) -> Result<Option<Change>> {
    // A new worker cannot adopt another session's non-default value, even when
    // it happens to equal one of the six supported targets.
    let Some(owned) = ownership else {
        return Ok(None);
    };
    owned.validate()?;
    ensure!(
        owned.contains(current),
        "Player size restoration ownership conflict"
    );
    Ok((current != ORIGINAL).then_some(Change {
        before: current,
        after: ORIGINAL,
    }))
}

pub(crate) fn scale_pair(scale: u32, inverse: u32) -> bool {
    let scale = f32::from_bits(scale);
    let inverse = f32::from_bits(inverse);
    scale.is_finite()
        && inverse.is_finite()
        && scale > 0.1
        && scale < 10.0
        && (scale * inverse - 1.0).abs() < 0.001
}

/// Reference policy only. The V2 runtime deliberately does not enable optional
/// NPC writes until its population-list and object-lifetime guards are proved.
pub(crate) fn arena_seed(original: u32, shared_target: u32) -> Result<(u32, u32)> {
    ensure!(valid_target(shared_target), "Unsupported arena size target");
    let original = f32::from_bits(original);
    ensure!(
        original.is_finite() && original > 0.1 && original < 10.0,
        "Invalid arena scale"
    );
    let desired = original * f32::from_bits(shared_target);
    let inverse = 1.0f32 / desired;
    ensure!(
        scale_pair(desired.to_bits(), inverse.to_bits()),
        "Invalid arena scale seed"
    );
    Ok((desired.to_bits(), inverse.to_bits()))
}

pub(crate) fn owned_arena_pair(
    original: u32,
    original_inverse: u32,
    scale: u32,
    inverse: u32,
) -> bool {
    if !scale_pair(original, original_inverse) || !scale_pair(scale, inverse) {
        return false;
    }
    let ratio = f32::from_bits(scale) / f32::from_bits(original);
    (ratio - 1.0).abs() < 0.0005
        || (95..100).any(|p| (ratio - f32::from_bits(target_bits(p).unwrap())).abs() < 0.0005)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_wpf_targets_and_safe_range() {
        assert_eq!(target_bits(100).unwrap(), ORIGINAL);
        for p in 0..=u8::MAX {
            assert_eq!(target_bits(p).is_ok(), (95..=100).contains(&p));
        }
        for p in 95..100 {
            assert!(target_bits(p).unwrap() < ORIGINAL);
        }
    }

    #[test]
    fn no_adoption_of_foreign_or_stale_values() {
        let low = target_bits(95).unwrap();
        assert!(plan(0, 95, None).is_err());
        assert!(plan(low, 95, None).is_err());
        assert!(plan(f32::NAN.to_bits(), 95, None).is_err());
        assert_eq!(restore(low, None).unwrap(), None);
        assert!(
            plan(
                ORIGINAL,
                95,
                Some(Ownership {
                    applied: 0,
                    requested: low
                })
            )
            .is_err()
        );
        let owned = Ownership {
            applied: low,
            requested: target_bits(96).unwrap(),
        };
        for current in [ORIGINAL, owned.applied, owned.requested] {
            assert!(plan(current, 97, Some(owned)).is_ok());
            assert!(restore(current, Some(owned)).is_ok());
        }
        assert!(plan(target_bits(99).unwrap(), 97, Some(owned)).is_err());
        assert!(restore(target_bits(99).unwrap(), Some(owned)).is_err());
    }

    #[test]
    fn changes_are_minimal_and_restoration_exact() {
        let low = target_bits(95).unwrap();
        assert_eq!(plan(ORIGINAL, 100, None).unwrap(), None);
        let change = plan(ORIGINAL, 95, None).unwrap().unwrap();
        assert_eq!(
            change,
            Change {
                before: ORIGINAL,
                after: low
            }
        );
        let owned = Ownership {
            applied: low,
            requested: low,
        };
        assert_eq!(plan(low, 95, Some(owned)).unwrap(), None);
        assert_eq!(
            restore(low, Some(owned)).unwrap(),
            Some(Change {
                before: low,
                after: ORIGINAL
            })
        );
    }

    #[test]
    fn arena_seed_is_shared_target_not_percentage_or_baseline_ratio() {
        let original = 1.2f32.to_bits();
        let target = target_bits(95).unwrap();
        let (seed, inverse) = arena_seed(original, target).unwrap();
        assert_eq!(seed, (1.2f32 * f32::from_bits(target)).to_bits());
        assert_ne!(seed, (1.2f32 * 0.95).to_bits());
        assert!(scale_pair(seed, inverse));
        assert!(owned_arena_pair(
            original,
            (1.0f32 / 1.2).to_bits(),
            seed,
            inverse
        ));
        assert!(!scale_pair(f32::NAN.to_bits(), 1.0f32.to_bits()));
        assert!(!scale_pair(1.0f32.to_bits(), f32::INFINITY.to_bits()));
        assert!(!scale_pair(0.1f32.to_bits(), 10.0f32.to_bits()));
    }

    #[test]
    fn original_csharp_oracle_comparisons() {
        let rows: serde_json::Value =
            serde_json::from_str(include_str!("../assets/scaler-policy-regressions.json")).unwrap();
        assert_eq!(rows["gameAccess"], false);
        for row in rows["targets"].as_array().unwrap() {
            assert_eq!(
                target_bits(row["percent"].as_u64().unwrap() as u8).unwrap(),
                row["bits"].as_u64().unwrap() as u32
            );
        }
        for row in rows["owned"].as_array().unwrap() {
            let owned = Ownership {
                applied: row["applied"].as_u64().unwrap() as u32,
                requested: row["requested"].as_u64().unwrap() as u32,
            };
            assert_eq!(
                owned.contains(row["current"].as_u64().unwrap() as u32),
                row["result"].as_bool().unwrap()
            );
        }
        for row in rows["pairs"].as_array().unwrap() {
            assert_eq!(
                scale_pair(
                    row["scale"].as_u64().unwrap() as u32,
                    row["inverse"].as_u64().unwrap() as u32
                ),
                row["result"].as_bool().unwrap()
            );
        }
        for row in rows["seeds"].as_array().unwrap() {
            let seed = arena_seed(
                row["original"].as_u64().unwrap() as u32,
                row["target"].as_u64().unwrap() as u32,
            )
            .unwrap();
            assert_eq!(
                seed,
                (
                    row["scale"].as_u64().unwrap() as u32,
                    row["inverse"].as_u64().unwrap() as u32
                )
            );
        }
        for row in rows["arenaOwned"].as_array().unwrap() {
            assert_eq!(
                owned_arena_pair(
                    row["original"].as_u64().unwrap() as u32,
                    row["originalInverse"].as_u64().unwrap() as u32,
                    row["scale"].as_u64().unwrap() as u32,
                    row["inverse"].as_u64().unwrap() as u32
                ),
                row["result"].as_bool().unwrap()
            );
        }
    }
}
