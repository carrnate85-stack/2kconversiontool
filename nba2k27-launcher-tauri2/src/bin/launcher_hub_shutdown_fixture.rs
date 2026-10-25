//! Harmless fixture exercising the same protocol; no GUI/game/session services.
#![allow(dead_code)]
include!("hub_worker_support.rs");
#[path="../hub_shutdown.rs"] mod hub_shutdown;
fn main(){
 let mode=std::env::args().nth(1).unwrap_or_else(||"clean".into());
 let deadline=std::time::Instant::now()+std::time::Duration::from_secs(35);
 while std::time::Instant::now()<deadline {
  if let Ok(Some(r))=hub_shutdown::poll(include_str!("../../../2k-tools-hub/dev-update-config/installed-worker.sha256")) {
   match mode.as_str(){
    "busy"=>{hub_shutdown::finish(&r,"busy").unwrap();},
    "hang"=>{},
    "exit-without-ack"=>return,
    "ack-alive"=>{hub_shutdown::finish(&r,"cleaned").unwrap();std::thread::sleep(std::time::Duration::from_secs(30));return},
    _=>{backend::atomic_json(&hub_shutdown::root().join("fixture-cleanup.json"),&serde_json::json!({"cleanupCompleted":true})).unwrap();hub_shutdown::finish(&r,"cleaned").unwrap();return}
   }
  }
  std::thread::sleep(std::time::Duration::from_millis(50));
 }
}
