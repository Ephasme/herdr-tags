use std::process::ExitCode;

use herdr_tags::cmd;

fn usage() -> &'static str {
    "usage: herdr-tags <sync|paths|ui>"
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("ui");

    let result = match command {
        "sync" => cmd::sync(),
        "paths" => cmd::paths(),
        "ui" => Err("ui is implemented in Task 7".to_string()),
        other => Err(format!("unknown command {other}\n{}", usage())),
    };

    // The single place a cmd:: message reaches stdout.
    match result {
        Ok(message) => {
            if !message.is_empty() {
                println!("{message}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("tags: {e}");
            ExitCode::FAILURE
        }
    }
}
