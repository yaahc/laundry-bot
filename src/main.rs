mod laundry;

#[macro_use]
extern crate serenity;

extern crate chrono;
extern crate kankyo;
extern crate typemap;

use laundry::alert::alerter::discord::DiscordAlerter;
use std::env;

fn main()
{
    kankyo::load().expect("Failed to load .env file");

    let pin: u64 = env::var("VIBRATION_SENSOR_PIN_NUMBER").expect("Expected a pin number in the \
                                                                   environment")
                                                          .parse()
                                                          .expect("pin not a valid int");

    let alerter = DiscordAlerter::new();

    let mut state = laundry::State::new(&alerter);

    state.laundry_thread(pin);
}
