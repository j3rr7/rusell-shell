use std::env;
use std::process;

use remote_shell::PROTOCOL_VERSION;

mod client;
mod server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    match args[1].to_lowercase().as_str() {
        "server" | "s" => {
            // Pass remaining args to server (port is optional)
            let server_args: Vec<String> = args.iter().skip(2).cloned().collect();
            server::run(server_args).await
        }
        "client" | "c" => {
            // Pass remaining args to client (server_ip required, port optional)
            let client_args: Vec<String> = args.iter().skip(2).cloned().collect();
            client::run(client_args).await
        }
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        "version" | "-v" | "--version" => {
            println!("{}", PROTOCOL_VERSION);
            Ok(())
        }
        other => {
            eprintln!("Unknown command: '{}'", other);
            eprintln!();
            print_usage();
            process::exit(1);
        }
    }
}

fn print_usage() {
    println!("{}", PROTOCOL_VERSION);
    println!();
    println!("USAGE:");
    println!("    remote-shell <COMMAND> [OPTIONS]");
    println!();
    println!("COMMANDS:");
    println!("    server, s    Start the remote shell server");
    println!("    client, c    Connect to a remote shell server");
    println!("    help         Show this help message");
    println!("    version      Show version information");
    println!();
    println!("SERVER OPTIONS:");
    println!("    [PORT]       Port to listen on (default: random in 49152-65535)");
    println!();
    println!("CLIENT OPTIONS:");
    println!("    <SERVER_IP>  IP address or hostname of the server");
    println!(
        "    [PORT]       Port to connect to (default: read from {})",
        remote_shell::PORT_FILE
    );
    println!();
    println!("EXAMPLES:");
    println!("    remote-shell server");
    println!("    remote-shell server 8080");
    println!("    remote-shell client 192.168.1.100");
    println!("    remote-shell client 192.168.1.100 54321");
    println!();
    println!("NOTES:");
    println!(
        "    - Server writes its port to '{}' for client discovery",
        remote_shell::PORT_FILE
    );
    println!("    - Shared secret is defined in lib.rs (change before production use!)");
}
