use super::*;
fn root() -> PathBuf {
    let path = std::env::current_dir()
        .unwrap()
        .join("artifacts")
        .join(format!(
            "qa084-artwork-self-{}",
            crate::backend::unique_id()
        ));
    fs::create_dir_all(path.join("Data")).unwrap();
    path
}
fn request() -> Value {
    json!({"kind":"portrait","key":"[\"A\",\"B\",3]","choose":true,"destination":{"recordIndex":1,"firstName":"A","lastName":"B","cyberfaceId":3}})
}
const TAB: &str = "11111111-2222-4333-8444-555555555555";
fn source(root: &Path) -> PathBuf {
    let p = root.join("image.png");
    fs::write(
        &p,
        crate::roster_artwork::png(2, 1, &[255, 0, 0, 128, 0, 255, 0, 255]).unwrap(),
    )
    .unwrap();
    p
}
fn follow(v: &Value, id: &Value, action: &str) -> Value {
    let mut r = v.clone();
    r.as_object_mut().unwrap().remove("choose");
    r["id"] = id.clone();
    r["action"] = json!(action);
    r
}
#[test]
fn prepare_is_no_publication_and_exact_owner_commit_preserves_other_library_entries() {
    let r = root();
    let p = source(&r);
    let path = r.join("Data/player-editor-artwork.json");
    fs::write(&path, b"{\"other\":\"keep\"}").unwrap();
    let s = Store::default();
    let v = request();
    let a = s
        .request(&r, TAB, &v, &|| Ok(()), &mut || Ok(Some(p.clone())))
        .unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"{\"other\":\"keep\"}");
    let mut c = follow(&v, &a["id"], "commit");
    c["destination"]["recordIndex"] = json!(2);
    assert!(
        s.request(&r, TAB, &c, &|| Ok(()), &mut || panic!())
            .is_err()
    );
    c = follow(&v, &a["id"], "commit");
    let value = s
        .request(&r, TAB, &c, &|| Ok(()), &mut || panic!())
        .unwrap();
    assert_eq!(value, a["dataUrl"]);
    let d: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(d["other"], "keep");
    assert_eq!(d.as_object().unwrap().len(), 2);
    assert!(
        s.request(&r, TAB, &c, &|| Ok(()), &mut || panic!())
            .is_err()
    );
}
#[test]
fn canceled_expired_replaced_and_changed_library_candidates_never_publish() {
    for fault in ["cancel", "expired", "replacement", "bytes", "stop"] {
        let r = root();
        let p = source(&r);
        let path = r.join("Data/player-editor-artwork.json");
        fs::write(&path, b"{}").unwrap();
        let s = Store::default();
        let v = request();
        let a = s
            .request(&r, TAB, &v, &|| Ok(()), &mut || Ok(Some(p.clone())))
            .unwrap();
        let c = follow(&v, &a["id"], "commit");
        match fault {
            "cancel" => {
                s.request(
                    &r,
                    TAB,
                    &follow(&v, &a["id"], "cancel"),
                    &|| Ok(()),
                    &mut || panic!(),
                )
                .unwrap();
            }
            "expired" => {
                s.pending.lock().unwrap().as_mut().unwrap().since =
                    Instant::now() - Duration::from_secs(61)
            }
            "replacement" => {
                fs::rename(&path, r.join("old.json")).unwrap();
                fs::write(&path, b"{}").unwrap();
            }
            "bytes" => fs::write(&path, b"{\"foreign\":1}").unwrap(),
            _ => {}
        }
        let before = fs::read(&path).unwrap();
        let check = || {
            ensure!(fault != "stop", "cancelled");
            Ok(())
        };
        assert!(
            s.request(&r, TAB, &c, &check, &mut || panic!()).is_err(),
            "{fault}"
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
#[test]
fn image_bounds_aspect_alpha_and_request_surface_are_strict() {
    assert_eq!(dimensions(1920, 1080), (512, 288));
    assert_eq!(dimensions(2, 1), (2, 1));
    assert_eq!(dimensions(1, 8192), (1, 512));
    let png = crate::roster_artwork::png(2, 1, &[255, 0, 0, 128, 0, 255, 0, 255]).unwrap();
    let value = image(&png, &|| Ok(())).unwrap();
    assert!(value.starts_with("data:image/png;base64,"));
    assert!(image(b"not an image at all", &|| Ok(())).is_err());
    assert!(image(&png, &|| anyhow::bail!("cancelled")).is_err());
    let mut v = request();
    v["path"] = json!("C:/foreign");
    assert!(Owner::parse(TAB, &v).is_err());
    let mut v = request();
    v["destination"]["extra"] = json!(1);
    assert!(Owner::parse(TAB, &v).is_err());
    assert!(Owner::parse("foreign", &request()).is_err());
}
