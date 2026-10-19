fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        (4..=5).contains(&args.len()),
        "Usage: legacy-media-offline SOURCE_PORTABLE PROFILE16 GAME_ROOT [NEW_WORKSPACE_STAGE]"
    );
    let destination = args.get(4).map(std::path::Path::new);
    if let Some(path) = destination {
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Missing stage parent"))?
            .canonicalize()?;
        let project = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()?;
        anyhow::ensure!(
            parent.starts_with(project.join("artifacts").canonicalize()?),
            "Audit CLI writes only a new project-artifacts stage"
        );
    }
    let report = jukebox_api_harness::audit(
        std::path::Path::new(&args[1]),
        &args[2],
        std::path::Path::new(&args[3]),
        destination,
    )?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
