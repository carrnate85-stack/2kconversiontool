use super::*;
fn fixture(name: &str) -> Value {
    json!({"format":"2k23-season-catalog","version":1,"game":"NBA2K23","source":{"sha256":"a".repeat(64),"playerSchema":"48362c10f664d79bb3fc8c81d54a72288d80debc52d2efd5253e690547af2eaa","historySchema":"df2519eb509843ef0df97ecb8666131875c75a6deeeba22db51c2e3f6790ee36"},"teams":[{"id":0,"name":"Test","city":"City","category":"current","classicEndYearRaw":0}],"records":[],"players":[{"id":0,"firstName":name,"lastName":"Source","teams":[0],"history":[],"categories":["current"]}]})
}
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("qa083-catalog-{}", crate::backend::unique_id()));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn request(s: &Store, r: &Root, v: Value, path: Option<PathBuf>) -> Result<Value> {
    s.request(&r.0, &v, &|| Ok(()), &mut || Ok(path.clone()))
}
#[test]
fn import_cancel_invalid_and_aba_keep_exact_generations() {
    let r = Root::new();
    let s = Store::default();
    let initial = request(&s, &r, json!({"action":"status"}), None).unwrap();
    assert_eq!(initial["bundled"], true);
    assert!(
        request(&s, &r, json!({"action":"import"}), None)
            .unwrap()
            .is_null()
    );
    let path = r.0.join("source.2k23stats");
    fs::write(&path, b"invalid").unwrap();
    assert!(request(&s, &r, json!({"action":"import"}), Some(path.clone())).is_err());
    assert_eq!(
        request(&s, &r, json!({"action":"status"}), None).unwrap(),
        initial
    );
    fs::write(&path, serde_json::to_vec(&fixture("First")).unwrap()).unwrap();
    let a = request(&s, &r, json!({"action":"import"}), Some(path.clone())).unwrap();
    assert_eq!(a["unified"], false);
    assert_eq!(a["bundled"], false);
    fs::write(&path, serde_json::to_vec(&fixture("Second")).unwrap()).unwrap();
    request(&s, &r, json!({"action":"import"}), Some(path.clone())).unwrap();
    fs::write(&path, serde_json::to_vec(&fixture("First")).unwrap()).unwrap();
    let again = request(&s, &r, json!({"action":"import"}), Some(path)).unwrap();
    assert_ne!(a["catalogId"], again["catalogId"]);
    assert!(
        request(
            &s,
            &r,
            json!({"action":"search","catalogId":a["catalogId"]}),
            None
        )
        .is_err()
    );
    let reopened = Store::default();
    assert_eq!(
        request(&reopened, &r, json!({"action":"status"}), None).unwrap()["players"],
        1
    );
    drop(reopened);
    drop(s);
}
#[test]
fn refuses_path_authority_tampered_pointer_and_existing_hash_object() {
    let r = Root::new();
    let s = Store::default();
    request(&s, &r, json!({"action":"status"}), None).unwrap();
    assert!(request(&s, &r, json!({"action":"import","path":"foreign"}), None).is_err());
    let path = r.0.join("source");
    let v = fixture("First");
    let canonical = serde_json::to_vec(import::validate(&v, &|| Ok(())).unwrap().value()).unwrap();
    let (dir, _pins) = directory(&r.0).unwrap();
    fs::write(
        dir.join(format!("{}.2k23catalog", digest(&canonical))),
        b"foreign bytes",
    )
    .unwrap();
    fs::write(&path, serde_json::to_vec(&v).unwrap()).unwrap();
    assert!(request(&s, &r, json!({"action":"import"}), Some(path.clone())).is_err());
    fs::remove_file(dir.join(format!("{}.2k23catalog", digest(&canonical)))).unwrap();
    fs::write(dir.join("selected.json"), b"foreign pointer").unwrap();
    assert!(request(&s, &r, json!({"action":"import"}), Some(path)).is_err());
    assert_eq!(
        request(&s, &r, json!({"action":"status"}), None).unwrap()["bundled"],
        true
    );
    drop(s);
}
#[test]
fn cancelled_after_dialog_never_changes_generation_and_inflight_queries_expire() {
    use std::cell::Cell;
    let r = Root::new();
    let s = Store::default();
    let initial = request(&s, &r, json!({"action":"status"}), None).unwrap();
    let path = r.0.join("source");
    fs::write(&path, serde_json::to_vec(&fixture("First")).unwrap()).unwrap();
    let canceled = Cell::new(false);
    assert!(
        s.request(
            &r.0,
            &json!({"action":"import"}),
            &|| {
                ensure!(!canceled.get(), "cancelled");
                Ok(())
            },
            &mut || {
                canceled.set(true);
                Ok(Some(path.clone()))
            }
        )
        .is_err()
    );
    assert_eq!(
        request(&s, &r, json!({"action":"status"}), None).unwrap(),
        initial
    );
    let checks = Cell::new(0);
    let result = s.request(
        &r.0,
        &json!({"action":"search","catalogId":initial["catalogId"]}),
        &|| {
            let n = checks.get();
            checks.set(n + 1);
            if n == 1 {
                request(&s, &r, json!({"action":"import"}), Some(path.clone()))?;
            }
            Ok(())
        },
        &mut || anyhow::bail!("query must not choose"),
    );
    assert!(result.is_err());
    assert_eq!(
        request(&s, &r, json!({"action":"status"}), None).unwrap()["bundled"],
        false
    );
    drop(s);
}

#[test]
fn loads_wpf_selected_pointer_without_version_and_refuses_unknown_version() {
    let r = Root::new();
    let (dir, _pins) = directory(&r.0).unwrap();
    let bytes = serde_json::to_vec(
        import::validate(&fixture("Old WPF"), &|| Ok(()))
            .unwrap()
            .value(),
    )
    .unwrap();
    let hash = digest(&bytes);
    fs::write(dir.join(format!("{hash}.2k23catalog")), bytes).unwrap();
    fs::write(
        dir.join("selected.json"),
        serde_json::to_vec(&json!({"catalogId":hash})).unwrap(),
    )
    .unwrap();
    let store = Store::default();
    assert_eq!(
        request(&store, &r, json!({"action":"status"}), None).unwrap()["bundled"],
        false
    );
    drop(store);
    fs::write(
        dir.join("selected.json"),
        serde_json::to_vec(&json!({"version":2,"catalogId":hash})).unwrap(),
    )
    .unwrap();
    let store = Store::default();
    assert!(request(&store, &r, json!({"action":"status"}), None).is_err());
    drop(store);
}
