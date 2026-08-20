mod herdr;

fn main() {
    match herdr::list_agents() {
        Ok(agents) => {
            println!("agents: {}", agents.len());
            for agent in &agents {
                println!(
                    "  {} ws={} agent={:?} tokens={}",
                    agent.pane_id,
                    agent.workspace_id,
                    agent.agent,
                    agent.tokens.len()
                );
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
