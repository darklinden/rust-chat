use std::{io, time::Duration};

use super::client::WsClient;
use crossterm::event::{self, poll, Event, KeyCode, KeyEventKind};
use ratatui::{prelude::*, widgets::*};
use tokio_tungstenite::tungstenite::Message;

pub enum InputMode {
    Normal,
    Editing,
}

/// App holds the state of the application
pub struct App {
    /// Current value of the input box
    input: String,
    /// Position of cursor in the editor area.
    cursor_position: usize,
    /// Current input mode
    input_mode: InputMode,
    /// History of recorded messages
    messages: Vec<String>,
    message_index: usize,
    client: Option<WsClient>,
}

impl Default for App {
    fn default() -> App {
        App {
            input: String::new(),
            input_mode: InputMode::Normal,
            messages: Vec::new(),
            message_index: 0,
            cursor_position: 0,
            client: None,
        }
    }
}

impl App {
    fn move_cursor_left(&mut self) {
        let cursor_moved_left = self.cursor_position.saturating_sub(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_left);
    }

    fn move_cursor_right(&mut self) {
        let cursor_moved_right = self.cursor_position.saturating_add(1);
        self.cursor_position = self.clamp_cursor(cursor_moved_right);
    }

    fn enter_char(&mut self, new_char: char) {
        self.input.insert(self.cursor_position, new_char);

        self.move_cursor_right();
    }

    fn delete_char(&mut self) {
        let is_not_cursor_leftmost = self.cursor_position != 0;
        if is_not_cursor_leftmost {
            // Method "remove" is not used on the saved text for deleting the selected char.
            // Reason: Using remove on String works on bytes instead of the chars.
            // Using remove would require special care because of char boundaries.

            let current_index = self.cursor_position;
            let from_left_to_current_index = current_index - 1;

            // Getting all characters before the selected character.
            let before_char_to_delete = self.input.chars().take(from_left_to_current_index);
            // Getting all characters after selected character.
            let after_char_to_delete = self.input.chars().skip(current_index);

            // Put all characters together except the selected one.
            // By leaving the selected one out, it is forgotten and therefore deleted.
            self.input = before_char_to_delete.chain(after_char_to_delete).collect();
            self.move_cursor_left();
        }
    }

    fn clamp_cursor(&self, new_cursor_pos: usize) -> usize {
        new_cursor_pos.clamp(0, self.input.len())
    }

    fn reset_cursor(&mut self) {
        self.cursor_position = 0;
    }

    async fn recv_messages(&mut self) {
        if self.client.is_none() {
            return;
        }

        let mut client = self.client.take().unwrap();
        let mut msg = client.recv();

        while msg.is_ok() {
            let message = msg.unwrap();
            match message {
                Message::Binary(bytes) => {
                    let packet = proto::ChatPacket::deserialize(bytes);

                    match packet.packet_type {
                        proto::ChatPacketType::Login => {
                            self.messages.push(packet.packet_message);
                        }
                        proto::ChatPacketType::Close => {
                            self.messages
                                .push(format!("{} has left the chat", packet.packet_message));
                        }
                        proto::ChatPacketType::Chat => {
                            self.messages.push(format!("{}", packet.packet_message));
                        }
                        _ => {}
                    }
                }
                Message::Close(_) => {
                    self.client = None;
                    self.messages.push("Connection closed".into());
                    break;
                }
                Message::Ping(_) => {
                    client.send(Message::Pong(vec![])).await.unwrap();
                }
                _ => {}
            }
            msg = client.recv();
        }

        self.client = Some(client);
    }

