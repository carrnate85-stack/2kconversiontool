//! Development-only adapter to the real launcher update transaction engine.
#![allow(dead_code)]
#[path = "../launcher_update.rs"] mod launcher_update;
#[path = "../hub_shutdown.rs"] mod hub_shutdown;
mod launcher_health { pub fn now_iso()->String {crate::backend::unique_id()} }
include!("hub_worker_support.rs");

fn main(){
    let action=std::env::args().nth(1).unwrap_or_default();
    let result=launcher_update::dev_hub::run(&action);
    match result {Ok(v)=>println!("{v}"),Err(e)=>{println!("{}",serde_json::json!({"error":format!("{e:#}")}));std::process::exit(1)}}
}
