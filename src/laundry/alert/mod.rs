pub mod alerter;

extern crate chrono;

use chrono::prelude::*;

pub fn laundry_done(start: &DateTime<Local>, stop: &DateTime<Local>) -> Option<String>
{
    let formated_time = format!("{}", start.format("%-l:%M %P"));
    let msg = format!("@everyone Your laundry is done! It started at {} and ran for {} minutes.",
                      formated_time,
                      stop.signed_duration_since(*start).num_minutes());

    return Some(msg);
}

pub fn please_unload(current_time: &DateTime<Local>,
                     stop: &Option<DateTime<Local>>)
                     -> Option<String>
{
    if let Some(stop) = stop {
        let wait_time = current_time.signed_duration_since(*stop).num_minutes();
        let msg = format!("@everyone, hey seriously, your laundry is done... its been sitting \
                           there for {} minutes",
                          wait_time);

        return Some(msg);
    }
    else {
        return None;
    }
}

pub fn finally_unloaded() -> Option<String>
{
    let msg = format!("Alright boss, looks like you unloaded the laundry, back to the wall, my \
                       watch continues...");

    return Some(msg);
}
