//! Offline CNG fresh full-byte SHA256 comparison. No process/game access.
use anyhow::{Result,ensure};use sha2::{Digest,Sha256};use std::time::Instant;
use windows::Win32::Security::Cryptography::{BCryptHash,BCRYPT_SHA256_ALG_HANDLE};
fn cng(v:&[u8])->Result<String>{let mut out=[0u8;32];unsafe{BCryptHash(BCRYPT_SHA256_ALG_HANDLE,None,v,&mut out).ok()?;}Ok(out.iter().map(|v|format!("{v:02x}")).collect())}
fn main()->Result<()>{
 for (v,expected) in [(Vec::new(),"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"),(b"abc".to_vec(),"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"),(vec![b'a';1000000],"cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0")] {ensure!(cng(&v)?==expected,"known vector");}
 for h in (0..8u8).map(|worker|std::thread::spawn(move||->Result<()>{for i in 0..100u8{let mut v=vec![worker;257];v[0]=i;let expected=format!("{:x}",Sha256::digest(&v));ensure!(cng(&v)?==expected,"parallel fresh input");let nested=cng(expected.as_bytes())?;ensure!(nested==format!("{:x}",Sha256::digest(expected.as_bytes())),"nested");}Ok(())})){h.join().unwrap()?;}
 let mut results=Vec::new();for n in [1024*1024,16*1024*1024]{let mut v=vec![0xa7;n];let mut old=Vec::new();let mut new=Vec::new();for i in 0..8{v[0]=i;let t=Instant::now();let expected=format!("{:x}",Sha256::digest(&v));old.push(t.elapsed().as_secs_f64()*1000.);let t=Instant::now();let got=cng(&v)?;new.push(t.elapsed().as_secs_f64()*1000.);ensure!(got==expected,"same full fresh input");}results.push(serde_json::json!({"bytes":n,"oldRustShaMilliseconds":old,"freshCngMilliseconds":new}));}
 println!("{}",serde_json::to_string_pretty(&serde_json::json!({"knownVectors":3,"parallelFreshCases":800,"nestedCases":800,"mutableGlobalCryptoState":false,"results":results}))?);Ok(())
}
