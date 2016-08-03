use types::{
    ToRedisArgs,
    FromRedisValue,
    Value,
    RedisResult,
    ErrorKind,
    from_redis_value
};

#[derive(Clone)]
enum Arg<'a> {
    Simple(Vec<u8>),
    Cursor,
    Borrowed(&'a [u8]),
}


/// Represents redis commands.
#[derive(Clone)]
pub struct Cmd {
    args: Vec<Arg<'static>>,
    cursor: Option<u64>,
    is_ignored: bool,
}

/// A command acts as a builder interface to creating encoded redis
/// requests.  This allows you to easiy assemble a packed command
/// by chaining arguments together.
///
/// Basic example:
///
/// ```rust
/// redis::Cmd::new().arg("SET").arg("my_key").arg(42);
/// ```
///
/// There is also a helper function called `cmd` which makes it a
/// tiny bit shorter:
///
/// ```rust
/// redis::cmd("SET").arg("my_key").arg(42);
/// ```
///
/// Because currently rust's currently does not have an ideal system
/// for lifetimes of temporaries, sometimes you need to hold on to
/// the initially generated command:
///
/// ```rust,no_run
/// # let client = redis::Client::open("redis://127.0.0.1/").unwrap();
/// # let con = client.get_connection().unwrap();
/// let mut cmd = redis::cmd("SMEMBERS");
/// let mut iter : redis::Iter<i32> = cmd.arg("my_set").iter(&con).unwrap();
/// ```
impl Cmd {
    /// Creates a new empty command.
    pub fn new() -> Cmd {
        Cmd { args: vec![], cursor: None, is_ignored: false }
    }

    /// Appends an argument to the command.  The argument passed must
    /// be a type that implements `ToRedisArgs`.  Most primitive types as
    /// well as vectors of primitive types implement it.
    ///
    /// For instance all of the following are valid:
    ///
    /// ```rust,no_run
    /// # let client = redis::Client::open("redis://127.0.0.1/").unwrap();
    /// # let con = client.get_connection().unwrap();
    /// redis::cmd("SET").arg(&["my_key", "my_value"]);
    /// redis::cmd("SET").arg("my_key").arg(42);
    /// redis::cmd("SET").arg("my_key").arg(b"my_value");
    /// ```
    #[inline]
    pub fn arg<T: ToRedisArgs>(&mut self, arg: T) -> &mut Cmd {
        for item in arg.to_redis_args().into_iter() {
            self.args.push(Arg::Simple(item));
        }
        self
    }

    /// Works similar to `arg` but adds a cursor argument.  This is always
    /// an integer and also flips the command implementation to support a
    /// different mode for the iterators where the iterator will ask for
    /// another batch of items when the local data is exhausted.
    ///
    /// ```rust,no_run
    /// # let client = redis::Client::open("redis://127.0.0.1/").unwrap();
    /// # let con = client.get_connection().unwrap();
    /// let mut cmd = redis::cmd("SSCAN");
    /// let mut iter : redis::Iter<isize> = cmd.arg("my_set").cursor_arg(0).iter(&con).unwrap();
    /// for x in iter {
    ///     // do something with the item
    /// }
    /// ```
    #[inline]
    pub fn cursor_arg(&mut self, cursor: u64) -> &mut Cmd {
        assert!(!self.in_scan_mode());
        self.cursor = Some(cursor);
        self.args.push(Arg::Cursor);
        self
    }

    /// Returns the packed command as a byte vector.
    #[inline]
    pub fn get_packed_command(&self) -> Vec<u8> {
        encode_command(&self.args, self.cursor.unwrap_or(0))
    }

    /// Like `get_packed_command` but replaces the cursor with the
    /// provided value.  If the command is not in scan mode, `None`
    /// is returned.
    #[inline]
    fn get_packed_command_with_cursor(&self, cursor: u64) -> Option<Vec<u8>> {
        if !self.in_scan_mode() {
            None
        } else {
            Some(encode_command(&self.args, cursor))
        }
    }

    /// Returns true if the command is in scan mode.
    #[inline]
    pub fn in_scan_mode(&self) -> bool {
        self.cursor.is_some()
    }
}

fn encode_command(args: &Vec<Arg>, cursor: u64) -> Vec<u8> {
    let mut totlen = 1 + countdigits(args.len()) + 2;
    for item in args {
        totlen += bulklen(match *item {
            Arg::Cursor => countdigits(cursor as usize),
            Arg::Simple(ref val) => val.len(),
            Arg::Borrowed(ptr) => ptr.len(),
        });
    }

    let mut cmd = Vec::with_capacity(totlen);
    cmd.push('*' as u8);
    cmd.extend(args.len().to_string().as_bytes());
    cmd.push('\r' as u8);
    cmd.push('\n' as u8);

    {
        let mut encode = |item: &[u8]| {
            cmd.push('$' as u8);
            cmd.extend(item.len().to_string().as_bytes());
            cmd.push('\r' as u8);
            cmd.push('\n' as u8);
            cmd.extend(item.iter());
            cmd.push('\r' as u8);
            cmd.push('\n' as u8);
        };

        for item in args.iter() {
            match *item {
                Arg::Cursor => encode(cursor.to_string().as_bytes()),
                Arg::Simple(ref val) => encode(val),
                Arg::Borrowed(ptr) => encode(ptr),
            }
        }
    }

    cmd
}

fn countdigits(mut v: usize) -> usize {
    let mut result = 1;
    loop {
        if v < 10 { return result; }
        if v < 100 { return result + 1; }
        if v < 1000 { return result + 2; }
        if v < 10000 { return result + 3; }

        v /= 10000;
        result += 4;
    }
}

#[inline]
fn bulklen(len: usize) -> usize {
    return 1+countdigits(len)+2+len+2;
}
