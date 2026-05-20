//! `hackline whoami` — print the current cached identity + scope.

use crate::config;

pub fn run(json: bool) -> anyhow::Result<()> {
    match config::load_credentials()? {
        Some(creds) => {
            if json {
                let val = serde_json::json!({
                    "server": creds.server,
                    "user_id": creds.user_id,
                    "name": creds.name,
                });
                crate::output::print_json(&val);
            } else {
                println!("Server:  {}", creds.server);
                println!("User ID: {}", creds.user_id);
                println!("Name:    {}", creds.name);
            }
        }
        None => {
            println!("Not logged in. Run `hackline login` first.");
        }
    }
    Ok(())
}
