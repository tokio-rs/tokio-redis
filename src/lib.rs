#![allow(unused_imports, dead_code)]

extern crate futures;
extern crate tokio_core;
extern crate tokio_io;
extern crate tokio_proto;
extern crate tokio_service;

#[macro_use]
extern crate log;

mod cmd;
mod macros;
mod parser;
mod transport;
mod types;

use std::cell::RefCell;
use std::io;
use std::net::SocketAddr;

use futures::{Async, Future};
use tokio_core::net::TcpStream;
use tokio_core::reactor::Handle;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_proto::TcpClient;
use tokio_proto::pipeline::{ClientProto, ClientService};
use tokio_service::Service;

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

pub struct ClientHandle {
    inner: ClientService<TcpStream, RedisProto>,
}

pub type Response = Box<Future<Item = Value, Error = io::Error>>;

struct RedisProto;

impl<T: AsyncRead + AsyncWrite + 'static> ClientProto<T> for RedisProto {
    type Request = Cmd;
    type Response = Value;
    type Transport = RedisTransport<T>;
    type BindTransport = io::Result<Self::Transport>;

    fn bind_transport(&self, io: T) -> Self::BindTransport {
        Ok(RedisTransport::new(io))
    }
}

impl Client {
    pub fn new() -> Client {
        Client {
            _private: (),
        }
    }

    pub fn connect(self, addr: &SocketAddr, handle: &Handle)
            -> Box<Future<Item = ClientHandle, Error = io::Error>>
    {
        let ret = TcpClient::new(RedisProto)
            .connect(addr, handle)
            .map(|c| ClientHandle { inner: c });

        Box::new(ret)
    }
}

impl ClientHandle {
    /// Get the value of a key.  If key is a vec this becomes an `MGET`.
    pub fn get<K: ToRedisArgs>(&mut self, key: K) -> Response {
        let mut cmd = Cmd::new();
        cmd.arg(if key.is_single_arg() { "GET" } else { "MGET" }).arg(key);

        self.call(cmd)
    }

    /// Set the string value of a key.
    pub fn set<K: ToRedisArgs, V: ToRedisArgs>(&mut self, key: K, value: V) -> Response {
        let mut cmd = Cmd::new();
        cmd.arg("SET").arg(key).arg(value);

        self.call(cmd)
    }
}

impl Service for ClientHandle {
    type Request = Cmd;
    type Response = Value;
    type Error = io::Error;
    type Future = Response;

    fn call(&self, req: Cmd) -> Response {
        Box::new(self.inner.call(req))
    }
}
