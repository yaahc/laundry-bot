extern crate chrono;
extern crate env_logger;
extern crate kankyo;
extern crate serenity;
extern crate sysfs_gpio;
extern crate typemap;

use chrono::prelude::*;
use serenity::framework::StandardFramework;
use serenity::http;
use serenity::model::gateway::Ready;
use serenity::model::id::ChannelId;
use serenity::prelude::*;
use std::collections::HashSet;
use std::env;
use std::sync::{Arc, Mutex};
use sysfs_gpio::{Direction, Edge, Pin};
use typemap::Key;

#[derive(Debug, Clone)]
enum Appliance
{
    On,
    WaitingForUnload,
    Off,
}

#[derive(Debug)]
enum Event
{
    Vibrated,
    TimedOut,
    PollerError,
}

#[derive(Debug)]
struct State
{
    state: Appliance,
    start: Option<DateTime<Local>>,
    stop: Option<DateTime<Local>>,
    last_msg: Option<DateTime<Local>>,
    snooze_time: Arc<Mutex<Option<DateTime<Local>>>>,
    channel_ids: Arc<Mutex<Option<Vec<ChannelId>>>>,
}

impl Default for State
{
    fn default() -> State
    {
        State { state: Appliance::Off,
                start: None,
                stop: None,
                last_msg: None,
                snooze_time: Arc::new(Mutex::new(None)),
                channel_ids: Arc::new(Mutex::new(None)), }
    }
}

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

fn send_alert(channel_ids: &Arc<Mutex<Option<Vec<ChannelId>>>>,
              snooze_time: &Arc<Mutex<Option<DateTime<Local>>>>,
              msg: &str)
{
    let channel_ids = &*channel_ids.lock().unwrap();
    let snooze_time = *snooze_time.lock().unwrap();

    println!("Trying to send message to discord");

    if let Some(snooze_time) = snooze_time {
        let snooze_duration = chrono::Duration::hours(8);
        if Local::now().signed_duration_since(snooze_time) < snooze_duration {
            println!("Not sending message, currently in snooze mode");
            return;
        }
    }

    if let Some(channel_ids) = channel_ids {
        for channel in channel_ids {
            let _ = channel.say(&msg);
        }
    }
}

fn step(data: &mut State, event: &Event)
{
    // consts
    let should_nag = true;
    let nag_frequency = chrono::Duration::minutes(5);
    let join_delta = chrono::Duration::seconds(10);

    let current_time = Local::now();
    println!("{:?} {:?}", data, event);

    match ((*data).state.clone(), event) {
        (Appliance::Off, Event::Vibrated) => {
            (*data).state = Appliance::On;
            (*data).start = Some(current_time);
            (*data).stop = Some(current_time);
        },
        (Appliance::On, Event::Vibrated) => {
            (*data).stop = Some(current_time);
        },
        (Appliance::On, Event::TimedOut) => {
            println!("{:#?} {:#?} {:#?} {:#?}",
                     (*data).stop,
                     current_time,
                     join_delta,
                     (*data).start);
            let stop = (*data).stop.unwrap();
            let start = (*data).start.unwrap();
            if current_time.signed_duration_since(stop) > join_delta {
                if current_time.signed_duration_since(start) > chrono::Duration::minutes(10) {
                    (*data).state = Appliance::WaitingForUnload;

                    let formated_time = format!("{}", (*data).start.unwrap().format("%-l:%M %P"));
                    let msg = format!("@everyone Your laundry is done! It started at {} and ran \
                                       for {} minutes.",
                                      formated_time,
                                      stop.signed_duration_since(start).num_minutes());

                    send_alert(&data.channel_ids, &data.snooze_time, &msg);
                    (*data).last_msg = Some(current_time);
                }
                else {
                    (*data).state = Appliance::Off;
                    println!("I dont think that was actually a load of laundry");
                }
            }
        },
        (Appliance::WaitingForUnload, Event::TimedOut) => {
            let lmt = (*data).last_msg.unwrap();
            if should_nag && current_time.signed_duration_since(lmt) > nag_frequency {
                let wait_time = current_time.signed_duration_since((*data).stop.unwrap())
                                            .num_minutes();
                let msg = format!("@everyone, hey seriously, your laundry is done... its been \
                                   sitting there for {} minutes",
                                  wait_time);
                send_alert(&data.channel_ids, &data.snooze_time, &msg);
                (*data).last_msg = Some(current_time);
            }
        },
        // Assumes someone came along and unloaded laundry
        (Appliance::WaitingForUnload, Event::Vibrated) => {
            // TODO do default
            (*data).state = Appliance::Off;
            (*data).start = None;
            (*data).stop = None;
            {
                let mut snooze_time = (*data).snooze_time.lock().unwrap();
                *snooze_time = None; // reset snooze on unload
            }
            let msg = format!("Alright boss, looks like you unloaded the laundry, back to the \
                               wall, my watch continues...");
            send_alert(&data.channel_ids, &data.snooze_time, &msg);
        },
        _ => {},
    }
}

fn main()
{
    // This will load the environment variables located at `./.env`, relative to
    // the CWD. See `./.env.example` for an example on how to structure this.
    kankyo::load().expect("Failed to load .env file");

    // Initialize the logger to use environment variables.
    //
    // In this case, a good default is setting the environment variable
    // `RUST_LOG` to debug`.
    env_logger::init().expect("Failed to initialize env_logger");

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let mut client = Client::new(&token, Handler).expect("Err creating client");

    {
        let mut data = client.data.lock();
        data.insert::<Snooze>(Arc::new(Mutex::new(None)));
        data.insert::<Channels>(Arc::new(Mutex::new(None)));
    }

    let owners = match http::get_current_application_info() {
        Ok(info) => {
            let mut set = HashSet::new();
            set.insert(info.owner.id);

            set
        },
        Err(why) => panic!("Couldn't get application info: {:?}", why),
    };

    // TODO cleanup and use command macros
    client.with_framework(
        StandardFramework::new()
        .configure(|c| c.owners(owners).prefix("!"))
        .on("shutup", |context, msg, _| {
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
            Ok(())
        })
        .on("jk", |context, msg, _| {
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
            Ok(())
        }),
        );

    {
        let data = client.data.lock();
        let snooze_time = Arc::clone(data.get::<Snooze>().unwrap());
        let channel_ids = Arc::clone(data.get::<Channels>().unwrap());

        let laundry_thread = move || {
            // Setup state variables for state machine
            let mut data = State { snooze_time: snooze_time,
                                   channel_ids: channel_ids,
                                   ..Default::default() };

            // create alert function
            let pin: u64 = 14;
            let input = Pin::new(pin);
            let vibration_loop = || {
                // configure the sensor
                input.set_direction(Direction::In)?;
                input.set_edge(Edge::BothEdges)?;
                let mut poller = input.get_poller()?;

                // wait for events
                loop {
                    // triage the event
                    let event = match poller.poll(1000) {
                        Ok(Some(_)) => Event::Vibrated,
                        Ok(None) => Event::TimedOut,
                        _ => Event::PollerError,
                    };

                    step(&mut data, &event);
                }
            };
            input.with_exported(vibration_loop)
                 .expect("Error occured in poller loop");
        };

        std::thread::spawn(laundry_thread);
    }

    // spawn laundry watching thread

    if let Err(why) = client.start() {
        println!("Client error: {:?}", why);
    }
}
