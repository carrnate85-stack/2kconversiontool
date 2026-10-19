#[path = "../../../../src/jukebox_audio.rs"]
mod jukebox_audio;
#[path = "../../../../src/jukebox_catalog.rs"]
mod jukebox_catalog;
#[path = "../../../../src/jukebox_decode_worker.rs"]
mod jukebox_decode_worker;
#[path = "../../../../src/jukebox_decoder.rs"]
mod jukebox_decoder;
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args.len() == 3 && args[1] == "--jukebox-decode-worker" && args[2] == "d".repeat(48) {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    let result = if args.len() == 3 && args[1] == "--jukebox-decode-worker" {
        jukebox_decode_worker::run(&args[2])
    } else {
        Err(anyhow::anyhow!("Private fixture worker mode required"))
    };
    if let Err(error) = result {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
