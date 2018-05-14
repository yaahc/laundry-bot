extern crate chrono;

use chrono::prelude::*;
use laundry::alert::Alerter;
use serenity::framework::StandardFramework;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::prelude::*;
use std::env;
use std::sync::{Arc, Mutex};
use typemap::Key;

struct Snooze;

impl Key for Snooze
{
    type Value = Arc<Mutex<Option<DateTime<Local>>>>;
}

struct Channels;

impl Key for Channels
{
    type Value = Arc<Mutex<Option<Vec<ChannelId>>>>;
}

struct Handler;

impl EventHandler for Handler
{
    fn ready(&self, context: Context, ready: Ready)
    {
        println!("Connected as {} {:#?}", ready.user.name, ready);

        let mut channel_ids = Vec::new();

        // collect laundry channel id(s)
        for guild in &ready.guilds {
            if let Ok(channels) = guild.id().channels() {
                for (channel, guild_channel) in &channels {
                    if guild_channel.name == "laundry" {
                        channel_ids.push(channel.clone());
                    }
                }
            }
        }

        let data = context.data.lock();
        match data.get::<Channels>() {
            Some(channels) => {
                let mut ids = channels.lock().unwrap();
                *ids = Some(channel_ids);
            },
            None => {
                println!("There was a problem updating the channels vector");
            },
        }
    }
}

#[derive(Debug)]
pub struct DiscordAlerter
{
    snooze_time: Arc<Mutex<Option<DateTime<Local>>>>,
    channel_ids: Arc<Mutex<Option<Vec<ChannelId>>>>,
}

impl DiscordAlerter
{
    pub fn new() -> DiscordAlerter
    {
        let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

        let alerter = DiscordAlerter { channel_ids: Arc::new(Mutex::new(None)),
                                       snooze_time: Arc::new(Mutex::new(None)), };

        let mut client = Client::new(&token, Handler).expect("Err creating client");

        // copy reference to shared data into client context
        {
            let mut data = client.data.lock();
            data.insert::<Snooze>(Arc::clone(&alerter.snooze_time));
            data.insert::<Channels>(Arc::clone(&alerter.channel_ids));
        }

        // configure bot
        client.with_framework(StandardFramework::new().configure(|c| c.prefix("!"))
                                                      .cmd("shutup", shutup)
                                                      .cmd("jk", jk));

        // start discord event loop as a seperate thread
        ::std::thread::spawn(move || {
                                 if let Err(why) = client.start() {
                                     println!("Client error: {:?}", why);
                                 }
                             });

        return alerter;
    }
}

impl Alerter for DiscordAlerter
{
    fn send(&self, msg: &Option<String>)
    {
        if should_snooze(&self.snooze_time) {
            return;
        }

        let channel_ids = &*self.channel_ids.lock().unwrap();

        println!("Trying to send message to discord");

        if let Some(msg) = msg {
            if let Some(channel_ids) = channel_ids {
                for channel in channel_ids {
                    let _ = channel.say(&msg);
                }
            }
        }
    }

    fn reset(&mut self)
    {
        *self.snooze_time.lock().unwrap() = None;
    }
}

fn should_snooze(snooze_time: &Arc<Mutex<Option<DateTime<Local>>>>) -> bool
{
    let snooze_time = *snooze_time.lock().unwrap();
    if let Some(snooze_time) = snooze_time {
        let snooze_duration = chrono::Duration::hours(8);
        if Local::now().signed_duration_since(snooze_time) < snooze_duration {
            println!("Not sending message, currently in snooze mode");
            return true;
        }
    }
    return false;
}

command!(shutup(context, msg) {
    let data = context.data.lock();
    match data.get::<Snooze>() {
        Some(snooze) => {
            let mut snooze_time = snooze.lock().unwrap();
            msg.channel_id
                .say("okay okay, ill stop bothering you... for now")?;
            *snooze_time = Some(Local::now());
        }
        None => {
            msg.channel_id.say("There was a problem snoozing the bot.")?;
        }
    }
});

command!(jk(context, msg) {
    let data = context.data.lock();
    match data.get::<Snooze>() {
        Some(snooze) => {
            let mut snooze_time = snooze.lock().unwrap();
            msg.channel_id
                .say("Aye Aye Boss")?;
            *snooze_time = None;
        }
        None => {
            msg.channel_id.say("There was a problem unsnoozing the bot.")?;
        }
    }
});
