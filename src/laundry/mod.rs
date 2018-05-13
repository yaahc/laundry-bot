pub mod alert;

extern crate chrono;
extern crate sysfs_gpio;

use self::alert::alerter::Alerter;
use self::sysfs_gpio::{Direction, Edge, Pin};
use chrono::prelude::*;

#[derive(Debug, Clone)]
enum Appliance
{
    On,
    WaitingForUnload,
    Off,
}

#[derive(Debug)]
pub enum Event
{
    Vibrated,
    TimedOut,
    PollerError,
}

// todo split up all state stuff
pub struct State<'a>
{
    state: Appliance,
    start: Option<DateTime<Local>>,
    stop: Option<DateTime<Local>>,
    last_msg: Option<DateTime<Local>>,
    alerter: &'a Alerter,
}

impl<'a> State<'a>
{
    pub fn new(alerter: &Alerter) -> State
    {
        State { state: Appliance::Off,
                start: None,
                stop: None,
                last_msg: None,
                alerter: alerter, }
    }

    pub fn laundry_thread(&mut self, pin: u64)
    {
        // create alert function
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

                self.step(&event);
            }
        };
        input.with_exported(vibration_loop)
             .expect("Error occured in poller loop");
    }

    fn step(&mut self, event: &Event)
    {
        let current_time = Local::now();
        println!("{:?} {:?}", self.state, event);

        match (self.state.clone(), event) {
            (Appliance::Off, Event::Vibrated) => {
                self.start_load(&current_time);
            },
            (Appliance::On, Event::Vibrated) => {
                self.stop = Some(current_time); // record recent vibration
            },
            (Appliance::On, Event::TimedOut) => {
                self.maybe_end_load(&current_time);
            },
            (Appliance::WaitingForUnload, Event::TimedOut) => {
                self.maybe_nag(&current_time);
            },
            // Assumes someone came along and unloaded laundry
            (Appliance::WaitingForUnload, Event::Vibrated) => {
                self.reset_load();
            },
            _ => {},
        }
    }

    fn start_load(&mut self, current_time: &DateTime<Local>)
    {
        self.state = Appliance::On;
        self.start = Some(*current_time);
        self.stop = Some(*current_time);
    }

    fn maybe_end_load(&mut self, current_time: &DateTime<Local>)
    {
        let join_delta = chrono::Duration::seconds(10);

        let start = self.start.unwrap();
        let stop = self.stop.unwrap();

        if current_time.signed_duration_since(stop) > join_delta {
            if current_time.signed_duration_since(start) > chrono::Duration::minutes(10) {
                self.state = Appliance::WaitingForUnload;
                self.alerter.send(&alert::laundry_done(&start, &stop));
                self.last_msg = Some(*current_time);
            }
            else {
                self.state = Appliance::Off;
                println!("I dont think that was actually a load of laundry");
            }
        }
    }

    fn maybe_nag(&mut self, current_time: &DateTime<Local>)
    {
        // consts
        let should_nag = true;
        let nag_frequency = chrono::Duration::minutes(5);

        let lmt = self.last_msg.unwrap();
        if should_nag && current_time.signed_duration_since(lmt) > nag_frequency {
            self.alerter.send(&alert::please_unload(&current_time, &self.stop));
            self.last_msg = Some(*current_time);
        }
    }

    fn reset_load(&mut self)
    {
        self.alerter.send(&alert::finally_unloaded());

        self.state = Appliance::Off;
        self.start = None;
        self.stop = None;
        {
            // TODO reset snooze time
            // let mut snooze_time = self.snooze_time.lock().unwrap();
            // *snooze_time = None; // reset snooze on unload
        }
    }
}
