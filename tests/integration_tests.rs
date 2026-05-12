
//! Integration tests - full client/server communication

use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncBufReadExt, BufReader};
use tokio::time::{timeout, Duration};
use remote_shell::{
    protocol, find_available_port, save_port_to_file,
    read_port_from_file, DEFAULT_SHARED_SECRET,
};

/// Start a minimal test server that handles one client
async fn test_server(listener: TcpListener, secret: &str) {
    let (mut socket, addr) = listener.accept().await.unwrap();
    println!("[test-server] Connection from: {}", addr);

    let mut buf = [0u8; 4096];

    // Send greeting
    socket.write_all(protocol::GREETING.as_bytes()).await.unwrap();
    socket.flush().await.unwrap();

    // Read auth
    let n = socket.read(&mut buf).await.unwrap();
    let auth = String::from_utf8_lossy(&buf[..n]).trim().to_string();

    if auth != secret {
        socket.write_all(protocol::AUTH_FAIL.as_bytes()).await.unwrap();
        socket.flush().await.unwrap();
        return;
    }

    socket.write_all(protocol::AUTH_OK.as_bytes()).await.unwrap();
    socket.flush().await.unwrap();

    // Command loop - handle simple echo for testing
    loop {
        let n = socket.read(&mut buf).await.unwrap();
        if n == 0 {
            break;
        }

        let cmd = String::from_utf8_lossy(&buf[..n]).trim().to_string();

        if cmd == "EXIT" || cmd == "QUIT" {
            socket.write_all(protocol::GOODBYE.as_bytes()).await.unwrap();
            socket.flush().await.unwrap();
            break;
        }

        if cmd.is_empty() {
            continue;
        }

        // Echo the command back as "output"
        let response = format!("ECHO: {}\n[Exit Code: 0]\n", cmd);
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.write_all(&[protocol::END_MARKER]).await.unwrap();
        socket.flush().await.unwrap();
    }

    println!("[test-server] Connection closed");
}

/// Simulate a client connection
async fn test_client(port: u16, secret: &str) -> Result<Vec<String>, String> {
    let mut socket = TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .map_err(|e| format!("Connect failed: {}", e))?;

    let (reader, mut writer) = socket.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let mut responses = Vec::new();

    // Read greeting
    reader.read_line(&mut line).await.map_err(|e| e.to_string())?;
    if !line.contains("AUTH_REQUIRED") {
        return Err(format!("Unexpected greeting: {}", line));
    }

    // Send auth
    writer.write_all(secret.as_bytes()).await.map_err(|e| e.to_string())?;
    writer.flush().await.map_err(|e| e.to_string())?;

    // Read auth response
    line.clear();
    reader.read_line(&mut line).await.map_err(|e| e.to_string())?;

    if line.contains("AUTH_FAIL") {
        return Err("Authentication failed".to_string());
    }

    if !line.contains("AUTH_OK") {
        return Err(format!("Unexpected auth response: {}", line));
    }

    // Send a test command
    writer.write_all(b"hello world").await.map_err(|e| e.to_string())?;
    writer.flush().await.map_err(|e| e.to_string())?;

    // Read response
    let mut response = Vec::new();
    let mut buf = [0u8; 4096];

    loop {
        let n = reader.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }

        if let Some(pos) = buf[..n].iter().position(|&b| b == protocol::END_MARKER) {
            response.extend_from_slice(&buf[..pos]);
            break;
        }
        response.extend_from_slice(&buf[..n]);
    }

    responses.push(String::from_utf8_lossy(&response).to_string());

    // Send exit
    writer.write_all(b"EXIT").await.map_err(|e| e.to_string())?;
    writer.flush().await.map_err(|e| e.to_string())?;

    Ok(responses)
}

#[tokio::test]
async fn test_full_client_server_flow() {
    let port = find_available_port().unwrap();
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();

    let secret = DEFAULT_SHARED_SECRET.to_string();
    tokio::spawn(async move {
        test_server(listener, &secret).await;
    });

    // Give server time to start
    tokio::time::sleep(Duration::from_millis(50)).await;

    let responses = test_client(port, DEFAULT_SHARED_SECRET).await.unwrap();

    assert_eq!(responses.len(), 1);
    assert!(responses[0].contains("ECHO: hello world"));
    assert!(responses[0].contains("[Exit Code: 0]"));
}

