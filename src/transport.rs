use {Cmd, Value};
use parser::Parser;
use types::RedisError;
use tokio_proto::io::{Readiness, Transport};
use tokio_proto::pipeline::{self, Frame};
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

pub type ReqFrame = Frame<Cmd, RedisError>;
pub type RespFrame = Frame<Value, RedisError>;

impl<T> RedisTransport<T>
    where T: io::Read + io::Write + Readiness,
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
    where T: io::Read + io::Write + Readiness,
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

impl<T> Transport for RedisTransport<T>
    where T: io::Read + io::Write + Readiness
{
    type In = ReqFrame;
    type Out = RespFrame;

    /// Read a message from the `Transport`
    fn read(&mut self) -> io::Result<Option<RespFrame>> {
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
                Ok(val) => Ok(Some(Frame::Message(val))),
                Err(e) => e.into(),
            }
        };

        match ret {
            Ok(None) => {},
            _ => {
                // Data is consumed
                let tail = self.rd.split_off(pos);
                mem::replace(&mut self.rd, tail);
            }
        }

        ret
    }

    /// Write a message to the `Transport`
    fn write(&mut self, req: ReqFrame) -> io::Result<Option<()>> {
        match req {
            Frame::Message(cmd) => {
                // Queue the command to be written
                self.cmds.push(cmd);

                // Try to flush the write queue
                self.flush()
            },
            Frame::MessageWithBody(..) => unimplemented!(),
            Frame::Body(..) => unimplemented!(),
            Frame::Error(_) => unimplemented!(),
            Frame::Done => unimplemented!(),
        }
    }

    /// Flush pending writes to the socket
    fn flush(&mut self) -> io::Result<Option<()>> {
        loop {
            // If the current write buf is empty, try to refill it
            if self.wr_is_empty() {
                // If there are no pending commands, then all commands have
                // been fully written
                if self.cmds.is_empty() {
                    return Ok(Some(()));
                }

                // Get the next command
                let cmd = self.cmds.remove(0);

                // Queue it for writting
                self.wr = Cursor::new(cmd.get_packed_command());
            }

            // Try to write the remaining buffer
            if !try!(self.wr_flush()) {
                return Ok(None);
            }
        }
    }
}

impl<T> Readiness for RedisTransport<T>
    where T: Readiness
{
    fn is_readable(&self) -> bool {
        self.inner.is_readable()
    }

    fn is_writable(&self) -> bool {
        // Always allow writing... this isn't really the best strategy to do in
        // practice, but it is the easiest to implement in this case. The
        // number of in-flight requests can be controlled using the pipeline
        // dispatcher.
        true
    }
}
