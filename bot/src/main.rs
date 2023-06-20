use minecraft_manager::events::MinecraftServerEventType;
use minecraft_manager::{self, tasks::MinecraftServerTask, MinecraftServerSettings};

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serenity::async_trait;
use serenity::model::channel::Message;
use serenity::model::gateway::{Activity, Ready};
use serenity::model::id::{ChannelId, GuildId};
use serenity::prelude::*;

struct Handler {
    is_running: Arc<AtomicBool>,
    should_run: AtomicBool,
    start_as: Arc<Mutex<String>>,
    task_sender: Arc<Mutex<Option<minecraft_manager::thread::MinecraftServerTaskSender>>>,
    bot_loop: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    my_ip: Arc<Mutex<String>>,
    status_channel_id: u64,
    chat_channel_id: u64,
    report_channel_id: u64,
    server_configs: Arc<HashMap<String, (String, MinecraftServerSettings)>>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // eprintln!("Message: {msg:?}");
        if msg.is_own(&ctx.cache) {
            return;
        }
        let ctx = Arc::new(ctx);
        if msg.channel_id.0 == self.chat_channel_id {
            eprintln!(">> {}: '{}'", msg.author.name, msg.content);
            if let Some(task_sender) = self.task_sender.lock().await.as_ref() {
                let author = msg.author.name.as_str();
                let content = msg.content_safe(&ctx.cache);
                let mcmsg = format!("<{author}> {content}");
                _ = task_sender.send_task(MinecraftServerTask::RunCommand(format!(
                    "tellraw @a \"{}\"",
                    mcmsg.replace("\\", "\\\\").replace("\"", "\\\"")
                )));
            }
        } else if msg.channel_id.0 == self.status_channel_id {
            if msg.content.as_str() == "mc..restart" {
                ctx.shard.shutdown_clean();
                std::thread::sleep(Duration::from_secs(2));
                std::process::exit(0);
            }
            if msg.content.as_str().starts_with("mc.setmode ") {
                let id = msg.content[11..].trim();
                if self.server_configs.contains_key(id) {
                    *self.start_as.lock().await = id.to_owned();
                } else {
                    if let Err(e) = msg
                        .reply(
                            &ctx.http,
                            format!(
                                "can't set mode to '{id}', try one of the following: {}",
                                self.server_configs
                                    .iter()
                                    .map(|(id, (name, _cfg))| format!("'{id}' for {name}, "))
                                    .collect::<String>(),
                            )
                            .as_str(),
                        )
                        .await
                    {
                        eprintln!("Error sending message: {:?}", e);
                    }
                }
            }
            if msg.content.as_str() == "mc.start" {
                if !self.is_running.load(Ordering::Relaxed) {
                    self.should_run.swap(true, Ordering::Relaxed);
                    let status_message = msg
                        .reply(
                            &ctx.http,
                            format!("starting {:?}...", self.start_as.lock().await.clone())
                                .as_str(),
                        )
                        .await;
                    if let Err(why) = &status_message {
                        eprintln!("Error sending message: {:?}", why);
                    }
                    self.run_or_stop(ctx.clone(), status_message.ok()).await;
                } else {
                    if let Err(e) = msg.reply(&ctx.http, "server already running!").await {
                        eprintln!("Error sending message: {:?}", e);
                    }
                }
            }
            if msg.content.as_str() == "mc.stop" {
                if self.is_running.load(Ordering::Relaxed) {
                    self.should_run.swap(false, Ordering::Relaxed);
                    if let Err(why) = msg.reply(&ctx.http, "stopping...").await {
                        eprintln!("Error sending message: {:?}", why);
                    }
                    self.run_or_stop(ctx.clone(), None).await;
                } else {
                    if let Err(e) = msg.reply(&ctx.http, "server not running!").await {
                        eprintln!("Error sending message: {:?}", e);
                    }
                }
            }
            if msg.content.as_str().starts_with("mc.run ") {
                if self.is_running.load(Ordering::Relaxed) {
                    let command = msg.content.as_str()[7..].to_string();
                    let status_message = msg
                        .reply(&ctx.http, format!("running command \"{command}\"."))
                        .await;
                    if let Err(why) = &status_message {
                        eprintln!("Error sending message: {:?}", why);
                    }
                    if let Some(task_sender) = self.task_sender.lock().await.as_ref() {
                        _ = task_sender
                            .send_task(MinecraftServerTask::RunCommand(format!("{}", command)));
                    } else {
                        eprintln!("can't run command (can't get sender).");
                    }
                }
            }
            if msg.content.as_str().starts_with("mc.status") {
                let running = self.is_running.load(Ordering::Relaxed);
                let start_as = &*self.start_as.lock().await;
                let status_message = msg
                    .reply(
                        &ctx.http,
                        format!(
                            "Mode: {} ({})\n{}",
                            if let Some((name, _cfg)) = self.server_configs.get(start_as) {
                                name
                            } else {
                                "<unknown>"
                            },
                            start_as,
                            if running { "running" } else { "stopped" },
                        ),
                    )
                    .await;
                if let Err(why) = &status_message {
                    eprintln!("Error sending message: {:?}", why);
                }
            }
        }
        // eprintln!("END OF MSG");
    }

    async fn ready(&self, ctx: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        let ip = if let Some(ip) = self.get_my_ip().await {
            *self.my_ip.lock().await = ip.clone();
            ip
        } else {
            format!("[failed to get ip]")
        };
        let message = ChannelId(self.report_channel_id)
            .send_message(&ctx, |m| m.content(format!("ready!\nip: {ip}")))
            .await;
        if let Err(why) = message {
            eprintln!("Error sending ready message: {:?}", why);
        };
        ctx.idle().await;
        let mut bot_loop = self.bot_loop.lock().await;
        if let Some(bl) = bot_loop.take() {
            std::mem::drop(bl);
        }
        let mut interval = tokio::time::interval(Duration::from_secs(15));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // you can clone this very quickly
        let ctx = Arc::new(ctx);
        let cloned_start_as = self.start_as.clone();
        *bot_loop = Some(tokio::task::spawn(async move {
            let mut counter = 0;
            loop {
                interval.tick().await;
                match counter {
                    0 => {
                        // sys info
                        let text = match (sys_info::mem_info(), sys_info::loadavg()) {
                            (Ok(mem), Ok(load)) => {
                                format!(
                                    "with {}% ram left, load: {}",
                                    mem.avail * 100 / mem.total,
                                    load.one
                                )
                            }
                            (Ok(mem), Err(_)) => {
                                format!("with {}% ram left", mem.avail * 100 / mem.total)
                            }
                            (Err(_), Ok(load)) => format!("load: {}", load.one),
                            (Err(_), Err(_)) => return,
                        };
                        ctx.set_activity(Activity::playing(&text)).await;
                    }
                    3 => {
                        ctx.set_activity(Activity::playing(&format!(
                            "Mode: {}",
                            cloned_start_as.lock().await
                        )))
                        .await;
                    }
                    _ => (),
                }
                if counter > 10 {
                    counter = 0;
                } else {
                    counter += 1;
                }
            }
        }));
    }

    // We use the cache_ready event just in case some cache operation is required in whatever use
    // case you have for this.
    async fn cache_ready(&self, _ctx: Context, _guilds: Vec<GuildId>) {
        println!("Cache built successfully!");

        // it's safe to clone Context, but Arc is cheaper for this use case.
        // Untested claim, just theoretically. :P
    }
}

