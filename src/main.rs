use std::io::IsTerminal;
use std::process::ExitCode;

use herdr_tags::cmd;
use herdr_tags::model::Mode;

fn usage() -> &'static str {
    concat!(
        "usage:\n",
        "  herdr-tags add <tag> [pane]\n",
        "  herdr-tags rm <tag> [pane]\n",
        "  herdr-tags ls\n",
        "  herdr-tags delete <tag>\n",
        "  herdr-tags filter <tag> <in|out|off>\n",
        "  herdr-tags filter-clear\n",
        "  herdr-tags sync | clear | gc | paths | ui [--dock]"
    )
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A bare invocation is the popup entrypoint, so it opens the TUI -- but only
    // where there is a terminal to draw on. Piped or captured, ratatui's init
    // panics with an OS error, so usage is the useful answer instead.
    let command = match args.first().map(String::as_str) {
        Some(c) => c,
        None if std::io::stdout().is_terminal() => "ui",
        None => "help",
    };
    let arg = |n: usize| args.get(n).map(String::as_str);

    let result = match command {
        "add" => match arg(1) {
            Some(tag) => cmd::add(tag, arg(2)),
            None => Err(usage().to_string()),
        },
        "rm" => match arg(1) {
            Some(tag) => cmd::remove(tag, arg(2)),
            None => Err(usage().to_string()),
        },
        "ls" => cmd::list(),
        "delete" => match arg(1) {
            Some(tag) => cmd::delete(tag),
            None => Err(usage().to_string()),
        },
        "filter" => match (arg(1), arg(2)) {
            (Some(tag), Some("in")) => cmd::filter(tag, Mode::In),
            (Some(tag), Some("out")) => cmd::filter(tag, Mode::Out),
            (Some(tag), Some("off")) => cmd::filter(tag, Mode::Off),
            _ => Err(usage().to_string()),
        },
        "filter-clear" => cmd::filter_clear(),
        "sync" => cmd::sync(),
        "clear" => cmd::clear(),
        "gc" => cmd::gc(),
        "paths" => cmd::paths(),
        "open-popup" => cmd::open_popup(),
        "ui" => herdr_tags::ui::run(args.iter().any(|a| a == "--dock")).map(|()| String::new()),
        "help" | "--help" | "-h" => Ok(usage().to_string()),
        other => Err(format!("unknown command {other}\n{}", usage())),
    };

    // The single place a `cmd::` message reaches stdout.
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
