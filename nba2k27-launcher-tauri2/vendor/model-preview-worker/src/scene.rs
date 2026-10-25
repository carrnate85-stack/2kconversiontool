//! Same authored object/pivot translations as the preserved renderer backend.
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
pub fn variants(name: &str) -> Vec<String> {
    let name = name.replace('\\', "/");
    let mut names = BTreeSet::from([
        name.clone(),
        name.rsplit('/').next().unwrap_or(&name).into(),
    ]);
    if let Some((_, tail)) = name.rsplit_once(':') {
        names.insert(tail.into());
    }
    if let Some((_, rest)) = name.split_once("@GameObjects@") {
        names.insert(rest.into());
        if let Some((head, _)) = rest.split_once(':') {
            names.insert(head.into());
        }
        if let Some((_, tail)) = rest.rsplit_once(':') {
            names.insert(tail.into());
        }
    }
    names
        .into_iter()
        .map(|n| n.trim().to_lowercase())
        .filter(|n| !n.is_empty())
        .collect()
}
fn translation(transforms: &Value, name: &str) -> [f64; 3] {
    let Some(map) = transforms.as_object() else {
        return [0.; 3];
    };
    let entries: Vec<_> = map.iter().collect();
    let mut index = entries.iter().position(|(n, _)| *n == name);
    let mut visited = BTreeSet::new();
    let mut out = [0.; 3];
    while let Some(i) = index {
        if !visited.insert(i) {
            break;
        }
        let Some((_, node)) = entries.get(i) else {
            break;
        };
        for (a, n) in out.iter_mut().enumerate() {
            *n += node["Translate"][a].as_f64().unwrap_or(0.);
        }
        index = node["Parent"].as_u64().map(|n| n as usize);
    }
    out
}
fn offset(root: &Value, obj: &Value) -> [f64; 3] {
    let transform = text(&obj["Transform"]);
    let Some((_, pivot)) = transform.rsplit_once('|') else {
        return [0.; 3];
    };
    let assembly = transform.split(':').next().unwrap_or("");
    let mut candidates = Vec::new();
    if let Some(models) = root["Model"].as_object() {
        for model in models.values() {
            if let Some(transforms) = model["Transform"].as_object() {
                for name in transforms.keys() {
                    if !name
                        .rsplit('|')
                        .next()
                        .unwrap_or("")
                        .eq_ignore_ascii_case(pivot)
                    {
                        continue;
                    }
                    let t = translation(&model["Transform"], name);
                    let same = name
                        .split(':')
                        .next()
                        .unwrap_or("")
                        .eq_ignore_ascii_case(assembly);
                    candidates.push((
                        same,
                        name.split('|').count(),
                        t.iter().map(|n| n.abs()).sum::<f64>(),
                        t,
                    ));
                }
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)).then(b.2.total_cmp(&a.2)));
    candidates.first().map(|c| c.3).unwrap_or([0.; 3])
}
fn collect(
    v: &Value,
    direct: &mut HashMap<String, Vec<Value>>,
    fallback: &mut HashMap<String, Vec<Value>>,
    depth: usize,
) -> Result<()> {
    ensure!(depth <= 128, "Scene placement nesting exceeds limit");
    if let (Some(objects), Some(models)) = (v["Object"].as_object(), v["Model"].as_object()) {
        for (name, obj) in objects {
            let Some(matrix) = obj["Matrix"].as_array().filter(|m| m.len() >= 16) else {
                continue;
            };
            let target = text(&obj["Target"]);
            if target.is_empty() {
                continue;
            }
            let Some(mut m) = matrix
                .iter()
                .take(16)
                .map(Value::as_f64)
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            if !m.iter().all(|n| n.is_finite()) {
                continue;
            }
            let t = offset(v, obj);
            for axis in 0..3 {
                m[12 + axis] += m[axis] * t[0] + m[4 + axis] * t[1] + m[8 + axis] * t[2];
            }
            let converted = [
                m[0], -m[2], m[1], 0., -m[8], m[10], -m[9], 0., m[4], -m[6], m[5], 0., m[12],
                -m[14], m[13], 1.,
            ];
            let place = json!({"name":name,"target":target,"matrix":converted});
            let keys: BTreeSet<_> = variants(target).into_iter().chain(variants(name)).collect();
            for key in keys {
                direct.entry(key).or_default().push(place.clone());
            }
            if let Some(model) = models.get(target) {
                if let Some(prims) = model["Prim"].as_array() {
                    for p in prims {
                        for k in ["Mesh", "Material"] {
                            for key in variants(text(&p[k])) {
                                fallback.entry(key).or_default().push(place.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(o) = v.as_object() {
        for child in o.values() {
            collect(child, direct, fallback, depth + 1)?;
        }
    }
    if let Some(a) = v.as_array() {
        for child in a {
            collect(child, direct, fallback, depth + 1)?;
        }
    }
    Ok(())
}
pub fn placements(scene: &Value) -> Result<HashMap<String, Vec<Value>>> {
    let mut direct = HashMap::new();
    let mut fallback = HashMap::new();
    collect(scene, &mut direct, &mut fallback, 0)?;
    for (key, places) in fallback {
        direct.entry(key).or_insert(places);
    }
    Ok(direct)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parent_pivot_translation_is_rotated_by_object() {
        let root = json!({"Model":{"chair":{"Transform":{"assembly:root":{"Translate":[1,2,3]},"assembly:root|pivot":{"Translate":[4,5,6],"Parent":0}}}},"Object":{"chair_obj":{"Target":"chair","Transform":"assembly:root|pivot","Matrix":[2,0,0,0,0,2,0,0,0,0,2,0,10,20,30,1]}}});
        let p = placements(&root).unwrap();
        assert_eq!(p["chair"][0]["matrix"][12], 20.);
        assert_eq!(p["chair"][0]["matrix"][13], -48.);
        assert_eq!(p["chair"][0]["matrix"][14], 34.);
        assert_eq!(p["chair"].len(), 1);
    }
    #[test]
    fn transform_cycles_are_bounded() {
        let t = json!({"a":{"Translate":[1,0,0],"Parent":1},"b":{"Translate":[2,0,0],"Parent":0}});
        assert_eq!(translation(&t, "a"), [3., 0., 0.]);
    }
    #[test]
    fn direct_targets_win_over_primitive_aliases() {
        let identity = [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1];
        let scene = json!({
            "Model": {
                "basket": {"Prim": [{"Mesh": "shared", "Material": "metal"}]},
                "glass": {"Prim": [{"Mesh": "basket", "Material": "glass"}]}
            },
            "Object": {
                "basket_object": {"Target": "basket", "Matrix": identity},
                "glass_object": {"Target": "glass", "Matrix": identity}
            }
        });
        let p = placements(&scene).unwrap();
        assert_eq!(p["basket"].len(), 1);
        assert_eq!(p["basket"][0]["name"], "basket_object");
        assert_eq!(p["shared"].len(), 1);
        assert_eq!(p["shared"][0]["name"], "basket_object");
    }
}