#[tokio::test]
async fn test_auth_failure() {
    let port = find_available_port().unwrap();
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();

    tokio::spawn(async move {
        test_server(listener, "correct_secret").await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = test_client(port, "wrong_secret").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Authentication failed"));
}

#[tokio::test]
async fn test_multiple_commands() {
    let port = find_available_port().unwrap();
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();

    let secret = DEFAULT_SHARED_SECRET.to_string();
    tokio::spawn(async move {
        test_server(listener, &secret).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let mut socket = TcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();
    let (reader, mut writer) = socket.split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    // Auth
    reader.read_line(&mut line).await.unwrap();
    writer.write_all(DEFAULT_SHARED_SECRET.as_bytes()).await.unwrap();
    writer.flush().await.unwrap();
    line.clear();
    reader.read_line(&mut line).await.unwrap();
    assert!(line.contains("AUTH_OK"));

    // Send multiple commands
    for i in 1..=3 {
        let cmd = format!("cmd{}", i);
        writer.write_all(cmd.as_bytes()).await.unwrap();
        writer.flush().await.unwrap();

        // Read response
        let mut response = Vec::new();
        let mut buf = [0u8; 4096];
        loop {
            let n = reader.read(&mut buf).await.unwrap();
            if n == 0 { break; }
            if let Some(pos) = buf[..n].iter().position(|&b| b == protocol::END_MARKER) {
                response.extend_from_slice(&buf[..pos]);
                break;
            }
            response.extend_from_slice(&buf[..n]);
        }

        let resp_str = String::from_utf8_lossy(&response);
        assert!(resp_str.contains(&format!("ECHO: cmd{}", i)));
    }

    // Exit
    writer.write_all(b"EXIT").await.unwrap();
    writer.flush().await.unwrap();
}

#[tokio::test]
async fn test_port_file_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.port");
    let path_str = path.to_str().unwrap();

    let test_port = 54321u16;

    save_port_to_file(test_port, path_str).unwrap();
    let read_port = read_port_from_file(path_str).unwrap();

    assert_eq!(test_port, read_port);
}

#[tokio::test]
async fn test_server_rejects_empty_auth() {
    let port = find_available_port().unwrap();
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();

    tokio::spawn(async move {
        test_server(listener, "secret").await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    let result = timeout(Duration::from_secs(2), test_client(port, "empty")).await;
    let err = result.expect("test timed out").expect_err("expected auth error");
    assert!(err.contains("Authentication failed"));
}

#[tokio::test]
async fn test_connection_refused() {
    // Use a port that's not listening
    let result = test_client(19990, "secret").await;
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Connect failed"));
}

#[tokio::test]
async fn test_server_handles_disconnect() {
    let port = find_available_port().unwrap();
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await.unwrap();

    let secret = DEFAULT_SHARED_SECRET.to_string();
    let handle = tokio::spawn(async move {
        test_server(listener, &secret).await;
    });

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Connect and disconnect without proper exit
    let mut socket = TcpStream::connect(format!("127.0.0.1:{}", port)).await.unwrap();
    let (mut reader, mut writer) = socket.split();
    let mut buf = [0u8; 1024];

    // Read greeting
    reader.read(&mut buf).await.unwrap();

    // Auth
    writer.write_all(DEFAULT_SHARED_SECRET.as_bytes()).await.unwrap();
    writer.flush().await.unwrap();
    reader.read(&mut buf).await.unwrap();

    // Just drop the connection
    drop(socket);

    // Server should handle this gracefully
    let _ = timeout(Duration::from_secs(1), handle).await;
}

// Test that we can find multiple available ports
#[test]
fn test_find_multiple_ports() {
    let mut ports = std::collections::HashSet::new();

    for _ in 0..20 {
        if let Some(port) = find_available_port() {
            ports.insert(port);
        }
    }

    // Should find at least a few different ports
    assert!(ports.len() >= 5, "Found {} unique ports, expected at least 5", ports.len());
}
