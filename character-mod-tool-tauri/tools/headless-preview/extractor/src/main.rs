// Read-only NBA 2K27 manifest tool. Lists entries, or extracts a player's IFFs
// (decompressed) into a folder laid out like a Character Mod Tool staged player.
//   player-extract list <game-root> <regex> [limit]
//   player-extract player <game-root> <png-id> <out-dir>
//   player-extract headshot <game-root> <graphic-id> <out.dds>
//   player-extract members <game-root> <manifest-path> [regex]
//   player-extract member <game-root> <manifest-path> <member-name>
//   player-extract buffer <game-root> <manifest-path> <member-name> <size> <out.bin>
//   player-extract dump <game-root> <manifest-path> <regex> <out-dir>
use anyhow::{Context, Result, bail, ensure};
use nba2k_assets::{catalog::Catalog, codec};
use std::{fs, path::Path, sync::atomic::AtomicBool};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().context("Usage: player-extract list|player <game-root> ...")?;
    let root = Path::new(args.get(1).context("Missing NBA 2K27 game root")?);
    let catalog = Catalog::load(root)?;
    match mode.as_str() {
        "list" => {
            let pattern = regex::Regex::new(&format!("(?i){}", args.get(2).context("Missing regex")?))?;
            let limit: usize = args.get(3).map(|s| s.parse()).transpose()?.unwrap_or(50);
            let mut shown = 0;
            for &index in &catalog.sorted {
                let entry = &catalog.entries[index];
                if !pattern.is_match(&entry.name) {
                    continue;
                }
                println!("{}\t{}\t{}", entry.name, entry.archive, entry.size);
                shown += 1;
                if shown >= limit {
                    break;
                }
            }
            eprintln!("{} manifest entries, {shown} shown", catalog.entries.len());
        }
        "player" => {
            let id = args.get(2).context("Missing png id, e.g. 00110")?;
            ensure!(id.chars().all(|c| c.is_ascii_digit()), "The png id must be digits");
            let out = Path::new(args.get(3).context("Missing output folder")?);
            fs::create_dir_all(out)?;
            // The companions stage_player copies: the main IFF, its config/item/geo
            // companions and face#### texture IFFs. Body textures resolve later.
            let companion = regex::Regex::new(&format!(
                r"(?i)^char/(?:sig/)?(?:png{id}(?:_(?:config|item|geo)_.+)?|face{id}(?:_.+)?)\.iff$"
            ))?;
            let oodle = codec::Oodle::load(root)?;
            let cancel = AtomicBool::new(false);
            let mut count = 0;
            for entry in &catalog.entries {
                if !companion.is_match(&entry.name) {
                    continue;
                }
                let bytes = oodle
                    .archive(&codec::range(&catalog, entry)?, &cancel)
                    .with_context(|| format!("Decompressing {}", entry.name))?;
                let leaf = entry.name.rsplit('/').next().unwrap();
                fs::write(out.join(leaf), &bytes)?;
                println!("{}\t{} -> {} bytes", entry.name, entry.size, bytes.len());
                count += 1;
            }
            ensure!(count > 0, "No manifest entries for png{id}");
            let main = fs::read(out.join(format!("png{id}.iff")))?;
            if let Ok(info) = codec::Archive::open(main).and_then(|mut a| a.named("appearance_info.json")) {
                let appearance = &codec::tree(&info)?["appearance"];
                println!(
                    "appearance: gender={} skin={} iris={}",
                    appearance["gender"].as_str().unwrap_or("?"),
                    appearance["skin color"].as_str().unwrap_or("?"),
                    appearance["iris color"].as_str().unwrap_or("?")
                );
            }
        }
        "headshot" => {
            // The game's own headshot render (player_images/chr_g####.iff), as DDS:
            // ground truth for hair, beard and skin color.
            let id: u32 = args.get(2).context("Missing graphic id, e.g. 809")?.parse()?;
            let out = Path::new(args.get(3).context("Missing output .dds path")?);
            let name = format!("chr_g{id:04}");
            let source = catalog.resolve(&format!("player_images/{name}.iff"))?;
            let cancel = AtomicBool::new(false);
            let decoded = codec::Oodle::load(root)?.archive(&codec::range(&catalog, source)?, &cancel)?;
            let mut archive = codec::Archive::open(decoded)?;
            let tld = catalog
                .entries
                .iter()
                .find(|e| e.name.rsplit('/').next().is_some_and(|leaf| leaf.starts_with(&format!("{name}.")) && leaf.ends_with(".tld")))
                .with_context(|| format!("No texture for {name}"))?;
            let leaf = tld.name.rsplit('/').next().unwrap_or(&tld.name);
            let dds = nba2k_assets::textures::one(&mut archive, &catalog, &tld.name, &cancel)
                .or_else(|_| nba2k_assets::textures::one(&mut archive, &catalog, leaf, &cancel))?;
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(out, &dds)?;
            println!("{name} -> {} ({} bytes)", out.display(), dds.len());
        }
        "members" => {
            // Member names and sizes of any manifest IFF, e.g. char/male_skin.iff.
            let source = catalog.resolve(args.get(2).context("Missing manifest path")?)?;
            let pattern = regex::Regex::new(&format!("(?i){}", args.get(3).map_or(".", String::as_str)))?;
            let cancel = AtomicBool::new(false);
            let decoded = codec::Oodle::load(root)?.archive(&codec::range(&catalog, source)?, &cancel)?;
            for member in codec::Archive::open(decoded)?.names()? {
                let name = member["name"].as_str().unwrap_or("");
                if pattern.is_match(name) {
                    println!("{name}	{}", member["size"]);
                }
            }
        }
        "member" => {
            // One member of a manifest IFF, printed as JSON (TXTR, SCNE and other trees).
            let source = catalog.resolve(args.get(2).context("Missing manifest path")?)?;
            let cancel = AtomicBool::new(false);
            let decoded = codec::Oodle::load(root)?.archive(&codec::range(&catalog, source)?, &cancel)?;
            let bytes = codec::Archive::open(decoded)?.named(args.get(3).context("Missing member name")?)?;
            println!("{}", serde_json::to_string_pretty(&codec::tree(&bytes)?)?);
        }
        "buffer" => {
            // A scene vertex/index buffer member, decompressed to the SCNE Size.
            let source = catalog.resolve(args.get(2).context("Missing manifest path")?)?;
            let size: usize = args.get(4).context("Missing SCNE Size")?.parse()?;
            let out = Path::new(args.get(5).context("Missing output path")?);
            let cancel = AtomicBool::new(false);
            let oodle = codec::Oodle::load(root)?;
            let decoded = oodle.archive(&codec::range(&catalog, source)?, &cancel)?;
            let name = args.get(3).context("Missing member name")?;
            let bytes = codec::Archive::open(decoded)?
                .named(name)
                .or_else(|_| codec::range(&catalog, catalog.resolve(name)?))?;
            let bytes = if bytes.starts_with(&[0x1f, 0x8b, 8]) {
                let mut out = Vec::new();
                std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&bytes[..]), &mut out)?;
                out
            } else if codec::vcz(&bytes) { oodle.buffer(&bytes, size)? } else { bytes };
            ensure!(bytes.len() == size, "Buffer is {} bytes, SCNE Size {size}", bytes.len());
            fs::write(out, &bytes)?;
            println!("{} bytes -> {}", bytes.len(), out.display());
        }
        "dump" => {
            // Every member of a manifest IFF whose name matches a regex, raw, into a folder.
            let source = catalog.resolve(args.get(2).context("Missing manifest path")?)?;
            let pattern = regex::Regex::new(&format!("(?i){}", args.get(3).context("Missing regex")?))?;
            let out = Path::new(args.get(4).context("Missing output folder")?);
            fs::create_dir_all(out)?;
            let cancel = AtomicBool::new(false);
            let decoded = codec::Oodle::load(root)?.archive(&codec::range(&catalog, source)?, &cancel)?;
            let mut archive = codec::Archive::open(decoded)?;
            for member in archive.names()? {
                let name = member["name"].as_str().unwrap_or("").to_owned();
                if pattern.is_match(&name) {
                    fs::write(out.join(&name), archive.named(&name)?)?;
                }
            }
        }
        other => bail!("Unknown mode {other}; use list, player, headshot, members, member, buffer or dump"),
    }
    Ok(())
}
