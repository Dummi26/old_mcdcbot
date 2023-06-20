use crate::{
    chat::ChatMessage,
    events::{self, MinecraftServerEventType},
    MinecraftServerSettings, MinecraftServerType,
};

pub enum ParseOutput {
    Nothing,
    Error(ParseError),
    Event(MinecraftServerEventType),
}

pub enum ParseError {}

pub fn parse_line(line: &str, settings: &MinecraftServerSettings) -> ParseOutput {
    if line.trim().is_empty() {
        return ParseOutput::Nothing;
    }
    match &settings.server_type {
        MinecraftServerType::VanillaMojang => {
            if let Some((_time, rest)) = line[1..].split_once("] [Server thread/INFO]: ") {
                let rest = rest.trim();
                if rest.starts_with("<") {
                    if let Some((user, msg)) = rest[1..].split_once("> ") {
                        return ParseOutput::Event(
                            MinecraftServerEventType::ChatMessage(
                                ChatMessage {
                                    author: user.to_owned(),
                                    message: msg.to_owned(),
                                }
                            )
                        );
                    }
                } else if rest.ends_with(" joined the game") {
                    return ParseOutput::Event(
                        MinecraftServerEventType::JoinLeave(
                            events::JoinLeaveEvent {
                                username: rest[0..rest.len() - " joined the game".len()].to_owned(),
                                joined: true,
                            }
                        )
                    );
                } else if rest.ends_with(" left the game") {
                    return ParseOutput::Event(
                        MinecraftServerEventType::JoinLeave(
                            events::JoinLeaveEvent {
                                username: rest[0..rest.len() - " left the game".len()].to_owned(),
                                joined: false,
                            }
                        )
                    );
                }

            }
            // Vanilla servers not yet supported...
        }
        MinecraftServerType::VanillaPaperMC => match line.chars().next() {
            Some('[') => {
                if let Some((_time, rest)) = line[1..].split_once(' ') {
                    if let Some((severity, rest)) = rest.split_once(']') {
                        if rest.starts_with(": ") {
                            let rest = &rest[2..];
                            // eprintln!("Time: '{time}', Severity: '{severity}', Rest: '{rest}'.");
                            match severity {
                                "INFO" => {
                                    if let Some('<') = rest.chars().next() {
                                        if let Some((username, message)) = rest[1..].split_once('>')
                                        {
                                            return ParseOutput::Event(
                                                MinecraftServerEventType::ChatMessage(
                                                    ChatMessage {
                                                        author: username.to_string(),
                                                        message: message[1..].to_string(),
                                                    },
                                                ),
                                            );
                                        }
                                    } // join/leave
                                    if rest.ends_with(" joined the game") {
                                        let username = &rest[..rest.len() - 16];
                                        return ParseOutput::Event(
                                            MinecraftServerEventType::JoinLeave(
                                                events::JoinLeaveEvent {
                                                    username: username.to_string(),
                                                    joined: true,
                                                },
                                            ),
                                        );
                                    }
                                    if rest.ends_with(" left the game") {
                                        let username = &rest[..rest.len() - 14];
                                        return ParseOutput::Event(
                                            MinecraftServerEventType::JoinLeave(
                                                events::JoinLeaveEvent {
                                                    username: username.to_string(),
                                                    joined: false,
                                                },
                                            ),
                                        );
                                    }
                                }
                                _ => (),
                            }
                        }
                    }
                }
            }
            _ => (),
        },
    }
    ParseOutput::Nothing
}
