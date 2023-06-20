use std::thread::JoinHandle;

use crate::tasks::MinecraftServerTaskCallback;

use {
    crate::{
        events::MinecraftServerEvent,
        tasks::MinecraftServerTask,
        threaded::{self, MinecraftServerStopReason},
        MinecraftServerSettings,
    },
    std::{collections::VecDeque, sync::mpsc},
};

pub struct MinecraftServerThread {
    events: ThreadData<MinecraftServerEvent>,
    task_sender: MinecraftServerTaskSender,
    join_handle: JoinHandle<MinecraftServerStopReason>,
}

/// A clonable type allowing multiple threads to send tasks to the server.
#[derive(Clone)]
pub struct MinecraftServerTaskSender(
    mpsc::Sender<(MinecraftServerTask, mpsc::Sender<Result<u8, String>>)>,
);

impl MinecraftServerTaskSender {
    pub fn send_task(&self, task: MinecraftServerTask) -> Result<MinecraftServerTaskCallback, ()> {
        let (sendable, callback) = task.generate_callback();
        if let Ok(_) = self.0.send(sendable) {
            Ok(callback)
        } else {
            Err(())
        }
    }
}

impl MinecraftServerThread {
    pub fn start(settings: MinecraftServerSettings) -> Self {
        let (task_sender, event_receiver, join_handle) = threaded::run(settings);
        Self {
            events: ThreadData::new(event_receiver, 100),
            task_sender: MinecraftServerTaskSender(task_sender),
            join_handle,
        }
    }
    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }
    pub fn get_stop_reason(self) -> Result<MinecraftServerStopReason, ()> {
        if self.is_finished() {
            if let Ok(v) = self.join_handle.join() {
                Ok(v)
            } else {
                Err(())
            }
        } else {
            Err(())
        }
    }
    pub fn update(&mut self) {
        self.events.update();
    }
    pub fn handle_new_events(
        &mut self,
    ) -> std::iter::Skip<std::collections::vec_deque::Iter<MinecraftServerEvent>> {
        self.events.handle_all()
    }
    pub fn clone_task_sender(&self) -> MinecraftServerTaskSender {
        self.task_sender.clone()
    }
}

struct ThreadData<T> {
    mpsc: mpsc::Receiver<T>,
    buffer: VecDeque<T>,
    unhandeled: usize,
    capacity: usize,
}

impl<T> ThreadData<T> {
    pub fn new(mpsc_receiver: mpsc::Receiver<T>, capacity: usize) -> Self {
        Self {
            mpsc: mpsc_receiver,
            buffer: VecDeque::with_capacity(capacity),
            unhandeled: 0,
            capacity,
        }
    }
    pub fn update(&mut self) -> usize {
        let mut unhandeled = 0;
        while let Ok(new_content) = self.mpsc.try_recv() {
            if self.buffer.len() == self.capacity {
                self.buffer.pop_front();
            }
            self.buffer.push_back(new_content);
            unhandeled += 1;
        }
        self.unhandeled += unhandeled;
        unhandeled
    }
    pub fn handle_all(&mut self) -> std::iter::Skip<std::collections::vec_deque::Iter<T>> {
        let unhandeled = self.unhandeled;
        self.unhandeled = 0;
        self.buffer
            .iter()
            .skip(self.buffer.len().saturating_sub(unhandeled))
    }
}
