extern crate chrono;
extern crate slack_api as slack;
extern crate sysfs_gpio;

use chrono::prelude::*;
use sysfs_gpio::{Direction, Edge, Pin};

#[derive(Debug)]
enum State {
    ApplianceOn,
    ApplianceOff,
}

fn main() {
    // Setup slack sender
    let args: Vec<String> = std::env::args().collect();
    let api_key = match args.len() {
        0 | 1 => {
            panic!("No api-key in args! Usage: cargo run --example slack_example -- <api-key>")
        }
        x => args[x - 1].clone(),
    };

    let client = slack::requests::default_client().unwrap();

    // Setup state variables for state machine
    let join_delta = chrono::Duration::seconds(10);
    let mut state = State::ApplianceOff;
    let mut last_active: Option<DateTime<Local>> = None;
    let mut period_start = None;

    // create alert function
    let send_alert = |start_time: &DateTime<Local>| {
        println!("Trying to send message to slack");

        let msg = format!(
            "Your laundry is done! It started at {:?} and ran for {:?}",
            *start_time,
            Local::now().signed_duration_since(*start_time)
        );

        let msg_request = slack::chat::PostMessageRequest {
            channel: "CAH46G230",
            text: &msg,
            parse: None,
            link_names: None,
            attachments: None,
            unfurl_links: None,
            unfurl_media: None,
            username: Some("LaundroBot"),
            as_user: None,
            icon_url: None,
            icon_emoji: None,
            thread_ts: None,
            reply_broadcast: None,
        };

        slack::chat::post_message(&client, &api_key, &msg_request).expect("or not");
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
                let event = poller.poll(1000).unwrap();
                let current_time = Local::now();
                println!("{:?} {:?}", state, event);

                match (&state, event) {
                    (State::ApplianceOff, Some(1)) => {
                        state = State::ApplianceOn;
                        period_start = Some(current_time);
                    }
                    (State::ApplianceOn, Some(_)) => {
                        last_active = Some(current_time);
                    }
                    (State::ApplianceOn, None) => {
                        if let Some(t) = last_active {
                            if current_time.signed_duration_since(t) > join_delta {
                                state = State::ApplianceOff;
                                if current_time.signed_duration_since(period_start.unwrap())
                                    > chrono::Duration::minutes(10)
                                {
                                    send_alert(&period_start.unwrap());
                                } else {
                                    println!("I dont think that was actually a load of laundry");
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        })
        .expect("Error occured in poller");
}
