use {Cmd, Value};
use parser::Parser;
use types::RedisError;
use tokio_io::{AsyncRead, AsyncWrite};
use futures::{Async, AsyncSink, Poll, Stream, Sink, StartSend};
use std::mem;
use std::io::{self, Cursor};

/// Line transport
pub struct RedisTransport<T> {
    // Inner socket
    inner: T,
    // Set to true when inner.read returns Ok(0);
    done: bool,
    // Buffered read data
    rd: Vec<u8>,
    // Current buffer to write to the socket
    wr: io::Cursor<Vec<u8>>,
    // Queued commands
    cmds: Vec<Cmd>,
}

struct RedisProto;

// pub type ReqFrame = Frame<Cmd, (), RedisError>;
// pub type RespFrame = Frame<Value, (), RedisError>;

impl<T> RedisTransport<T>
    where T: AsyncRead + AsyncWrite,
{
    pub fn new(inner: T) -> RedisTransport<T> {
        RedisTransport {
            inner: inner,
            done: false,
            rd: vec![],
            wr: io::Cursor::new(vec![]),
            cmds: vec![],
        }
    }
}

impl<T> RedisTransport<T>
    where T: AsyncRead + AsyncWrite,
{
    fn wr_is_empty(&self) -> bool {
        self.wr_remaining() == 0
    }

    fn wr_remaining(&self) -> usize {
        self.wr.get_ref().len() - self.wr_pos()
    }

    fn wr_pos(&self) -> usize {
        self.wr.position() as usize
    }

    fn wr_flush(&mut self) -> io::Result<bool> {
        // Making the borrow checker happy
        let res = {
            let buf = {
                let pos = self.wr.position() as usize;
                let buf = &self.wr.get_ref()[pos..];

                trace!("writing; remaining={:?}", buf);

                buf
            };

            self.inner.write(buf)
        };

        match res {
            Ok(mut n) => {
                n += self.wr.position() as usize;
                self.wr.set_position(n as u64);
                Ok(true)
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::WouldBlock {
                    return Ok(false);
                }

                trace!("transport flush error; err={:?}", e);
                return Err(e)
            }
        }
    }
}

impl<T> Stream for RedisTransport<T>
    where T: AsyncRead + AsyncWrite,
{
    type Item = Value;
    type Error = io::Error;

    /// Read a message from the `Transport`
    fn poll(&mut self) -> Poll<Option<Value>, io::Error> {
        // Not at all a smart implementation, but it gets the job done.

        // First fill the buffer
        while !self.done {
            match self.inner.read_to_end(&mut self.rd) {
                Ok(0) => {
                    self.done = true;
                    break;
                }
                Ok(_) => {}
                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        break;
                    }

                    return Err(e)
                }
            }
        }

        // Try to parse some data!
        let pos;
        let ret = {
            let mut cursor = Cursor::new(&self.rd);
            let res = {
                let mut parser = Parser::new(&mut cursor);
                parser.parse_value()
            };
            pos = cursor.position() as usize;

            match res {
                Ok(val) => Ok(Async::Ready(Some(val))),
                Err(e) => e.into(),
            }
        };

        match ret {
            Ok(Async::NotReady) => {},
            _ => {
                // Data is consumed
                let tail = self.rd.split_off(pos);
                mem::replace(&mut self.rd, tail);
            }
        }

        ret
    }
}

impl<T> Sink for RedisTransport<T>
    where T: AsyncRead + AsyncWrite,
{
    type SinkItem = Cmd;
    type SinkError = io::Error;

    /// Write a message to the `Transport`
    fn start_send(&mut self, cmd: Cmd) -> StartSend<Cmd, io::Error> {
        self.cmds.push(cmd);
        Ok(AsyncSink::Ready)
    }

    /// Flush pending writes to the socket
    fn poll_complete(&mut self) -> Poll<(), io::Error> {
        loop {
            // If the current write buf is empty, try to refill it
            if self.wr_is_empty() {
                // If there are no pending commands, then all commands have
                // been fully written
                if self.cmds.is_empty() {
                    return Ok(Async::Ready(()));
                }

                // Get the next command
                let cmd = self.cmds.remove(0);

                // Queue it for writting
                self.wr = Cursor::new(cmd.get_packed_command());
            }

            // Try to write the remaining buffer
            if !try!(self.wr_flush()) {
                return Ok(Async::NotReady);
            }
        }
    }
}
