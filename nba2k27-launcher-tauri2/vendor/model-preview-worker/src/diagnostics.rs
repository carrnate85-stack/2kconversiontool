use serde_json::Value;
pub fn sanitized(value: &Value) -> Value {
    match value {
        Value::String(s) => {
            let paths =
                regex::Regex::new(r#"(?i)(?:[a-z]:[\\/]|\\\\|//|/(?:Users|home)/)[^\r\n\"'<>]*"#)
                    .unwrap();
            Value::String(paths.replace_all(s, "[local path removed]").into_owned())
        }
        Value::Array(a) => Value::Array(a.iter().map(sanitized).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.to_lowercase().as_str(),
                        "gameroot"
                            | "outputroot"
                            | "target"
                            | "folder"
                            | "path"
                            | "userprofile"
                            | "username"
                            | "hostname"
                    )
                })
                .map(|(k, v)| (k.clone(), sanitized(v)))
                .collect(),
        ),
        _ => value.clone(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn private_paths_removed_without_losing_virtual_resources() {
        let v = sanitized(
            &json!({"gameRoot":"D:\\private\\game","name":"char/sig/png04090.iff","error":"Denied C:\\Users\\private person\\file.iff\nNext line","unc":"\\\\server\\secret\\file","unix":"/home/person/secret"}),
        );
        assert_eq!(v["name"], "char/sig/png04090.iff");
        assert!(v.get("gameRoot").is_none());
        let text = v.to_string();
        for secret in ["private person", "server", "secret", "person"] {
            assert!(!text.contains(secret));
        }
    }
}
