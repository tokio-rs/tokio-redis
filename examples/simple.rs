extern crate futures;
extern crate tokio;
extern crate tokio_redis as redis;
extern crate env_logger;

use futures::Future;
use tokio::Service;
use redis::{Client, Cmd};

pub fn main() {
    env_logger::init().unwrap();

    let addr = "127.0.0.1:6379".parse().unwrap();

    let c1 = Client::new()
        .connect(&addr)
        .unwrap();

    let c2 = c1.clone();

    let r = c1.set("zomghi2u", "SOME VALUE")
        .and_then(move |_| c2.get("zomghi2u"))
        ;

    // let resp = client.get("zomghi2u");
    // let resp = client.call("Hello".to_string());

    println!("RESPONSE: {:?}", await(r));
}

// Why this isn't in futures-rs, I do not know...
fn await<T: Future>(f: T) -> Result<T::Item, T::Error> {
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel();

    f.then(move |res| {
        tx.send(res).unwrap();
        Ok::<(), ()>(())
    }).forget();

    rx.recv().unwrap()
}

