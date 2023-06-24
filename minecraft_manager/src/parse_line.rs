use std::{
    io::{BufRead, BufReader, Write},
    process::{self, Stdio},
};

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

pub enum ParseError {
    /// any other errors (for custom line parser implementations)
    Custom(String),
}

pub fn parse_line(line: &str, settings: &MinecraftServerSettings) -> ParseOutput {
    if line.trim().is_empty() {
        return ParseOutput::Nothing;
    }
    match &settings.server_type {
        MinecraftServerType::Custom {
            line_parser,
            line_parser_proc,
            ..
        } => {
            let mut proc = line_parser_proc.lock().unwrap();
            let proc = &mut *proc;
            let make_new_proc = if let Some((proc, _, _)) = proc {
                if let Ok(Some(_)) = proc.try_wait() {
                    // has exited
                    true
                } else {
                    false
                }
            } else {
                true
            };
            if make_new_proc {
                if let Ok(mut new_proc) = process::Command::new(line_parser)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .spawn()
                {
                    if let (Some(stdin), Some(stdout)) =
                        (new_proc.stdin.take(), new_proc.stdout.take())
                    {
                        *proc = Some((new_proc, stdin, BufReader::new(stdout)));
                    } else {
                        eprintln!("[WARN/CUSTOM-LINE-PARSER] No stdin/stdout handles!");
                        _ = new_proc.kill();
                    }
                } else {
                    eprintln!("[WARN/CUSTOM-LINE-PARSER] Can't spawn command '{line_parser}'!");
                }
            }
            if let Some((_proc, stdin, stdout)) = proc {
                if let Err(e) = writeln!(stdin, "{line}") {
                    eprintln!("[WARN/CUSTOM-LINE-PARSER] Can't write to stdin: {e:?}");
                    return ParseOutput::Nothing;
                };
                let mut buf = String::new();
                if let Err(e) = stdout.read_line(&mut buf) {
                    eprintln!("[WARN/CUSTOM-LINE-PARSER] Can't read_line: {e:?}");
                    return ParseOutput::Nothing;
                };
                if buf.ends_with('\n') || buf.ends_with('\r') {
                    buf.pop();
                }
                if buf.len() > 0 {
                    match buf.as_bytes()[0] {
                        b'c' => {
                            ParseOutput::Event(MinecraftServerEventType::ChatMessage(ChatMessage {
                                author: buf[1..].to_owned(),
                                message: {
                                    let mut o = String::new();
                                    if let Err(e) = stdout.read_line(&mut o) {
                                        eprintln!(
                                            "[WARN/CUSTOM-LINE-PARSER] Can't read_line: {e:?}"
                                        );
                                        return ParseOutput::Nothing;
                                    }
                                    o
                                },
                            }))
                        }
                        b'j' => ParseOutput::Event(MinecraftServerEventType::JoinLeave(
                            events::JoinLeaveEvent {
                                username: buf[1..].to_owned(),
                                joined: true,
                            },
                        )),
                        b'l' => ParseOutput::Event(MinecraftServerEventType::JoinLeave(
                            events::JoinLeaveEvent {
                                username: buf[1..].to_owned(),
                                joined: false,
                            },
                        )),
                        b'e' => ParseOutput::Error({
                            if buf.len() > 1 {
                                match buf.as_bytes()[1] {
                                    b'c' => ParseError::Custom(buf[2..].to_string()),
                                    _ => ParseError::Custom(String::new()),
                                }
                            } else {
                                ParseError::Custom(String::new())
                            }
                        }),
                        _ => ParseOutput::Nothing,
                    }
                } else {
                    ParseOutput::Nothing
                }
            } else {
                eprintln!("[WARN/CUSTOM-LINE-PARSER] No process!");
                ParseOutput::Nothing
            }
        }
        MinecraftServerType::VanillaMojang => {
            if let Some((_time, rest)) = line[1..].split_once("] [Server thread/INFO]: ") {
                let rest = rest.trim();
                if rest.starts_with("<") {
                    if let Some((user, msg)) = rest[1..].split_once("> ") {
                        return ParseOutput::Event(MinecraftServerEventType::ChatMessage(
                            ChatMessage {
                                author: user.to_owned(),
                                message: msg.to_owned(),
                            },
                        ));
                    }
                } else if rest.ends_with(" joined the game") {
                    return ParseOutput::Event(MinecraftServerEventType::JoinLeave(
                        events::JoinLeaveEvent {
                            username: rest[0..rest.len() - " joined the game".len()].to_owned(),
                            joined: true,
                        },
                    ));
                } else if rest.ends_with(" left the game") {
                    return ParseOutput::Event(MinecraftServerEventType::JoinLeave(
                        events::JoinLeaveEvent {
                            username: rest[0..rest.len() - " left the game".len()].to_owned(),
                            joined: false,
                        },
                    ));
                }
            }
            ParseOutput::Nothing
            // Vanilla servers not yet supported...
        }
        MinecraftServerType::VanillaPaperMC => {
            match line.chars().next() {
                Some('[') => {
                    if let Some((_time, rest)) = line[1..].split_once(' ') {
                        if let Some((severity, rest)) = rest.split_once(']') {
                            if rest.starts_with(": ") {
                                let rest = &rest[2..];
                                // eprintln!("Time: '{time}', Severity: '{severity}', Rest: '{rest}'.");
                                match severity {
                                    "INFO" => {
                                        if let Some('<') = rest.chars().next() {
                                            if let Some((username, message)) =
                                                rest[1..].split_once('>')
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
                                        if rest.trim_end().ends_with(" joined the game") {
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
                                        if rest.trim_end().ends_with(" left the game") {
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
            }
            ParseOutput::Nothing
        }
    }
}
