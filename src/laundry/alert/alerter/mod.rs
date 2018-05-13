pub trait Alerter
{
    fn send(&self, msg: &Option<String>);
}

pub mod discord;
