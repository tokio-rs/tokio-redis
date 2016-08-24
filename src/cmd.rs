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

impl Cmd {
    /// Creates a new empty command.
    pub fn new() -> Cmd {
        Cmd { args: vec![], cursor: None, is_ignored: false }
    }

    #[inline]
    pub fn arg<T: ToRedisArgs>(&mut self, arg: T) -> &mut Cmd {
        for item in arg.to_redis_args().into_iter() {
            self.args.push(Arg::Simple(item));
        }
        self
    }

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
