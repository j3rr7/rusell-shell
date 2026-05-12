use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::process::Command;

use remote_shell::{
    DEFAULT_SHARED_SECRET, PORT_FILE, PROTOCOL_VERSION, build_response, find_available_port,
    is_exit_command, protocol, save_port_to_file, validate_secret,
};

pub async fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    println!("{} Server starting", PROTOCOL_VERSION);

    let port: u16 = if !args.is_empty() {
        args[0]
            .parse::<u16>()
            .unwrap_or_else(|_| find_available_port().unwrap_or(0))
    } else {
        find_available_port().unwrap_or(0)
    };

    if port == 0 {
        eprintln!("Failed to find available port");
        std::process::exit(1);
    }

    let bind_addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&bind_addr).await?;

    save_port_to_file(port, PORT_FILE)?;

    println!("{} Server started", PROTOCOL_VERSION);
    println!("Listening on: {}", bind_addr);
    println!("Port saved to: {}", PORT_FILE);
    println!(
        "Secret key: {} (change in source code)",
        DEFAULT_SHARED_SECRET
    );
    println!("Waiting for connections...\n");

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("[+] Connection from: {}", addr);

        tokio::spawn(async move {
            if let Err(e) = handle_client(socket, addr).await {
                eprintln!("[-] Error handling client {}: {}", addr, e);
            }
            println!("[-] Connection closed: {}", addr);
        });
    }
}

async fn handle_client(
    mut socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut buffer = [0u8; 4096];

    // Send greeting
    socket.write_all(protocol::GREETING.as_bytes()).await?;
    socket.flush().await?;

    // Read auth attempt
    let n = socket.read(&mut buffer).await?;
    let auth_attempt = String::from_utf8_lossy(&buffer[..n]).trim().to_string();

    if !validate_secret(&auth_attempt, DEFAULT_SHARED_SECRET) {
        eprintln!("[-] Auth failed from: {}", addr);
        socket.write_all(protocol::AUTH_FAIL.as_bytes()).await?;
        socket.flush().await?;
        return Err("Authentication failed".into());
    }

    println!("[✓] Authenticated: {}", addr);
    socket.write_all(protocol::AUTH_OK.as_bytes()).await?;
    socket.flush().await?;

    // Command loop
    loop {
        let n = socket.read(&mut buffer).await?;

        if n == 0 {
            break;
        }

        let command = String::from_utf8_lossy(&buffer[..n]).trim().to_string();

        if is_exit_command(&command) {
            socket.write_all(protocol::GOODBYE.as_bytes()).await?;
            socket.flush().await?;
            break;
        }

        if command.is_empty() {
            continue;
        }

        println!("[>] Executing: {}", command);

        let output = if cfg!(target_os = "windows") {
            Command::new("cmd")
                .args(["/C", &command])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
        } else {
            Command::new("sh")
                .args(["-c", &command])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await
        };

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let exit_code = out.status.code().unwrap_or(-1);

                let response = build_response(&stdout, &stderr, exit_code);
                socket.write_all(response.as_bytes()).await?;
                socket.write_all(&[protocol::END_MARKER]).await?;
                socket.flush().await?;
            }
            Err(e) => {
                let err_msg = format!("Error executing command: {}\n", e);
                socket.write_all(err_msg.as_bytes()).await?;
                socket.write_all(&[protocol::END_MARKER]).await?;
                socket.flush().await?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    async fn start_test_server(port: u16) -> TcpListener {
        let addr = format!("127.0.0.1:{}", port);
        TcpListener::bind(&addr).await.unwrap()
    }

    #[tokio::test]
    async fn test_server_sends_greeting() {
        let port = 19999;
        let listener = start_test_server(port).await;

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            handle_client(socket, "127.0.0.1:19999".parse().unwrap())
                .await
                .ok();
        });

        let mut client = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();
        let mut buf = [0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();

        let greeting = String::from_utf8_lossy(&buf[..n]);
        assert!(greeting.contains("AUTH_REQUIRED"));
    }

    #[tokio::test]
    async fn test_server_rejects_wrong_secret() {
        let port = 19998;
        let listener = start_test_server(port).await;

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            handle_client(socket, "127.0.0.1:19998".parse().unwrap())
                .await
                .ok();
        });

        let mut client = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();

        // Read greeting
        let mut buf = [0u8; 1024];
        client.read(&mut buf).await.unwrap();

        // Send wrong secret
        client.write_all(b"wrong_secret").await.unwrap();
        client.flush().await.unwrap();

        // Read response
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);
        assert!(response.contains("AUTH_FAIL"));
    }

    #[tokio::test]
    async fn test_server_accepts_correct_secret() {
        let port = 19997;
        let listener = start_test_server(port).await;

        tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            handle_client(socket, "127.0.0.1:19997".parse().unwrap())
                .await
                .ok();
        });

        let mut client = TcpStream::connect(format!("127.0.0.1:{}", port))
            .await
            .unwrap();

        // Read greeting
        let mut buf = [0u8; 1024];
        client.read(&mut buf).await.unwrap();

        // Send correct secret
        client
            .write_all(DEFAULT_SHARED_SECRET.as_bytes())
            .await
            .unwrap();
        client.flush().await.unwrap();

        // Read response
        let n = client.read(&mut buf).await.unwrap();
        let response = String::from_utf8_lossy(&buf[..n]);
        assert!(response.contains("AUTH_OK"));
    }
}
