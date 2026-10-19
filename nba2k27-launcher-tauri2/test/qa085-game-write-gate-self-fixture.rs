#![allow(dead_code)]
use anyhow::{Result, ensure};
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    time::Duration,
};
#[path = "../src/game_write_gate.rs"]
mod gate;
fn child(mode: &str) -> Result<()> {
    match mode {
        "try-busy" => ensure!(
            gate::try_lock()?.is_none(),
            "foreign process acquired held gate"
        ),
        "try-poison" => ensure!(gate::try_lock().is_err(), "abandonment poison not shared"),
        "lock" => {
            println!("waiting");
            std::io::stdout().flush()?;
            let _g = gate::lock()?;
            println!("acquired");
        }
        "abandon" => {
            let _g = gate::lock()?;
            println!("owned");
            std::io::stdout().flush()?;
            std::process::exit(0);
        }
        _ => anyhow::bail!("unknown fixture child"),
    }
    Ok(())
}
fn run() -> Result<()> {
    let exe = std::env::current_exe()?;
    let guard = gate::lock()?;
    ensure!(gate::try_lock()?.is_none(), "local try gate reentered");
    ensure!(
        Command::new(&exe).arg("try-busy").status()?.success(),
        "cross-process try failed"
    );
    let mut ch = Command::new(&exe)
        .arg("lock")
        .stdout(Stdio::piped())
        .spawn()?;
    let mut rd = BufReader::new(ch.stdout.take().unwrap());
    let mut line = String::new();
    rd.read_line(&mut line)?;
    ensure!(line.trim() == "waiting", "wait phase");
    std::thread::sleep(Duration::from_millis(150));
    ensure!(
        ch.try_wait()?.is_none(),
        "child escaped while parent owns gate"
    );
    drop(guard);
    line.clear();
    rd.read_line(&mut line)?;
    ensure!(
        line.trim() == "acquired",
        "child did not acquire after release"
    );
    ensure!(ch.wait()?.success(), "clean guard release");
    let guard = gate::lock()?;
    drop(guard);
    // Parent retains named object handles while abrupt child exit abandons ownership.
    let mut ch = Command::new(&exe)
        .arg("abandon")
        .stdout(Stdio::piped())
        .spawn()?;
    let mut rd = BufReader::new(ch.stdout.take().unwrap());
    line.clear();
    rd.read_line(&mut line)?;
    ensure!(line.trim() == "owned", "abandon ownership");
    ensure!(ch.wait()?.success(), "abandon child exit");
    ensure!(
        gate::lock().is_err(),
        "abandoned mutex must not grant forward authority"
    );
    ensure!(
        Command::new(&exe).arg("try-poison").status()?.success(),
        "abandonment was not globally latched"
    );
    println!(
        "PASS: local+2-process serialization, nonblocking try, clean release, abandoned ownership release, shared poison refusal"
    );
    Ok(())
}
fn main() {
    let result = match std::env::args().nth(1) {
        Some(m) => child(&m),
        None => run(),
    };
    if let Err(e) = result {
        eprintln!("{e:#}");
        std::process::exit(1)
    }
}
