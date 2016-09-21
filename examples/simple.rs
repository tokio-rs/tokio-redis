extern crate env_logger;
extern crate futures;
extern crate tokio_core;
extern crate tokio_redis as redis;

use futures::Future;
use tokio_core::reactor::Core;
use redis::Client;

pub fn main() {
    env_logger::init().unwrap();

    let addr = "127.0.0.1:6379".parse().unwrap();
    let mut lp = Core::new().unwrap();

    let client = Client::new().connect(&lp.handle(), &addr);

    let r1 = client.set("zomghi2u", "SOME VALUE");
    let r2 = r1.and_then(move |_| client.get("zomghi2u"));
    let val = lp.run(r2).unwrap();

    println!("RESPONSE: {:?}", val);
}
