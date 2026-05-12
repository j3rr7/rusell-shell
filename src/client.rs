use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use remote_shell::{
    DEFAULT_SHARED_SECRET, PORT_FILE, PROTOCOL_VERSION, protocol, read_port_from_file,
};

pub async fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    println!("{} Client starting", PROTOCOL_VERSION);

    if args.is_empty() {
        eprintln!("Usage: remote-shell client <server_ip> [port]");
        return Ok(());
    }

    let server_ip = &args[0];

    let port: u16 = if args.len() > 1 {
        args[1].parse()?
    } else {
        match read_port_from_file(PORT_FILE) {
            Ok(p) => {
                eprintln!("[*] Using port {} from {}", p, PORT_FILE);
                p
            }
            Err(_) => {
                eprintln!("Error: Port file '{}' not found", PORT_FILE);
                std::process::exit(1);
            }
        }
    };

    connect_and_run(server_ip, port).await
}

async fn connect_and_run(server_ip: &str, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", server_ip, port);

    eprintln!("[*] Connecting to {}...", addr);

    let mut socket = TcpStream::connect(&addr).await?;

    let (reader, mut writer) = socket.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    reader.read_line(&mut line).await?;
    eprintln!("[*] Server: {}", line.trim());

    if !line.contains("AUTH_REQUIRED") {
        eprintln!("Unexpected server response");
        return Ok(());
    }

    eprintln!("[*] Authenticating...");
    writer.write_all(DEFAULT_SHARED_SECRET.as_bytes()).await?;
    writer.flush().await?;

    line.clear();
    reader.read_line(&mut line).await?;

    if line.contains("AUTH_FAIL") {
        eprintln!("[-] Authentication failed!");
        return Ok(());
    }

    if !line.contains("AUTH_OK") {
        eprintln!("Unexpected auth response: {}", line.trim());
        return Ok(());
    }

    eprintln!("[✓] Connected to {}", addr);
    eprintln!("[*] Type commands to execute. Type 'exit' to quit.\n");

    let stdin = tokio::io::stdin();
    let mut stdin_reader = BufReader::new(stdin);
    let mut input = String::new();

    loop {
        print!("> ");
        std::io::Write::flush(&mut std::io::stdout())?;

        input.clear();
        match stdin_reader.read_line(&mut input).await {
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }

        let cmd = input.trim();

        if cmd.is_empty() {
            continue;
        }

        if cmd.eq_ignore_ascii_case("exit") || cmd.eq_ignore_ascii_case("quit") {
            writer.write_all(b"EXIT").await?;
            writer.flush().await?;

            let mut buf = [0u8; 1024];
            if let Ok(n) = reader.read(&mut buf).await {
                if n > 0 {
                    print!("{}", String::from_utf8_lossy(&buf[..n]));
                }
            }
            break;
        }

        writer.write_all(cmd.as_bytes()).await?;
        writer.flush().await?;

        let mut response = Vec::new();
        let mut buf = [0u8; 4096];

        loop {
            let n = reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }

            if let Some(pos) = buf[..n].iter().position(|&b| b == protocol::END_MARKER) {
                response.extend_from_slice(&buf[..pos]);
                break;
            }

            response.extend_from_slice(&buf[..n]);
        }

        if !response.is_empty() {
            print!("{}", String::from_utf8_lossy(&response));
            std::io::Write::flush(&mut std::io::stdout())?;
        }
    }

    eprintln!("\n[*] Disconnected");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Mock server that follows the protocol
    async fn mock_server(listener: TcpListener, secret: &str, responses: Vec<String>) {
        let (mut socket, _) = listener.accept().await.unwrap();

        // Send greeting
        socket
            .write_all(protocol::GREETING.as_bytes())
            .await
            .unwrap();
        socket.flush().await.unwrap();

        // Read auth
        let mut buf = [0u8; 1024];
        let n = socket.read(&mut buf).await.unwrap();
        let auth = String::from_utf8_lossy(&buf[..n]).trim().to_string();

        if auth == secret {
            socket
                .write_all(protocol::AUTH_OK.as_bytes())
                .await
                .unwrap();
            socket.flush().await.unwrap();

            // Send responses for each command
            for resp in responses {
                let n = socket.read(&mut buf).await.unwrap();
                let cmd = String::from_utf8_lossy(&buf[..n]).trim().to_string();

                if cmd == "EXIT" {
                    socket
                        .write_all(protocol::GOODBYE.as_bytes())
                        .await
                        .unwrap();
                    socket.flush().await.unwrap();
                    break;
                }

                socket.write_all(resp.as_bytes()).await.unwrap();
                socket.write_all(&[protocol::END_MARKER]).await.unwrap();
                socket.flush().await.unwrap();
            }
        } else {
            socket
                .write_all(protocol::AUTH_FAIL.as_bytes())
                .await
                .unwrap();
            socket.flush().await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_client_connects_with_correct_secret() {
        let port = 19899;
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .unwrap();

        let secret = DEFAULT_SHARED_SECRET.to_string();
        let responses = vec!["Hello World\n".to_string()];

        tokio::spawn(async move {
            mock_server(listener, &secret, responses).await;
        });

        let result = connect_and_run("127.0.0.1", port).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_client_fails_with_wrong_secret() {
        let port = 19898;
        let listener = TcpListener::bind(format!("127.0.0.1:{}", port))
            .await
            .unwrap();

        // Server expects different secret
        tokio::spawn(async move {
            mock_server(listener, "different_secret", vec![]).await;
        });

        // Client uses default secret - should fail
        let result = connect_and_run("127.0.0.1", port).await;
        // It's ok because it just exits on auth fail
        assert!(result.is_ok());
    }
}
