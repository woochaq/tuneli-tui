#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusedPanel {
    Profiles,
    Logs,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProtocolType {
    WireGuard,
    OpenVpn,
}

pub struct AddConfigState {
    pub name: String,
    pub name_cursor: usize,
    pub protocol: ProtocolType,
    pub content: Vec<String>,
    pub content_cursor: (usize, usize), // (col, row)
    pub content_scroll: (u16, u16),     // (vertical, horizontal)
    pub focused_field: usize, // 0: Name, 1: Protocol, 2: Content, 3: Save
}

impl AddConfigState {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            name_cursor: 0,
            protocol: ProtocolType::WireGuard,
            content: vec![String::new()],
            content_cursor: (0, 0),
            content_scroll: (0, 0),
            focused_field: 0,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if self.focused_field == 0 {
            let max_len = match self.protocol {
                ProtocolType::WireGuard => 15,
                ProtocolType::OpenVpn => 50,
            };
            if self.name.chars().count() < max_len {
                let idx = self.name.char_indices().nth(self.name_cursor).map(|(i, _)| i).unwrap_or(self.name.len());
                self.name.insert(idx, c);
                self.name_cursor += 1;
            }
        } else if self.focused_field == 2 {
            if c == '\n' {
                let remaining = self.content[self.content_cursor.1].split_off(self.content_cursor.0);
                self.content.insert(self.content_cursor.1 + 1, remaining);
                self.content_cursor.1 += 1;
                self.content_cursor.0 = 0;
            } else {
                let idx = self.content[self.content_cursor.1].char_indices().nth(self.content_cursor.0).map(|(i, _)| i).unwrap_or(self.content[self.content_cursor.1].len());
                self.content[self.content_cursor.1].insert(idx, c);
                self.content_cursor.0 += 1;
            }
        }
    }

    pub fn delete_back(&mut self) {
        if self.focused_field == 0 && self.name_cursor > 0 {
            let idx = self.name.char_indices().nth(self.name_cursor - 1).map(|(i, _)| i).unwrap();
            self.name.remove(idx);
            self.name_cursor -= 1;
        } else if self.focused_field == 2 {
            if self.content_cursor.0 > 0 {
                let idx = self.content[self.content_cursor.1].char_indices().nth(self.content_cursor.0 - 1).map(|(i, _)| i).unwrap();
                self.content[self.content_cursor.1].remove(idx);
                self.content_cursor.0 -= 1;
            } else if self.content_cursor.1 > 0 {
                let current_line = self.content.remove(self.content_cursor.1);
                self.content_cursor.1 -= 1;
                self.content_cursor.0 = self.content[self.content_cursor.1].chars().count();
                self.content[self.content_cursor.1].push_str(&current_line);
            }
        }
    }

    pub fn paste(&mut self, text: &str) {
        for c in text.chars() {
            if c != '\r' {
                self.insert_char(c);
            }
        }
    }

    pub fn toggle_protocol(&mut self) {
        self.protocol = match self.protocol {
            ProtocolType::WireGuard => ProtocolType::OpenVpn,
            ProtocolType::OpenVpn => {
                if self.name.chars().count() > 15 {
                    self.name = self.name.chars().take(15).collect();
                    self.name_cursor = std::cmp::min(self.name_cursor, 15);
                }
                ProtocolType::WireGuard
            },
        };
    }

    pub fn move_cursor(&mut self, dx: isize, dy: isize) {
        if self.focused_field == 0 {
            let mut nx = self.name_cursor as isize + dx;
            nx = std::cmp::max(0, nx);
            nx = std::cmp::min(self.name.chars().count() as isize, nx);
            self.name_cursor = nx as usize;
        } else if self.focused_field == 2 {
            let mut row = self.content_cursor.1 as isize + dy;
            row = std::cmp::max(0, row);
            row = std::cmp::min((self.content.len() - 1) as isize, row);
            self.content_cursor.1 = row as usize;
            
            let line_len = self.content[self.content_cursor.1].chars().count() as isize;
            self.content_cursor.0 = std::cmp::min(self.content_cursor.0, line_len as usize);
            
            let mut col = self.content_cursor.0 as isize + dx;
            if col < 0 {
                if self.content_cursor.1 > 0 {
                    self.content_cursor.1 -= 1;
                    col = self.content[self.content_cursor.1].chars().count() as isize;
                } else {
                    col = 0;
                }
            } else if col > line_len {
                if self.content_cursor.1 < self.content.len() - 1 {
                    self.content_cursor.1 += 1;
                    col = 0;
                } else {
                    col = line_len;
                }
            }
            self.content_cursor.0 = col as usize;
        }
    }

    pub fn get_content_string(&self) -> String {
        self.content.join("\n")
    }
}
