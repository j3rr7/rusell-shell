//! Additional unit tests

use remote_shell::*;

#[test]
fn test_find_available_port_returns_value_in_range() {
    // Run multiple times to check consistency
    for _ in 0..10 {
        let port = find_available_port();
        assert!(port.is_some(), "Should find an available port");
        let port = port.unwrap();
        assert!(
            port >= PORT_RANGE_START && port <= PORT_RANGE_END,
            "Port {} should be in range {}-{}",
            port,
            PORT_RANGE_START,
            PORT_RANGE_END
        );
    }
}

#[test]
fn test_validate_secret_case_sensitive() {
    assert!(validate_secret("SecretKey", "SecretKey"));
    assert!(!validate_secret("secretkey", "SecretKey"));
    assert!(!validate_secret("SECRETKEY", "SecretKey"));
}

#[test]
fn test_validate_secret_with_whitespace() {
    // The secret should not have whitespace for security
    // But our validation is exact match
    assert!(validate_secret("my secret", "my secret"));
    assert!(!validate_secret("my secret", "mysecret"));
    assert!(!validate_secret("mysecret", "my secret"));
}

#[test]
fn test_is_exit_command_variations() {
    let exit_commands = [
        "exit", "EXIT", "Exit", "ExIt", "quit", "QUIT", "Quit", "qUiT",
    ];

    for cmd in exit_commands {
        assert!(is_exit_command(cmd), "'{}' should be exit command", cmd);
    }
}

#[test]
fn test_is_exit_command_with_whitespace() {
    // Should not match with whitespace
    assert!(!is_exit_command("exit "));
    assert!(!is_exit_command(" exit"));
    assert!(!is_exit_command(" exit "));
}

#[test]
fn test_build_response_preserves_newlines() {
    let stdout = "line1\nline2\nline3\n";
    let result = build_response(stdout, "", 0);
    assert!(result.contains("line1\n"));
    assert!(result.contains("line2\n"));
    assert!(result.contains("line3\n"));
}

#[test]
fn test_build_response_empty_outputs() {
    let result = build_response("", "", 0);
    assert!(result.contains("exit code: 0"));
    assert!(!result.contains("STDOUT"));
    assert!(!result.contains("STDERR"));
}

#[test]
fn test_build_response_negative_exit_code() {
    let result = build_response("", "", -1);
    assert!(result.contains("exit code: -1"));
}

#[test]
fn test_build_response_large_exit_code() {
    let result = build_response("", "", 255);
    assert!(result.contains("exit code: 255"));
}

#[test]
fn test_save_port_creates_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.port");

    assert!(!path.exists());
    save_port_to_file(54321, path.to_str().unwrap()).unwrap();
    assert!(path.exists());
}

#[test]
fn test_save_port_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.port");

    save_port_to_file(11111, path.to_str().unwrap()).unwrap();
    save_port_to_file(22222, path.to_str().unwrap()).unwrap();

    let port = read_port_from_file(path.to_str().unwrap()).unwrap();
    assert_eq!(port, 22222);
}

#[test]
fn test_read_port_trims_whitespace() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.port");

    std::fs::write(path.to_str().unwrap(), "  12345  \n").unwrap();
    let port = read_port_from_file(path.to_str().unwrap()).unwrap();
    assert_eq!(port, 12345);
}

#[test]
fn test_read_port_invalid_number() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.port");

    std::fs::write(path.to_str().unwrap(), "abc").unwrap();
    assert!(read_port_from_file(path.to_str().unwrap()).is_err());
}

#[test]
fn test_read_port_too_large() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("large.port");

    std::fs::write(path.to_str().unwrap(), "99999").unwrap();
    // u16 max is 65535, so 99999 should fail
    assert!(read_port_from_file(path.to_str().unwrap()).is_err());
}

#[test]
fn test_read_port_negative() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("neg.port");

    std::fs::write(path.to_str().unwrap(), "-1").unwrap();
    assert!(read_port_from_file(path.to_str().unwrap()).is_err());
}

#[test]
fn test_protocol_greeting_format() {
    assert!(protocol::GREETING.starts_with(PROTOCOL_VERSION));
    assert!(protocol::GREETING.contains("AUTH_REQUIRED"));
}

#[test]
fn test_constants_are_consistent() {
    // Make sure protocol version in greeting matches constant
    assert!(protocol::GREETING.starts_with(PROTOCOL_VERSION));
}
