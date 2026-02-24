use crate::ui::add_config::{AddConfigState, ProtocolType};
use crate::ui::layout::format_speed;

#[test]
fn test_format_speed() {
    assert_eq!(format_speed(500.0), "500.0 B/s");
    assert_eq!(format_speed(1024.0), "1.0 KB/s");
    assert_eq!(format_speed(1536.0), "1.5 KB/s");
    assert_eq!(format_speed(1048576.0), "1.0 MB/s"); // 1024 * 1024
    assert_eq!(format_speed(2621440.0), "2.5 MB/s"); // 2.5 * 1024 * 1024
}

// `add_config.rs` testing logic
#[test]
fn test_add_config_state_name_limit_wireguard() {
    let mut state = AddConfigState::new();
    state.protocol = ProtocolType::WireGuard;
    state.focused_field = 0; // Name field
    
    // Try inserting 20 characters
    for i in 0..20 {
        state.insert_char(std::char::from_digit(i % 10, 10).unwrap());
    }
    
    // WireGuard limit is 15
    assert_eq!(state.name.len(), 15);
    assert_eq!(state.name, "012345678901234");
}

#[test]
fn test_add_config_state_name_limit_openvpn() {
    let mut state = AddConfigState::new();
    state.protocol = ProtocolType::OpenVpn;
    state.focused_field = 0; // Name field
    
    // Try inserting 60 characters
    for _ in 0..60 {
        state.insert_char('A');
    }
    
    // OpenVPN limit is 50
    assert_eq!(state.name.len(), 50);
    assert_eq!(state.name, "A".repeat(50));
}

#[test]
fn test_add_config_state_protocol_toggle_truncation() {
    let mut state = AddConfigState::new();
    state.protocol = ProtocolType::OpenVpn;
    state.focused_field = 0;
    
    // Insert 30 characters (valid for OpenVPN)
    for _ in 0..30 {
        state.insert_char('B');
    }
    assert_eq!(state.name.len(), 30);
    
    // Toggle to WireGuard, should truncate to 15
    state.toggle_protocol();
    assert_eq!(state.protocol, ProtocolType::WireGuard);
    assert_eq!(state.name.len(), 15);
    assert_eq!(state.name, "B".repeat(15));
}

#[test]
fn test_add_config_state_multiline_editing() {
    let mut state = AddConfigState::new();
    state.focused_field = 2; // Content field
    
    // Paste multiple lines
    state.paste("line 1\nline 2");
    
    assert_eq!(state.content.len(), 2);
    assert_eq!(state.content[0], "line 1");
    assert_eq!(state.content[1], "line 2");
    
    // Test get_content_string
    assert_eq!(state.get_content_string(), "line 1\nline 2");
    
    // Test backspace deleting a newline
    // Currently cursor is at end of line 2 (col 6, row 1)
    state.content_cursor = (0, 1); // Move to start of line 2
    state.delete_back(); // Should merge with line 1
    
    assert_eq!(state.content.len(), 1);
    assert_eq!(state.content[0], "line 1line 2");
}

#[test]
fn test_add_config_cursor_movement() {
    let mut state = AddConfigState::new();
    state.focused_field = 2; // Content field
    state.paste("hello\nworld");
    
    // Cursor is at end of "world" (col 5, row 1)
    assert_eq!(state.content_cursor, (5, 1));
    
    // Move up
    state.move_cursor(0, -1);
    assert_eq!(state.content_cursor, (5, 0)); // End of "hello"
    
    // Move right (should wrap or stay)
    state.move_cursor(1, 0);
    assert_eq!(state.content_cursor, (0, 1)); // Wrapped to next line start
    
    // Move left
    state.move_cursor(-1, 0);
    assert_eq!(state.content_cursor, (5, 0)); // Wrapped back to previous line end
}

use crate::ui::sudo_prompt::SudoPrompt;

#[test]
fn test_sudo_prompt_initialization() {
    let prompt = SudoPrompt::new();
    assert_eq!(prompt.is_active, false);
    assert_eq!(prompt.input, "");
    assert_eq!(prompt.error_msg, None);
    assert_eq!(prompt.is_verifying, false);
}
