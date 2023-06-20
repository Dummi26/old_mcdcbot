use crate::chat::ChatMessage;

#[derive(Debug)]
pub struct MinecraftServerEvent {
    pub time: (),
    pub event: MinecraftServerEventType,
}

#[derive(Debug)]
pub enum MinecraftServerEventType {
    Warning(MinecraftServerWarning),
    JoinLeave(JoinLeaveEvent),
    ChatMessage(ChatMessage),
}

#[derive(Debug)]
pub enum MinecraftServerWarning {
    /// The server process was spawned, but std{in,out,err} was not captured.
    CouldNotGetServerProcessStdio,
    CantWriteToStdin(std::io::Error),
}

#[derive(Debug)]
pub struct JoinLeaveEvent {
    pub username: String,
    pub joined: bool,
}