impl Handler {
    async fn run_or_stop(&self, ctx: Arc<Context>, mut status_message: Option<Message>) {
        // create the minecraft server

        // An AtomicBool is used because it doesn't require a mutable reference to be changed, as
        // we don't have one due to self being an immutable reference.
        let (is_running, should_run) = (
            self.is_running.load(Ordering::Relaxed),
            self.should_run.load(Ordering::Relaxed),
        );
        if is_running != should_run {
            if should_run {
                eprintln!("STARTING MC SERVER");
                self.is_running.swap(true, Ordering::Relaxed);
                self.should_run.swap(false, Ordering::Relaxed);
                ctx.online().await;
                // We have to clone the Arc, as it gets moved into the new thread.
                // tokio::spawn creates a new green thread that can run in parallel with the rest of
                // the application.
                let ctx = ctx.clone();
                let arc_sender = self.task_sender.clone();
                let arc_is_running = self.is_running.clone();
                let started_as = &*self.start_as.lock().await;
                let (display_name, minecraft_server_settings) =
                    match self.server_configs.get(started_as) {
                        Some(v) => v.clone(),
                        None => {
                            self.should_run.store(false, Ordering::Relaxed);
                            return;
                        }
                    };
                let ip_mutex = self.my_ip.clone();
                let chat_channel_id = self.chat_channel_id;
                tokio::spawn(async move {
                    // create minecraft server config
                    // let minecraft_server_settings = MinecraftServerSettings {
                    //     server_type: MinecraftServerType::VanillaPaperMC,
                    //     // directory: "/home/mark/Dokumente/minecraft_server/1".to_string(),
                    //     directory: "/run/media/mark/mcsrv/minecraft_server/survival server bei der omi".to_string(),
                    //     executable: "paper-1.19-81.jar".to_string(),
                    //     dedicated_wam: 2048,
                    // };
                    // start server
                    let mut thread = minecraft_server_settings.spawn();
                    let thread_task_sender = thread.clone_task_sender();
                    *arc_sender.lock().await = Some(thread_task_sender);
                    // handle stdout
                    let mut players_online = HashSet::new();
                    let mut any_changes = true;
                    let mut last_changes = Instant::now();
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        if !thread.is_finished() {
                            thread.update();
                            for event in thread.handle_new_events() {
                                eprintln!("[SRV:] {:?}", event);
                                match &event.event {
                                    MinecraftServerEventType::Warning(w) => {
                                        eprintln!("Warning: {w:?}");
                                    }
                                    MinecraftServerEventType::JoinLeave(ev) => {
                                        if ev.joined {
                                            players_online.insert(ev.username.to_string());
                                        } else {
                                            players_online.remove(&ev.username);
                                        }
                                        any_changes = true;
                                    }
                                    MinecraftServerEventType::ChatMessage(ev) => {
                                        let message = ChannelId(chat_channel_id)
                                            .send_message(&ctx.http, |m| {
                                                m.embed(|e| {
                                                    e.set_author(
                                                        serenity::builder::CreateEmbedAuthor({
                                                            let mut hm =
                                                                std::collections::HashMap::new();
                                                            hm.insert(
                                                                "name",
                                                                ev.author.as_str().into(),
                                                            );
                                                            // hm.insert(
                                                            //     "iconURL",
                                                            //     "https://i.imgur.com/AfFp7pu.png"
                                                            //         .into(),
                                                            // );
                                                            // hm.insert(
                                                            //     "url",
                                                            //     "https://discord.js.org".into(),
                                                            // );
                                                            hm
                                                        }),
                                                    )
                                                    .description(ev.message.as_str())
                                                })
                                            })
                                            .await;
                                        if let Err(why) = message {
                                            eprintln!("Error sending message: {:?}", why);
                                        };
                                    }
                                }
                            }
                            if any_changes || last_changes.elapsed().as_secs_f64() > 15.0 {
                                any_changes = false;
                                last_changes = Instant::now();
                                if let Some(msg) = &mut status_message {
                                    let mut desc = format!(
                                        "IP: {}\nPlayers online: {}\nChat: <#{}>",
                                        ip_mutex.lock().await.as_str(),
                                        {
                                            let mut online: Vec<_> =
                                                players_online.iter().collect();
                                            online.sort_unstable();
                                            let lenm1 = online.len().saturating_sub(1);
                                            online
                                                .into_iter()
                                                .enumerate()
                                                .map(|(i, v)| {
                                                    if i == 0 {
                                                        v.to_string()
                                                    } else if i == lenm1 {
                                                        format!(" and {v}")
                                                    } else {
                                                        format!(", {v}")
                                                    }
                                                })
                                                .collect::<String>()
                                        },
                                        chat_channel_id
                                    );
                                    if let Ok(mem) = sys_info::mem_info() {
                                        let percentage =
                                            100.0 * mem.avail as f64 / mem.total as f64;
                                        desc.push_str(
                                            format!("\nSystem memory: {percentage:.1}% available")
                                                .as_str(),
                                        );
                                    }
                                    if let Ok(load) = sys_info::loadavg() {
                                        desc.push_str(
                                            format!(
                                                "\nSystem load avg. (1/5/15min): {}, {}, {}",
                                                load.one, load.five, load.fifteen
                                            )
                                            .as_str(),
                                        );
                                    }
                                    _ = msg
                                        .edit(&ctx, |m| {
                                            m.content("Minecraft Server Info").embed(|e| {
                                                e.title(format!(
                                                    "{} ({})",
                                                    display_name,
                                                    chrono::offset::Local::now()
                                                        .format("%H:%M, %d.%m.")
                                                ))
                                                .description(desc)
                                            })
                                        })
                                        .await;
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_millis(2000));
                        } else {
                            if let Ok(stop_reason) = thread.get_stop_reason() {
                                eprintln!("Thread stopped: {stop_reason:?}");
                            } else {
                                eprintln!("Thread stopped, but no reason could be found.");
                            }
                            arc_is_running.swap(false, Ordering::Relaxed);
                            // SERVER CLOSED
                            ctx.idle().await;
                            break;
                        }
                    }
                });
            } else {
                eprintln!("STOPPING MC SERVER");
                if let Some(task_sender) = self.task_sender.lock().await.as_mut() {
                    if let Ok(callback) = task_sender.send_task(MinecraftServerTask::Stop) {
                        loop {
                            match callback.recv.recv() {
                                Ok(Err(s)) => {
                                    eprintln!("Command 'Stop' sent custom message '{s}'.")
                                }
                                Ok(Ok(n)) => match n {
                                    100 => {
                                        eprintln!("Stopped server.");
                                        break;
                                    }
                                    100.. => {
                                        eprintln!(
                                            "Command 'Stop' returned nonstandard exit status {n}!"
                                        );
                                        break;
                                    }
                                    n => eprintln!("Stopping server: {n}%"),
                                },
                                Err(_) => {
                                    eprintln!("mpsc channel broke!");
                                    break;
                                }
                            }
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                    } else {
                        eprintln!(
                            "Attempted to send 'Stop' task, but got no callback to wait for."
                        );
                    }
                } else {
                    eprintln!("Couldn't get a task sender.");
                }
                self.is_running.swap(false, Ordering::Relaxed);
            }

            // And of course, we can run more than one thread at different timings.
            // let ctx2 = Arc::clone(&ctx);
            // tokio::spawn(async move {
            //     loop {
            //         set_status_to_current_time(Arc::clone(&ctx2)).await;
            //         tokio::time::sleep(Duration::from_secs(60)).await;
            //     }
            // });

            // Now that the loop is running, we set the bool to true
        }
    }
    async fn get_my_ip(&self) -> Option<String> {
        if let Ok(curl) = std::process::Command::new("curl")
            .arg("https://ipinfo.io/ip")
            .output()
        {
            return Some(String::from_utf8_lossy(curl.stdout.as_slice()).into_owned());
        }
        None
    }
}

// async fn set_status_to_current_time(ctx: Arc<Context>) {
// let current_time = Utc::now();
// let formatted_time = current_time.to_rfc2822();
// ctx.set_activity(Activity::playing(&formatted_time)).await;
// }

#[tokio::main]
async fn main() {
    let token = std::env::var("mcdcbot_token").expect("mcdcbot_token env variable is required!");
    let report_channel_id = std::env::var("mcdcbot_id_report")
        .expect("mcdcbot_id_report env variable is required!")
        .parse()
        .expect("mcdcbot_id_report env variable must be a number (u64)!");
    let status_channel_id = std::env::var("mcdcbot_id_status")
        .expect("mcdcbot_id_status env variable is required!")
        .parse()
        .expect("mcdcbot_id_status env variable must be a number (u64)!");
    let chat_channel_id = std::env::var("mcdcbot_id_chat")
        .expect("mcdcbot_id_chat env variable is required!")
        .parse()
        .expect("mcdcbot_id_chat env variable must be a number (u64)!");
    let servers_file =
        std::env::var("mcdcbot_servers").expect("mcdcbot_servers env var is required (file path)!");
    let default_server_config = match std::env::var("mcdcbot_server_default") {
        Ok(v) => v.trim().to_owned(),
        Err(_) => String::new(),
    };
    let mut server_configs = HashMap::new();
    match std::fs::read_to_string(&servers_file) {
        Ok(v) => {
            let mut lines = v.lines();
            loop {
                let line = if let Some(l) = lines.next() {
                    l
                } else {
                    break;
                };
                if line.trim().is_empty() {
                    continue;
                }
                if let Some((identifier, display_name)) = line.split_once(' ') {
                    server_configs.insert(
                        identifier.to_owned(),
                        (
                            display_name.to_owned(),
                            MinecraftServerSettings::from_lines(&mut lines).unwrap(),
                        ),
                    );
                } else {
                    panic!("server config header line ({line}) didn't contain a space, but format must be <id> <display name>")
                }
            }
        }
        Err(e) => panic!(
            "Couldn't load file provided through mcdcbot_servers env var ({servers_file}): {e}"
        ),
    }
    let intents = GatewayIntents::GUILD_MESSAGES
        // | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::GUILDS
        | GatewayIntents::MESSAGE_CONTENT;
    eprintln!(" | - - STARTING - - |");
    eprintln!(
        " | token: {} ... {}",
        &token[0..token.len() / 8],
        &token[token.len() / 8 * 7..]
    );
    eprintln!(" | report channel id: {report_channel_id}");
    eprintln!(" | status channel id: {status_channel_id}");
    eprintln!(" | chat   channel id: {chat_channel_id}");
    eprintln!(" | server configs:");
    for (id, (name, cfg)) in server_configs.iter() {
        eprintln!(" | | {id} - \"{name}\" - {cfg:?}");
    }
    eprintln!(
        " | default config: {default_server_config}{}",
        if server_configs.contains_key(&default_server_config) {
            ""
        } else {
            " (WARN: doesn't exist - use mcdcbot_server_default env var to change)"
        }
    );
    let mut client = Client::builder(&token, intents)
        .event_handler(Handler {
            is_running: Arc::new(AtomicBool::new(false)),
            should_run: AtomicBool::new(false),
            start_as: Arc::new(Mutex::new(default_server_config)),
            task_sender: Arc::new(Mutex::new(None)),
            bot_loop: Arc::new(Mutex::new(None)),
            my_ip: Arc::new(Mutex::new(format!("(???)"))),
            chat_channel_id,
            status_channel_id,
            report_channel_id,
            server_configs: Arc::new(server_configs),
        })
        .await
        .expect("Error creating client");

    if let Err(why) = client.start().await {
        eprintln!("Client error: {:?}", why);
    }
}
