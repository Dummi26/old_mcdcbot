pub mod chat;
pub mod events;
mod parse_line;
pub mod tasks;
pub mod thread;
mod threaded;

use std::process::Command;

use thread::MinecraftServerThread;

#[derive(Clone, Debug)]
pub struct MinecraftServerSettings {
    pub server_type: MinecraftServerType,
    pub directory: String,
    pub executable: String,
    /// the amount of dedicated wam for the JVM in [TODO!] (-Xm{s,x}...M)
    pub dedicated_wam: u32,
    pub java_cmd: Option<String>,
}
impl MinecraftServerSettings {
    /// takes lines from the provided iterator until an empty line is reached (line.trim().is_empty()) or the iterator ends.
    /// Note: The iterator items should NOT contain newline characters!
    pub fn from_lines<'a, L: Iterator<Item = &'a str>>(
        lines: &mut L,
    ) -> Result<Self, MinecraftServerSettingsFromLinesError> {
        let mut server_type = Err(MinecraftServerSettingsFromLinesError::MissingServerType);
        let mut directory = Err(MinecraftServerSettingsFromLinesError::MissingDirectory);
        let mut executable = Err(MinecraftServerSettingsFromLinesError::MissingExecutable);
        let mut ram = None;
        let mut java_cmd = None;
        loop {
            if let Some(line) = lines.next() {
                if let Some((key, value)) = line.split_once('=') {
                    match key {
                        "type" => {
                            server_type =
                                Ok(match value.trim() {
                                    "vanilla-mojang" => MinecraftServerType::VanillaMojang,
                                    "vanilla-papermc" => MinecraftServerType::VanillaPaperMC,
                                    other => return Err(
                                        MinecraftServerSettingsFromLinesError::UnknownServerType(
                                            other.to_owned(),
                                        ),
                                    ),
                                });
                        }
                        "dir" => directory = Ok(value.to_owned()),
                        "exec" => executable = Ok(value.to_owned()),
                        "ram" => {
                            if let Ok(v) = value.trim().parse() {
                                ram = Some(v);
                            } else {
                                return Err(MinecraftServerSettingsFromLinesError::RamNotAnInt(
                                    value.to_owned(),
                                ));
                            }
                        }
                        "java_cmd" => java_cmd = Some(value.to_owned()),
                        k => {
                            return Err(MinecraftServerSettingsFromLinesError::UnknownKey(
                                k.to_owned(),
                            ))
                        }
                    }
                } else if line.trim().is_empty() {
                    break;
                } else {
                    return Err(MinecraftServerSettingsFromLinesError::UnknownKey(
                        line.to_owned(),
                    ));
                }
            } else {
                break;
            }
        }
        let mut o = Self::new(server_type?, directory?, executable?);
        if let Some(ram) = ram {
            o = o.with_ram(ram);
        }
        if let Some(java_cmd) = java_cmd {
            o = o.with_java_cmd(Some(java_cmd));
        }
        Ok(o)
    }
}
#[derive(Debug)]
pub enum MinecraftServerSettingsFromLinesError {
    UnknownKey(String),
    MissingServerType,
    UnknownServerType(String),
    MissingDirectory,
    MissingExecutable,
    RamNotAnInt(String),
}

impl MinecraftServerSettings {
    pub fn spawn(self) -> MinecraftServerThread {
        MinecraftServerThread::start(self)
    }

    pub fn new(server_type: MinecraftServerType, directory: String, executable: String) -> Self {
        Self {
            server_type,
            directory,
            executable,
            dedicated_wam: 1024,
            java_cmd: None,
        }
    }
    pub fn with_ram(mut self, ram_mb: u32) -> Self {
        self.dedicated_wam = ram_mb;
        self
    }
    pub fn with_java_cmd(mut self, java_cmd: Option<String>) -> Self {
        self.java_cmd = java_cmd;
        self
    }

    pub fn get_command(&self) -> Command {
        let mut cmd = Command::new(if let Some(c) = &self.java_cmd {
            c.as_str()
        } else {
            match &self.server_type {
                MinecraftServerType::VanillaMojang => "java", // "/usr/lib/jvm/openjdk17/bin/java",
                MinecraftServerType::VanillaPaperMC => "java", // "/usr/lib/jvm/openjdk17/bin/java",
            }
        });
        cmd.current_dir(&self.directory);
        match &self.server_type {
            MinecraftServerType::VanillaMojang | MinecraftServerType::VanillaPaperMC => cmd.args([
                format!("-Xms{}M", self.dedicated_wam),
                format!("-Xmx{}M", self.dedicated_wam),
                "-Dsun.stdout.encoding=UTF-8".to_owned(),
                "-Dsun.stderr.encoding=UTF-8".to_owned(),
                "-DFile.Encoding=UTF-8".to_owned(),
                "-jar".to_string(),
                self.executable.to_string(),
                "nogui".to_string(),
            ]),
        };
        cmd
    }
}

#[derive(Clone, Debug)]
pub enum MinecraftServerType {
    VanillaMojang,
    VanillaPaperMC,
}

pub fn test() {
    // create minecraft server config
    let minecraft_server_settings = MinecraftServerSettings {
        server_type: MinecraftServerType::VanillaPaperMC,
        directory: "/home/mark/Dokumente/minecraft_server/1".to_string(),
        executable: "paper-1.19-81.jar".to_string(),
        dedicated_wam: 1024,
        java_cmd: None,
    };
    // start server
    let mut thread = minecraft_server_settings.spawn();
    // handle stdin
    if false {
        let sender = thread.clone_task_sender();
        std::thread::spawn(move || {
            let stdin = std::io::stdin();
            loop {
                let mut line = String::new();
                if let Ok(_) = stdin.read_line(&mut line) {
                    if line.trim().is_empty() {
                        std::thread::sleep(std::time::Duration::from_secs(300));
                        continue;
                    }
                    if let Err(_) = sender.send_task(tasks::MinecraftServerTask::RunCommand(line)) {
                        break;
                    }
                } else {
                    break;
                }
            }
        });
    }
    // handle stdout
    loop {
        if !thread.is_finished() {
            thread.update();
            for event in thread.handle_new_events() {
                eprintln!("Event: {event:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        } else {
            if let Ok(stop_reason) = thread.get_stop_reason() {
                eprintln!("Thread stopped: {stop_reason:?}");
            }
            break;
        }
    }
}
