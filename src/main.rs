#[macro_use]
extern crate if_chain;
#[macro_use]
extern crate log;

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
use serenity::prelude::*;
use std::collections::HashSet;
use std::env;
use std::sync::{Arc, Mutex};
use sysfs_gpio::{Direction, Edge, Pin};
use typemap::Key;

#[derive(Debug)]
enum State {
    ApplianceOn,
    ApplianceWaitingForUnload,
    ApplianceOff,
}

#[derive(Debug)]
enum Event {
    Vibrated,
    TimedOut,
    PollerError,
}

struct Snooze;

impl Key for Snooze {
    type Value = Arc<Mutex<Option<DateTime<Local>>>>;
}

struct Handler;

impl EventHandler for Handler {
    fn ready(&self, context: Context, ready: Ready) {
        info!("Connected as {} {:#?}", ready.user.name, ready);

        let data = context.data.lock();
        let snooze_time = Arc::clone(data.get::<Snooze>().unwrap());

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

        // spawn laundry watching thread
        std::thread::spawn(move || {
            // Setup state variables for state machine
            let join_delta = chrono::Duration::seconds(10);
            let mut state = State::ApplianceOff;
            let mut last_active: Option<DateTime<Local>> = None;
            let mut period_start = None;

            let should_nag = true;
            let nag_frequency = chrono::Duration::minutes(5);
            let snooze_duration = chrono::Duration::hours(8);
            let mut last_message_time = None;

            // create alert function
            let send_alert =
                |start_time: &DateTime<Local>, last_message_time: &mut Option<DateTime<Local>>| {
                    debug!("Trying to send message to discord");

                    if let Some(snooze_time) = *snooze_time.lock().unwrap() {
                        if Local::now().signed_duration_since(snooze_time) < snooze_duration {
                            debug!("Not sending message, currently in snooze mode");
                            return;
                        }
                    }

                    let formated_time = format!("{}", (*start_time).format("%-l:%M %P"));
                    let msg = format!(
                        "Your laundry is done! It started at {} and ran for {} minutes.",
                        formated_time,
                        Local::now()
                            .signed_duration_since(*start_time)
                            .num_minutes()
                    );

                    for channel in &channel_ids {
                        let _ = channel.say(&msg);
                    }

                    *last_message_time = Some(Local::now());
                };

            // configure the sensor
            let pin: u64 = 14;
            let input = Pin::new(pin);
            input
                .with_exported(|| {
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

                        let current_time = Local::now();
                        debug!("{:?} {:?}", state, event);

                        match (&state, event) {
                            (State::ApplianceOff, Event::Vibrated) => {
                                state = State::ApplianceOn;
                                period_start = Some(current_time);
                            }
                            (State::ApplianceOn, Event::Vibrated) => {
                                last_active = Some(current_time);
                            }
                            (State::ApplianceOn, Event::TimedOut) => {
                                if_chain! {
                                    if let Some(stop) = last_active;
                                    if current_time.signed_duration_since(stop) > join_delta;
                                    if let Some(start) = period_start;
                                    if current_time.signed_duration_since(start) > chrono::Duration::minutes(10);
                                    then {
                                        state = State::ApplianceWaitingForUnload;
                                        send_alert(&start, &mut last_message_time);
                                    } else {
                                        state = State::ApplianceOff;
                                        info!("I dont think that was actually a load of laundry");
                                    }
                                }
                            }
                            (State::ApplianceWaitingForUnload, Event::TimedOut) => {
                                if_chain! {
                                    if should_nag;
                                    if let Some(lmt) = last_message_time;
                                    if current_time.signed_duration_since(lmt) > nag_frequency;
                                    if let Some(start) = period_start;
                                    then {
                                        send_alert(&start, &mut last_message_time);
                                    }
                                }
                            }
                            // Assumes someone came along and unloaded laundry
                            (State::ApplianceWaitingForUnload, Event::Vibrated) => {
                                state = State::ApplianceOff;
                            }
                            _ => {}
                        }
                    }
                })
                .expect("Error occured in poller loop");
        });
    }
}

fn main() {
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
    }

    let owners = match http::get_current_application_info() {
        Ok(info) => {
            let mut set = HashSet::new();
            set.insert(info.owner.id);

            set
        }
        Err(why) => panic!("Couldn't get application info: {:?}", why),
    };

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
            }),
    );

    if let Err(why) = client.start() {
        error!("Client error: {:?}", why);
    }
}
