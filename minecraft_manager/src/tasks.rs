use std::sync::mpsc;

#[derive(Clone, Debug)]
pub enum MinecraftServerTask {
    Stop,
    Kill,
    RunCommand(String),
}

impl MinecraftServerTask {
    pub fn generate_callback(
        self,
    ) -> (
        (Self, mpsc::Sender<Result<u8, String>>),
        MinecraftServerTaskCallback,
    ) {
        let (sender, update_receiver) = mpsc::channel();
        (
            (self, sender),
            MinecraftServerTaskCallback::new(update_receiver),
        )
    }
}

pub struct MinecraftServerTaskCallback {
    /// Ok(n) if n < 100 = progress in %
    /// Ok(100) = finished
    /// Ok(n) if n > 100 = task ended with non-standard exit status (advise checking log)
    /// Err(_) = custom message (for log)
    pub recv: mpsc::Receiver<Result<u8, String>>, // TODO: NOT PUBLIC
}

impl MinecraftServerTaskCallback {
    pub fn new(recv: mpsc::Receiver<Result<u8, String>>) -> Self {
        Self { recv }
    }
}
