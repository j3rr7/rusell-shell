/// Protocol version string
pub const PROTOCOL_VERSION: &str = "RSHELLv1";

/// File to store the port number
pub const PORT_FILE: &str = ".remote-shell.port";

/// Default shared secret
pub const DEFAULT_SHARED_SECRET: &str = "my-super-secret-key-change-me";

/// Port range for random selection
pub const PORT_RANGE_START: u16 = 49152;
pub const PORT_RANGE_END: u16 = 65535;
pub const MAX_PORT_ATTEMPTS: u32 = 5;

/// Protocol messages
pub mod protocol {
    pub const GREETING: &str = "RSHELLv1 AUTH_REQUIRED\n";
    pub const AUTH_FAIL: &str = "AUTH_FAIL\n";
    pub const AUTH_OK: &str = "AUTH_OK\n";
    pub const GOODBYE: &str = "Goodbye\n";
    pub const END_MARKER: u8 = 0x00;
}

/// Find an available port in the dynamic/private range
pub fn find_available_port() -> Option<u16> {
    use rand::RngExt;
    let mut rng = rand::rng();

    for _ in 0..MAX_PORT_ATTEMPTS {
        let port = rng.random_range(PORT_RANGE_START..PORT_RANGE_END);
        println!("Trying port: {}", port);
        // Try to bind to check availability, then drop
        if std::net::TcpListener::bind(format!("0.0.0.0:{}", port)).is_ok() {
            return Some(port);
        }
    }
    None
}

/// Check if a specific port is available
pub fn is_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(format!("0.0.0.0:{}", port)).is_ok()
}

/// Write port to file
pub fn save_port_to_file(port: u16, path: &str) -> std::io::Result<()> {
    std::fs::write(path, port.to_string())
}

/// Read port from file
pub fn read_port_from_file(path: &str) -> std::io::Result<u16> {
    let content = std::fs::read_to_string(path)?;
    content
        .trim()
        .parse::<u16>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

/// Validate shared secret
pub fn validate_secret(provided: &str, expected: &str) -> bool {
    provided == expected
}

/// Build a command response
pub fn build_response(stdout: &str, stderr: &str, exit_code: i32) -> String {
    match (!stdout.is_empty(), !stderr.is_empty()) {
        (true, true) => format!(
            "=== STDOUT ===\n{}=== STDERR ===\n{}=== Exit Code: {} ===\n",
            stdout, stderr, exit_code
        ),
        (true, false) => format!("{}[Exit Code: {}]\n", stdout, exit_code),
        (false, true) => format!("STDERR: {}[Exit Code: {}]\n", stderr, exit_code),
        (false, false) => format!("[Command completed with exit code: {}]\n", exit_code),
    }
}

/// Check if command is an exit command
pub fn is_exit_command(cmd: &str) -> bool {
    let upper = cmd.to_uppercase();
    upper == "EXIT" || upper == "QUIT"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_is_set() {
        assert!(!PROTOCOL_VERSION.is_empty());
        assert!(PROTOCOL_VERSION.starts_with("RSHELL"));
    }

    #[test]
    fn test_default_secret_is_not_empty() {
        assert!(!DEFAULT_SHARED_SECRET.is_empty());
    }

    #[test]
    fn test_port_range_constants() {
        assert!(PORT_RANGE_START < PORT_RANGE_END);
        assert!(PORT_RANGE_START >= 49152); // IANA dynamic ports start
    }

    #[test]
    fn test_validate_secret_correct() {
        assert!(validate_secret("mysecret", "mysecret"));
    }

    #[test]
    fn test_validate_secret_incorrect() {
        assert!(!validate_secret("wrong", "mysecret"));
    }

    #[test]
    fn test_validate_secret_empty() {
        assert!(!validate_secret("", "mysecret"));
    }

    #[test]
    fn test_is_exit_command() {
        assert!(is_exit_command("exit"));
        assert!(is_exit_command("EXIT"));
        assert!(is_exit_command("Exit"));
        assert!(is_exit_command("quit"));
        assert!(is_exit_command("QUIT"));
        assert!(!is_exit_command("echo hello"));
        assert!(!is_exit_command(""));
    }

    #[test]
    fn test_build_response_stdout_only() {
        let result = build_response("hello\n", "", 0);
        assert!(result.contains("hello\n"));
        assert!(result.contains("[Exit Code: 0]"));
        assert!(!result.contains("STDERR"));
    }

    #[test]
    fn test_build_response_stderr_only() {
        let result = build_response("", "error occurred\n", 1);
        assert!(result.contains("STDERR: error occurred\n"));
        assert!(result.contains("[Exit Code: 1]"));
    }

    #[test]
    fn test_build_response_both() {
        let result = build_response("output\n", "warning\n", 2);
        assert!(result.contains("=== STDOUT ==="));
        assert!(result.contains("output\n"));
        assert!(result.contains("=== STDERR ==="));
        assert!(result.contains("warning\n"));
        assert!(result.contains("=== Exit Code: 2 ==="));
    }

    #[test]
    fn test_build_response_empty() {
        let result = build_response("", "", 0);
        assert!(result.contains("Command completed with exit code: 0"));
    }

    #[test]
    fn test_save_and_read_port() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.port");
        let path_str = path.to_str().unwrap();

        save_port_to_file(12345, path_str).unwrap();
        let port = read_port_from_file(path_str).unwrap();

        assert_eq!(port, 12345);
    }

    #[test]
    fn test_read_port_invalid_file() {
        let result = read_port_from_file("/nonexistent/path/port");
        assert!(result.is_err());
    }

    #[test]
    fn test_read_port_invalid_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.port");
        std::fs::write(path.to_str().unwrap(), "not a number").unwrap();

        let result = read_port_from_file(path.to_str().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn test_find_available_port() {
        let port = find_available_port();
        assert!(port.is_some());
        let port = port.unwrap();
        assert!(port >= PORT_RANGE_START);
        assert!(port <= PORT_RANGE_END);
    }

    #[test]
    fn test_is_port_available_high_port() {
        // Very high port should generally be available
        let port = 65534;
        // This might fail if something is using it, but unlikely
        // Just test the function doesn't panic
        let _ = is_port_available(port);
    }

    #[test]
    fn test_protocol_messages_format() {
        assert!(protocol::GREETING.ends_with('\n'));
        assert!(protocol::AUTH_FAIL.ends_with('\n'));
        assert!(protocol::AUTH_OK.ends_with('\n'));
        assert!(protocol::GOODBYE.ends_with('\n'));
    }
}
