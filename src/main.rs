fn main() {
    match herdr_tags::herdr::list_agents() {
        Ok(agents) => println!("agents: {}", agents.len()),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
