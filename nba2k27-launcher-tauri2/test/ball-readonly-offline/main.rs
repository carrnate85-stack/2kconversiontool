fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let result = (|| -> anyhow::Result<serde_json::Value> {
        anyhow::ensure!(
            args.len() == 2,
            "Usage: ball-v2-readonly-probe <portable launcher folder> <exact game PID>; requires fresh human approval before invocation"
        );
        let pid = args[1]
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid PID"))?
            .parse()?;
        ball_readonly_offline::run(std::path::Path::new(&args[0]), pid)
    })();
    match result {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
        Err(e) => {
            println!(
                "{}",
                serde_json::json!({"readOnly":true,"writes":0,"error":format!("{e:#}")})
            );
            std::process::exit(1);
        }
    }
}
