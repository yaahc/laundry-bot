mod laundry;

#[macro_use]
extern crate serenity;

extern crate chrono;
extern crate kankyo;
extern crate typemap;

use laundry::alert::discord::DiscordAlerter;
use laundry::Laundry;
use std::env;

fn main()
{
    kankyo::load().expect("Failed to load .env file");

    let pin: u64 = env::var("VIBRATION_SENSOR_PIN_NUMBER").expect("Expected a pin number in the \
                                                                   environment")
                                                          .parse()
                                                          .expect("pin not a valid int");

    let mut alerter = DiscordAlerter::new();

    let mut laundry = Laundry::new(&mut alerter);

    laundry.laundry_thread(pin);
}
