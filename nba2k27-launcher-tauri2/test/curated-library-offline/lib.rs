#![allow(dead_code)]
extern crate self as tauri;
pub use url::Url;
#[path="../../src/preview_download.rs"]mod preview_download;
#[path="../../src/previews.rs"]mod previews;
#[path="../../src/curated_catalog.rs"]mod curated_catalog;
#[path="../../src/curated_http.rs"]mod curated_http;
#[path="../../src/curated_metadata.rs"]mod curated_metadata;
#[path="../../src/curated_library.rs"]mod curated_library;
#[path="../../src/curated_publisher.rs"]mod curated_publisher;
#[path="../../src/startup_mp4.rs"]mod startup_mp4;
mod backend {
 pub fn unique_id()->String{format!("{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos())}
 pub fn atomic_json(path:&std::path::Path,v:&impl serde::Serialize)->anyhow::Result<()>{use std::io::Write;let mut f=std::fs::File::create(path)?;f.write_all(&serde_json::to_vec(v)?)?;f.sync_all()?;Ok(())}
}
#[test]
fn replacement_download_survives_removed_original_and_rejects_wrong_type() {
 let mut c=curated_metadata::bundled().unwrap();
 let id=c.items[0].id.clone();
 let d=curated_catalog::Download{url:"https://www.mediafire.com/file_premium/abcdefghijklmno/intro.mp4/file".into(),filename:"intro.mp4".into(),bytes:120,sha256:"a".repeat(64)};
 c.items[0].download=Some(d.clone());
 let mut rows=vec![];curated_metadata::apply(&mut rows,&c,None).unwrap();
 assert_eq!(rows.len(),1);assert_eq!(rows[0].id,id);assert_eq!(rows[0].url,d.url);assert_eq!(rows[0].sha256,d.sha256);
 let mut wrong=d.clone();wrong.filename="intro.exe".into();assert!(curated_catalog::validate_download(&id,&wrong).is_err());
 for link in ["https://evil.test/file/abcdefghijklmno/x/file","https://www.mediafire.com/folder/abcdefghijklmno","https://www.mediafire.com/file/abcdefghijklmno/x/file?redirect=1"] {assert!(curated_catalog::link_key(link).is_err());}
 wrong=d;wrong.bytes=curated_catalog::MAX_FILE+1;assert!(curated_catalog::validate_download(&id,&wrong).is_err());
}
#[test]#[ignore="Explicit public preview download to QA133 artifacts only; no library import"]
fn live_qa133_preview() {
 let cancel=std::sync::atomic::AtomicBool::new(false);
 let d=curated_catalog::resolve_download(curated_catalog::PREVIEW_ID,"https://www.mediafire.com/file_premium/tjl35b7c57a5yvt/blacktoppreviewpack.zip/file",&cancel).unwrap();
 assert_eq!(d.sha256,"75b3cc03e2b7bda4f59e41f156706ad96d69dd780893aceb7866ce87063a9e55");assert_eq!(d.bytes,614685202);
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/qa133-download-autoload");
 let target=root.join("verified-preview.zip");
 preview_download::download_from(&target,&d.url,d.bytes,&d.sha256,&cancel,|_,_|{}).unwrap();
 backend::atomic_json(&root.join("preview-download.json"),&d).unwrap();
}
#[test]fn actual_folder_snapshots_and_artwork_catalog(){let mut all=vec![];for (kind,folder) in curated_catalog::FOLDERS{let file=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../artifacts/qa131-download-library/{kind}-listing.json"));let bytes=std::fs::read(file).unwrap();let s=std::str::from_utf8(&bytes).unwrap().trim_start_matches('\u{feff}');let (entries,more)=curated_catalog::page(&serde_json::from_str(s).unwrap(),kind,folder,1).unwrap();assert!(!more);assert!(entries.iter().all(|e|e.supported));all.extend(entries);}assert_eq!(all.len(),11);let before:Vec<_>=all.iter().map(|e|(e.id.clone(),e.url.clone(),e.sha256.clone())).collect();let mut c=curated_metadata::bundled().unwrap();c.items[0].title="Custom title".into();c.items[0].order=-100;curated_metadata::apply(&mut all,&c,Some(curated_metadata::URL)).unwrap();assert_eq!(all[0].name,"Custom title");for e in &all{assert!(e.artwork.starts_with("https://raw.githubusercontent.com/"));assert!(before.contains(&(e.id.clone(),e.url.clone(),e.sha256.clone())));}c.items[0].hidden=true;curated_metadata::apply(&mut all,&c,None).unwrap();assert_eq!(all.len(),10);}
#[test]fn shutdown_prevents_refresh_and_install(){let s=curated_library::Service::default();s.shutdown();assert!(s.refresh().is_err());assert!(s.install(std::path::Path::new("C:\\unused"),"x",|_,_,_,_|panic!("must not import")).is_err());}
#[test]#[ignore="Explicit GitHub read only; never publishes"]fn live_curator_read(){let v=curated_publisher::editor().unwrap();assert_eq!(v["catalog"]["schema"],1);assert!(v["catalog"]["items"].as_array().unwrap().len()>=11);}
#[test]#[ignore="Explicit public MediaFire read and isolated download only"]fn live_video_download(){
 let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../artifacts/qa131-download-library/live-video").canonicalize().unwrap();
 let s=curated_library::Service::default();s.refresh().unwrap();let v=s.snapshot(&root).unwrap();let e=v["items"].as_array().unwrap().iter().find(|e|e["name"]=="NBA 2K12"&&e["kind"]=="videos").unwrap();
 s.install(&root,e["id"].as_str().unwrap(),|_,path,_,_|{let mut f=std::fs::File::open(path)?;let m=startup_mp4::inspect(&mut f,"mp4")?;assert!(m.duration_ms>1000);Ok("fixture-only-not-a-real-library-import".into())}).unwrap();
 std::fs::write(root.join("result.json"),serde_json::to_vec_pretty(&s.snapshot(&root).unwrap()).unwrap()).unwrap();
}
