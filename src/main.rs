extern crate slack;
extern crate sysfs_gpio;

use slack::RtmClient;
use std::time::{Duration, Instant};
use sysfs_gpio::{Direction, Edge, Pin};

#[derive(Debug)]
enum State
{
    ApplianceOn,
    ApplianceOff,
}

fn main()
{
    // Setup slack sender
    let args: Vec<String> = std::env::args().collect();
    let api_key = match args.len() {
        0 | 1 => {
            panic!("No api-key in args! Usage: cargo run --example slack_example -- <api-key>")
        },
        x => args[x - 1].clone(),
    };
    let r = RtmClient::login(&api_key).unwrap();

    // Setup state variables for state machine
    let join_delta = Duration::new(10, 0);
    let mut state = State::ApplianceOff;
    let mut last_active: Option<Instant> = None;
    let mut period_start = None;

    // create alert function
    let send_alert = |start_time: &Instant| {
        let sender = r.sender();
        let msg = format!("Your laundry is done! It took {:?}.",
                          Instant::now().duration_since(*start_time));
        sender.send_message("#laundry", &msg)
              .expect("Failed to send slack message");
    };

    // configure the sensor
    let pin: u64 = 13;
    let input = Pin::new(pin);
    input.with_exported(|| {
                            input.set_direction(Direction::In)?;
                            input.set_edge(Edge::BothEdges)?;
                            let mut poller = input.get_poller()?;

                            // wait for events
                            loop {
                                let event = poller.poll(1000).unwrap();
                                let current_time = Instant::now();

                                match (&state, event) {
                                    (State::ApplianceOff, Some(1)) => {
                                        state = State::ApplianceOn;
                                        period_start = Some(current_time);
                                    },
                                    (State::ApplianceOn, Some(_)) => {
                                        last_active = Some(current_time);
                                    },
                                    (State::ApplianceOn, None) => {
                                        if let Some(t) = last_active {
                                            if t.duration_since(current_time) > join_delta {
                                                send_alert(&period_start.unwrap());
                                            }
                                        }
                                    },
                                    _ => {
                                        println!("{:?} {:?}", state, event);
                                    },
                                }
                            }
                        })
         .expect("Error occured in poller");
}
