extern crate tokio_proto;
extern crate futures;
extern crate tokio_core;

#[macro_use]
extern crate log;

mod cmd;
mod macros;
mod parser;
mod transport;
mod types;

use std::cell::RefCell;
use std::net::SocketAddr;

use futures::stream::Receiver;
use futures::{Future, BoxFuture};
use tokio_core::LoopHandle;
use tokio_core::io::IoFuture;
use tokio_proto::Service;
use tokio_proto::proto::pipeline;

use transport::RedisTransport;

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
    _private: (),
}

#[derive(Clone)]
pub struct ClientHandle {
    inner: pipeline::Client<Cmd, Value, Receiver<(), Error>, Error>,
}

pub type Response = BoxFuture<Value, Error>;

impl Client {
    pub fn new() -> Client {
        Client {
            _private: (),
        }
    }

    pub fn connect(self,
                   handle: LoopHandle,
                   addr: &SocketAddr) -> IoFuture<ClientHandle> {
        handle.clone().tcp_connect(addr).map(|tcp| {
            let tcp = RefCell::new(Some(tcp));
            let client = pipeline::connect(handle, move || {
                Ok(RedisTransport::new(tcp.borrow_mut().take().unwrap()))
            });

            ClientHandle { inner: client }
        }).boxed()
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
