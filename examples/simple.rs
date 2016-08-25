extern crate env_logger;
extern crate futures;
extern crate tokio_core;
extern crate tokio_redis as redis;

use futures::Future;
use tokio_core::Loop;
use redis::Client;

pub fn main() {
    env_logger::init().unwrap();

    let addr = "127.0.0.1:6379".parse().unwrap();
    let mut lp = Loop::new().unwrap();

    let client = Client::new().connect(lp.handle(), &addr);
    let c1 = lp.run(client).unwrap();

    let c2 = c1.clone();

    let r = c1.set("zomghi2u", "SOME VALUE")
        .and_then(move |_| c2.get("zomghi2u"))
        ;

    // let resp = client.get("zomghi2u");
    // let resp = client.call("Hello".to_string());

    println!("RESPONSE: {:?}", lp.run(r));
}
