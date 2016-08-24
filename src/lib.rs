extern crate tokio;
extern crate mio;

#[macro_use]
extern crate log;

mod cmd;
mod macros;
mod parser;
mod transport;
mod types;

use transport::RedisTransport;
use tokio::Service;
use tokio::reactor::{Reactor, ReactorHandle};
use tokio::proto::pipeline;
use tokio::tcp::TcpStream;
use tokio::util::future::{Empty, Val};
use std::io;
use std::net::SocketAddr;

pub use cmd::Cmd;

pub use types::{
    /* low level values */
    Value,

    /* error and result types */
    RedisError as Error,

    /* error kinds */
    ErrorKind,

    RedisResult as Result,

    /* conversion traits */
    FromRedisValue,
    ToRedisArgs,
};

pub struct Client {
    reactor: Option<ReactorHandle>,
}

#[derive(Clone)]
pub struct ClientHandle {
    inner: pipeline::Client<Cmd, Value, Empty<(), Error>, Error>,
}

pub type Response = Val<Value, Error>;

impl Client {
    pub fn new() -> Client {
        Client {
            reactor: None,
        }
    }

    pub fn connect(self, addr: &SocketAddr) -> io::Result<ClientHandle> {
        let reactor = match self.reactor {
            Some(r) => r,
            None => {
                let reactor = try!(Reactor::default());
                let handle = reactor.handle();
                reactor.spawn();
                handle
            },
        };

        let addr = addr.clone();

        // Connect the client
        let client = pipeline::connect(&reactor, move || {
            let stream = try!(TcpStream::connect(&addr));
            Ok(RedisTransport::new(stream))
        });

        Ok(ClientHandle { inner: client })
    }
}

impl ClientHandle {
    /// Get the value of a key.  If key is a vec this becomes an `MGET`.
    pub fn get<K: ToRedisArgs>(&self, key: K) -> Response {
        let mut cmd = Cmd::new();
        cmd.arg(if key.is_single_arg() { "GET" } else { "MGET" }).arg(key);

        self.call(cmd)
    }

    /// Set the string value of a key.
    pub fn set<K: ToRedisArgs, V: ToRedisArgs>(&self, key: K, value: V) -> Response {
        let mut cmd = Cmd::new();
        cmd.arg("SET").arg(key).arg(value);

        self.call(cmd)
    }
}

impl Service for ClientHandle {
    type Req = Cmd;
    type Resp = Value;
    type Error = Error;
    type Fut = Response;

    fn call(&self, req: Cmd) -> Response {
        self.inner.call(pipeline::Message::WithoutBody(req))
    }
}
