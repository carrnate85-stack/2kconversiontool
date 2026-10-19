use super::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

fn oracle() -> Value {
    let value: Value =
        serde_json::from_str(include_str!("../assets/camera-policy-regressions.json")).unwrap();
    assert_eq!(value["schema"], "nba2k27-camera-policy-oracle-v1");
    assert_eq!(value["gameAccess"], false);
    value
}
fn pattern(length: usize, seed: u64) -> Vec<u8> {
    (0..length)
        .map(|i| ((i as u64 + seed) % 251) as u8)
        .collect()
}
fn parameters(seed: u64, pose: [f32; 5]) -> Vec<u8> {
    let mut bytes = pattern(PARAMETER_BYTES, seed);
    set_framing(&mut bytes, pose);
    bytes
}
fn entry(seed: u64, kind: u32, a: u64, b: u64) -> Vec<u8> {
    let mut bytes = pattern(ENTRY_BYTES, seed);
    bytes[8..12].copy_from_slice(&kind.to_le_bytes());
    set_q(&mut bytes, 0x30, a);
    set_q(&mut bytes, 0x38, b);
    bytes
}
fn digest(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}
fn bad(value: &Value) -> f32 {
    match value.as_str().unwrap() {
        "nan" => f32::NAN,
        "infinity" => f32::INFINITY,
        _ => 99.,
    }
}
#[test]
fn metadata_bytes_and_rejections_match_unchanged_wpf_method() {
    let fixture = oracle();
    let cases = fixture["metadata"].as_array().unwrap();
    assert_eq!(cases.len(), 11);
    for case in cases {
        let mut source = pattern(
            case["length"].as_u64().unwrap() as usize,
            case["seed"].as_u64().unwrap(),
        );
        if source.len() == METADATA_BYTES {
            for (i, value) in [1u32, 1, 1, 10, 1, 10].into_iter().enumerate() {
                source[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        if let Some(word) = case["badWord"].as_i64().filter(|v| *v >= 0) {
            source[word as usize * 4..word as usize * 4 + 4].copy_from_slice(&99u32.to_le_bytes());
        }
        let result = unrestricted_metadata(&source);
        assert_eq!(
            result.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{case}"
        );
        if let Ok(bytes) = result {
            assert_eq!(digest(&bytes), case["hash"].as_str().unwrap(), "{case}");
            assert_eq!(bytes[..4], source[..4]);
            assert_eq!(bytes[8..], source[8..]);
            assert_eq!(d(&bytes, 4), 0);
        }
    }
}
#[test]
fn press_tracking_all_bytes_match_unchanged_wpf_transform() {
    let fixture = oracle();
    let cases = fixture["press"].as_array().unwrap();
    assert_eq!(cases.len(), 96);
    for case in cases {
        let target = entry(11, 8, 0x140200000, 0x140200000);
        let donor = entry(
            31,
            case["kind"].as_u64().unwrap() as u32,
            0x140100000,
            0x140105000,
        );
        let seed = case["seed"].as_u64().unwrap();
        let mut inputs = [
            parameters(seed, PRESS),
            parameters(seed + 1, PRESS_27),
            parameters(seed + 2, PRESS_27),
        ];
        if let Some(origin) = case["origin"].as_i64().filter(|v| *v >= 0) {
            set_f(
                &mut inputs[origin as usize],
                case["offset"].as_u64().unwrap() as usize,
                bad(&case["bad"]),
            );
        }
        if case["tolerantW"] == true {
            for (input, a, b) in [(1, 0.995f32, 1.005f32), (2, 1.005, 0.995)] {
                set_f(&mut inputs[input], 0x1c, a);
                set_f(&mut inputs[input], 0x3c, b);
            }
        }
        let result = press_27_tracking(
            &target, &donor, &inputs[0], &inputs[1], &inputs[2], 0x20000000, 0x20005000,
        );
        assert_eq!(
            result.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{case}"
        );
        if let Ok(output) = result {
            assert_eq!(
                digest(&output.entry),
                case["entry"].as_str().unwrap(),
                "{case}"
            );
            assert_eq!(
                digest(&output.primary),
                case["primary"].as_str().unwrap(),
                "{case}"
            );
            assert_eq!(
                digest(output.secondary.as_ref().unwrap()),
                case["secondary"].as_str().unwrap(),
                "{case}"
            );
        }
    }
}
#[test]
fn beluba_keeps_captured_geometry_with_side_tracking_exactly_as_wpf() {
    let fixture = oracle();
    let cases = fixture["beluba"].as_array().unwrap();
    assert_eq!(cases.len(), 64);
    for case in cases {
        let seed = case["seed"].as_u64().unwrap();
        let mut inputs = [parameters(seed, BELUBA), parameters(seed + 1, SIDE)];
        if let Some(origin) = case["origin"].as_i64().filter(|v| *v >= 0) {
            set_f(
                &mut inputs[origin as usize],
                case["offset"].as_u64().unwrap() as usize,
                bad(&case["bad"]),
            );
        }
        let result = beluba_side_parameters(&inputs[0], &inputs[1]);
        assert_eq!(
            result.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{case}"
        );
        if let Ok(output) = result {
            assert_eq!(digest(&output), case["hash"].as_str().unwrap(), "{case}");
        }
    }
}
#[test]
fn camera_step_float_bytes_match_original_csharp_expression() {
    let fixture = oracle();
    let cases = fixture["steps"]["rows"].as_array().unwrap();
    assert_eq!(cases.len(), 144);
    for case in cases {
        let source = pattern(PARAMETER_BYTES, case["seed"].as_u64().unwrap());
        let steps = Steps::new(
            case["height"].as_i64().unwrap() as i32,
            case["zoom"].as_i64().unwrap() as i32,
        )
        .unwrap();
        let output =
            stepped_parameters(case["id"].as_u64().unwrap() as u32, &source, steps).unwrap();
        assert_eq!(digest(&output), case["hash"].as_str().unwrap(), "{case}");
    }
}
#[test]
fn fractional_saved_steps_and_invalid_numeric_ranges_are_not_silently_changed() {
    for (height, zoom) in [
        (1.5, 1.),
        (1., 1.25),
        (f64::NAN, 1.),
        (1., f64::INFINITY),
        (-101., 1.),
        (1., 101.),
    ] {
        assert!(Steps::from_stored(height, zoom).is_err());
    }
    assert_eq!(
        Steps::from_stored(-100., 100.).unwrap(),
        Steps {
            height: -100,
            zoom: 100
        }
    );
    assert!(
        stepped_parameters(999, &pattern(PARAMETER_BYTES, 0), Steps::new(1, 1).unwrap()).is_err()
    );
    assert!(stepped_parameters(51, &[0; 8], Steps::new(1, 1).unwrap()).is_err());
}

#[test]
fn retained_private_geometry_accepts_only_exact_bounded_policy_outputs() {
    for id in [21, 51, 52, 53] {
        for steps in [
            Steps::new(-100, -100).unwrap(),
            Steps::new(1, 1).unwrap(),
            Steps::new(100, 100).unwrap(),
        ] {
            let current = stepped_parameters(id, &pattern(PARAMETER_BYTES, 17), steps).unwrap();
            assert!(owns_stepped_geometry(id, &current).unwrap());
            let mut corrupt = current.clone();
            corrupt[0x14..0x18].copy_from_slice(&f32::NAN.to_le_bytes());
            assert!(!owns_stepped_geometry(id, &corrupt).unwrap());
            corrupt = current.clone();
            corrupt[0x34] ^= 1;
            assert!(!owns_stepped_geometry(id, &corrupt).unwrap());
        }
    }
    assert!(owns_stepped_geometry(51, &[0; 8]).is_err());
    assert!(owns_stepped_geometry(999, &vec![0; PARAMETER_BYTES]).is_err());
}
#[test]
fn private_camera_ranges_cannot_alias_each_other_or_stock_sources() {
    let target = entry(11, 8, 0x140200000, 0x140200000);
    let donor = entry(31, 3, 0x140100000, 0x140105000);
    let source = parameters(0, PRESS);
    let a = parameters(1, PRESS_27);
    let b = parameters(2, PRESS_27);
    for (left, right) in [
        (0, 0x20005000),
        (0x20000000, 0x20000000),
        (0x20000000, 0x20001000),
        (0x140100000, 0x20000000),
        (0x20000000, 0x140105000),
        (0x20000001, 0x20005000),
        (u64::MAX - 16, 0x20005000),
    ] {
        assert!(press_27_tracking(&target, &donor, &source, &a, &b, left, right).is_err());
    }
    // Stock records legitimately overlap in Version 2; only private copies
    // must be disjoint. Never mutate an overlapping stock donor in place.
    let overlapping = entry(31, 3, 0x1499922c0, 0x1499929e0);
    assert!(
        press_27_tracking(
            &target,
            &overlapping,
            &source,
            &a,
            &b,
            0x20000000,
            0x20005000
        )
        .is_ok()
    );
    assert!(
        press_27_tracking(
            &target[..32],
            &donor,
            &source,
            &a,
            &b,
            0x20000000,
            0x20005000
        )
        .is_err()
    );
}
#[test]
fn broadcast_low_and_beluba_replace_behavior_without_losing_target_identity() {
    for id in [21, 51, 52] {
        let target = entry(11, 8, 0x140200000, 0x140200000);
        let donor = entry(31, 8, 0x140100000, 0x140100000);
        let source = parameters(
            0,
            match id {
                21 => PRESS,
                51 => LOW_18,
                _ => BELUBA,
            },
        );
        let tracking = parameters(
            1,
            if id == 52 {
                SIDE
            } else {
                [-1500., 500., 0., 0.995, 60.]
            },
        );
        let output = single_tracking(id, &target, &donor, &source, &tracking, 0x20000000).unwrap();
        assert_eq!(output.entry[..8], target[..8]);
        assert_eq!(output.entry[0x20..0x28], target[0x20..0x28]);
        assert_eq!(output.entry[0x28..0x30], donor[0x28..0x30]);
        assert_eq!(q(&output.entry, 0x30), 0x20000000);
        assert_eq!(q(&output.entry, 0x38), 0x20000000);
        if id == 52 {
            assert_eq!(output.entry[0x10..0x20], target[0x10..0x20]);
            framing(&output.primary, BELUBA_HYBRID).unwrap();
        } else {
            assert_eq!(output.entry[0x10..0x20], donor[0x10..0x20]);
            assert_eq!(f(&output.primary, 0x1c), 0.995);
            assert_eq!(f(&output.primary, 0x3c), 0.995);
        }
        assert!(single_tracking(id, &target, &donor, &source, &tracking, 0x140100000).is_err());
    }
}
#[test]
fn camera_modes_preserve_native_order_and_append_only_selected_extra_slots() {
    // Captured from the separately audited Version 2 on-disk mode lists.
    // These numeric examples exercise policy only, not native write authority.
    let stock: &[&[u32]] = &[
        &[1, 2, 3, 4, 15, 8, 9, 16, 17, 19, 20, 22, 23, 0],
        &[1, 2, 3, 4, 15, 8, 9, 16, 17, 19, 20, 22, 23, 0],
        &[1, 2, 3, 4, 15, 8, 9, 16, 17, 19, 20, 22, 23, 0],
        &[1, 2, 3, 4, 15, 11, 12, 16, 17, 19, 35, 20, 22, 23, 33],
        &[1, 2, 3, 4, 15, 11, 12, 16, 17, 19, 35, 20, 22, 23, 33],
        &[1, 2, 7, 3, 4, 16, 18, 35, 25, 20, 22],
        &[1, 2, 3, 4, 11, 12, 16, 22],
        &[1, 2, 3, 4, 11, 12, 16],
        &[1, 2, 3, 4, 11, 12, 16, 35, 22],
        &[5, 6, 13, 14],
        &[1, 2, 3, 4, 17, 20, 22],
        &[1, 2, 3, 7, 4, 15, 10, 12, 16, 17, 35, 20, 22],
        &[1, 2, 3, 4, 12],
    ];
    for source in stock {
        for count in 1..=CURATED.len() {
            let selected: Vec<u32> = CURATED[..count].iter().rev().map(|(id, _)| *id).collect();
            let output = mode_ids(source, &selected).unwrap();
            assert_eq!(
                &output[output.len() - count..],
                &(FIRST_EXTRA..FIRST_EXTRA + count as u32).collect::<Vec<_>>()
            );
            assert_eq!(
                output[..output.len() - count],
                source
                    .iter()
                    .copied()
                    .filter(|id| !selected.contains(id))
                    .collect::<Vec<_>>()
            );
        }
    }
    assert_eq!(
        mode_ids(&[1, 1, 51, 63, 64, 2], &[51, 52]).unwrap(),
        vec![1, 2, 63, 64]
    );
    for selected in [vec![], vec![51, 51], vec![72], vec![1]] {
        assert!(mode_ids(&[1, 2], &selected).is_err());
    }
    assert!(mode_ids(&[], &[51]).is_err());
    assert!(mode_ids(&[72], &[51]).is_err());
    assert!(mode_ids(&[1; 65], &[51]).is_err());
}
