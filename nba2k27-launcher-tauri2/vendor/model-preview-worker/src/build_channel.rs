pub const DEVELOPMENT: bool = cfg!(feature = "development");
pub const NAME: &str = if DEVELOPMENT {
    "Development"
} else {
    "Production"
};
pub const TITLE: &str = if DEVELOPMENT {
    "NBA 2K27 Exporter - Development"
} else {
    "NBA 2K27 Exporter"
};

pub fn profile(base: std::path::PathBuf) -> std::path::PathBuf {
    if DEVELOPMENT {
        base.join("Development/WebView2")
    } else {
        base.join("WebView2")
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn profile_and_brand_match_compiled_channel() {
        let base = std::path::PathBuf::from("local-data");
        let actual = super::profile(base.clone());
        if cfg!(feature = "development") {
            assert_ne!(actual, base.join("WebView2"));
            assert!(super::TITLE.contains("Development"));
        } else {
            assert_eq!(actual, base.join("WebView2"));
            assert_eq!(super::TITLE, "NBA 2K27 Exporter");
        }
    }
}
