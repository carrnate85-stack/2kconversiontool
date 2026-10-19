//! Trusted one-time CLI. Production target/stage/review are compiled constants;
//! no arbitrary path/command/PID arguments, GUI, Steam or game-memory access.
#![allow(dead_code)]
#[path = "../src/saved_media_publish.rs"]
mod saved_media_publish;
#[path = "saved-media-publish-closed.rs"]
mod process_guard;
fn main() -> anyhow::Result<()> {
    use anyhow::Context;
    let args = std::env::args().collect::<Vec<_>>();
    anyhow::ensure!(
        args.len() == 2,
        "Use one fixed action: --verify-reviewed, --publish-reviewed, --inspect-reviewed or --rollback-reviewed"
    );
    let report = match args[1].as_str() {
        "--verify-reviewed" => saved_media_publish::verify_reviewed_main()
            .context("Verifying fixed saved-media plan without writes")?,
        "--publish-reviewed" => saved_media_publish::publish_reviewed_main(process_guard::closed)
            .context("Publishing fixed saved-media plan; inspect its retained receipt before retrying")?,
        "--inspect-reviewed" => saved_media_publish::inspect_reviewed_main(process_guard::closed)
            .context("Inspecting fixed saved-media receipt without writes")?,
        "--rollback-reviewed" => saved_media_publish::rollback_reviewed_main(process_guard::closed)
            .context("Rolling back only exact owned saved-media publication")?,
        _ => anyhow::bail!("Unknown fixed action"),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