    async fn submit_message(&mut self) {
        if self.input.is_empty() {
            return;
        }

        let mut message = self.input.clone();

        if message.starts_with("connect ") {
            let url = message.split_off(8);
            if self.client.is_some() {
                let mut client = self.client.take().unwrap();
                client.disconnect().await.unwrap();
                self.client = None;
            }
            let client = WsClient::new(&url).await.unwrap();
            self.client = Some(client);
            self.messages.push("Connection established".into());
        } else if message.starts_with("exit") {
            if self.client.is_some() {
                let mut client = self.client.take().unwrap();
                client.disconnect().await.unwrap();
                self.client = None;
                self.messages.push("Connection closed".into());
            }
        } else if message.starts_with("login ") {
            let name = message.split_off(6);
            let message = proto::ChatPacket::new(proto::ChatPacketType::Login, name);
            let bytes = Message::Binary(message.serialize());
            let send_result = self.client.as_ref().unwrap().send(bytes).await;
            if send_result.is_err() {
                self.messages.push("Not connected".into());
                self.client = None;
            } else {
                // self.messages.push(self.input.clone());
            }
        } else {
            let message = proto::ChatPacket::new(proto::ChatPacketType::Chat, message);
            let bytes = Message::Binary(message.serialize());
            let send_result = self.client.as_ref().unwrap().send(bytes).await;
            if send_result.is_err() {
                self.messages.push("Not connected".into());
                self.client = None;
            } else {
                // self.messages.push(self.input.clone());
            }
        }

        self.input.clear();
        self.reset_cursor();
    }

    pub async fn run_app<B: Backend>(mut self: Self, terminal: &mut Terminal<B>) -> io::Result<()> {
        loop {
            terminal.draw(|f| self.ui(f))?;

            self.recv_messages().await;

            if poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    match self.input_mode {
                        InputMode::Normal => match key.code {
                            KeyCode::Char('e') => {
                                self.input_mode = InputMode::Editing;
                            }
                            KeyCode::Char('q') => {
                                return Ok(());
                            }
                            _ => {}
                        },
                        InputMode::Editing if key.kind == KeyEventKind::Press => match key.code {
                            KeyCode::Enter => self.submit_message().await,
                            KeyCode::Char(to_insert) => {
                                self.enter_char(to_insert);
                            }
                            KeyCode::Backspace => {
                                self.delete_char();
                            }
                            KeyCode::Left => {
                                self.move_cursor_left();
                            }
                            KeyCode::Right => {
                                self.move_cursor_right();
                            }
                            KeyCode::Esc => {
                                self.input_mode = InputMode::Normal;
                            }
                            KeyCode::Up => {
                                if self.message_index > 0 {
                                    self.message_index -= 1;
                                }
                            }
                            KeyCode::Down => {
                                if self.message_index < self.messages.len() - 1 {
                                    self.message_index += 1;
                                }
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
            }
        }
    }

    pub fn ui(self: &Self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // Messages
                Constraint::Length(1), // Tip
                Constraint::Length(3), // Input
            ])
            .split(f.size());

        // Messages
        let messages: Vec<ListItem> = self.messages[self.message_index..]
            .iter()
            .enumerate()
            .map(|(_i, m)| {
                let content = Line::from(Span::raw(m));
                ListItem::new(content)
            })
            .collect();
        let messages =
            List::new(messages).block(Block::default().borders(Borders::ALL).title("Messages"));
        f.render_widget(messages, chunks[0]);

        // Tip
        let (msg, style) = match self.input_mode {
            InputMode::Normal => (
                vec![
                    "Press ".into(),
                    "q".bold(),
                    " to exit, ".into(),
                    "e".bold(),
                    " to start editing.".bold(),
                ],
                Style::default().add_modifier(Modifier::RAPID_BLINK),
            ),
            InputMode::Editing => (
                vec![
                    "Press ".into(),
                    "Esc".bold(),
                    " to stop editing, ".into(),
                    "Enter".bold(),
                    " to record the message".into(),
                ],
                Style::default(),
            ),
        };
        let mut text = Text::from(Line::from(msg));
        text.patch_style(style);
        let help_message = Paragraph::new(text);
        f.render_widget(help_message, chunks[1]);

        // Input
        let input = Paragraph::new(self.input.as_str())
            .style(match self.input_mode {
                InputMode::Normal => Style::default(),
                InputMode::Editing => Style::default().fg(Color::Yellow),
            })
            .block(Block::default().borders(Borders::ALL).title("Input"));
        f.render_widget(input, chunks[2]);
        match self.input_mode {
            InputMode::Normal =>
                // Hide the cursor. `Frame` does this by default, so we don't need to do anything here
                {}

            InputMode::Editing => {
                // Make the cursor visible and ask ratatui to put it at the specified coordinates after
                // rendering
                f.set_cursor(
                    // Draw the cursor at the current position in the input field.
                    // This position is can be controlled via the left and right arrow key
                    chunks[2].x + self.cursor_position as u16 + 1,
                    // Move one line down, from the border to the input line
                    chunks[2].y + 1,
                )
            }
        }
    }
}
