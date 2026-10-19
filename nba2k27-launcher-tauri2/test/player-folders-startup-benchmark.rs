// Included into the existing isolated selector filesystem harness by the
// engineering build script. Every write is a uniquely named fixture. The
// process callback measures real read-only production process_scan work; it
// does not authorize mutations of any installed profile or game.
mod provider_worker {
    pub(crate) fn owned(_: &crate::process_guard::Identity) -> bool {
        false
    }
}
#[cfg(test)]
mod player_folder_startup_benchmark {
    use super::*;
    use std::{cell::Cell, fs, time::Instant};
    #[test]
    fn benchmark_361_real_process_scans_and_fixture_links() {
        let base = std::env::temp_dir().join(format!("player-perf-{}", backend::unique_id()));
        fs::create_dir_all(base.join("portable/Data")).unwrap();
        let portable = base.join("portable");
        let mods = base.join("mods");
        for i in 0..361 {
            let dir = mods.join(format!("players/Player{i:04}"));
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(format!("png{i:04}.iff")), b"fixture").unwrap();
        }
        let scans = Cell::new(0usize);
        let scan_time = Cell::new(std::time::Duration::ZERO);
        let scan = || {
            let begin = Instant::now();
            let rows = process_guard::process_scan()?;
            scans.set(scans.get() + 1);
            scan_time.set(scan_time.get() + begin.elapsed());
            std::hint::black_box(rows);
            Ok(())
        };
        let begin = Instant::now();
        let mut s = player_folders::Store::create(&portable, &mods, &backend::unique_id(), scan)
            .unwrap()
            .unwrap();
        let create_elapsed = begin.elapsed();
        let create_scans = scans.get();
        s.activate(scan).unwrap();
        let total = begin.elapsed();
        let activation_scans = scans.get();
        s.verify_routes().unwrap();
        let restore_begin = Instant::now();
        s.restore(scan).unwrap();
        let restore_elapsed = restore_begin.elapsed();
        let restore_scans = scans.get() - activation_scans;
        println!(
            "PLAYER_PERF files=361 create_scans={create_scans} activation_total_scans={activation_scans} restore_scans={restore_scans} create_ms={} activation_total_ms={} restore_ms={} process_scan_ms={}",
            create_elapsed.as_millis(),
            total.as_millis(),
            restore_elapsed.as_millis(),
            scan_time.get().as_millis()
        );
        drop(s);
        assert!(!portable.join("Data").join(player_folders::MARKER).exists());
        assert_eq!(fs::read_dir(mods.join("char/sig")).unwrap().count(), 0);
        assert!(
            base.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("player-perf-")
        );
        fs::remove_dir_all(base).unwrap();
    }
}
