pub use miniclap_derive::MiniClap;
use std::error::Error as StdError;
use std::{ffi::OsString, str::FromStr};

pub trait MiniClap: Sized {
    #[inline]
    fn parse_or_exit() -> Self {
        Self::parse_or_exit_from(std::env::args_os())
    }

    #[inline]
    fn parse_or_exit_from<I, T>(args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        match Self::try_parse_from(args) {
            Ok(x) => x,
            Err(e) => e.exit(),
        }
    }

    #[inline]
    fn try_parse() -> Result<Self> {
        Self::try_parse_from(std::env::args_os())
    }

    #[inline]
    fn try_parse_from<I, T>(args: I) -> Result<Self>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        Self::__parse_internal(&mut args.into_iter().map(|x| x.into()))
    }

    fn __parse_internal(args: &mut dyn Iterator<Item = OsString>) -> Result<Self>;
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum ErrorKind {
    ParseFailed,
    UnknownSwitch,
    TooManyPositional,
    MissingRequiredArgument,
    InvalidUtf8,
    Other,
}

#[derive(Debug)]
pub struct Error {
    pub message: String,
    pub kind: ErrorKind,
    pub source: Option<Box<dyn StdError + 'static>>,
}

impl Error {
    pub fn exit(&self) -> ! {
        eprintln!("error: {}", self.message);
        std::process::exit(1)
    }

    pub fn parse_failed(name: &str, err: Box<dyn StdError>) -> Error {
        Error {
            message: format!("Invalid value for '{}': {}", name, err),
            kind: ErrorKind::ParseFailed,
            source: Some(err),
        }
    }

    pub fn unknown_switch(name: &str) -> Error {
        Error {
            message: format!("Did not recognize switched argument '{}'", name),
            kind: ErrorKind::UnknownSwitch,
            source: None,
        }
    }

    pub fn too_many_positional(arg: &str) -> Error {
        Error {
            message: format!("Too many positional arguments, starting with '{}'", arg),
            kind: ErrorKind::TooManyPositional,
            source: None,
        }
    }

    pub fn missing_required_argument(arg_name: &str) -> Error {
        Error {
            message: format!("Missing required argument '{}'", arg_name),
            kind: ErrorKind::MissingRequiredArgument,
            source: None,
        }
    }

    pub fn invalid_utf8() -> Error {
        Error {
            message: "Invalid UTF-8 was detected in one or more arguments".into(),
            kind: ErrorKind::InvalidUtf8,
            source: None,
        }
    }

    pub fn other<I: Into<String>>(message: I) -> Error {
        Error {
            message: message.into(),
            kind: ErrorKind::Other,
            source: None,
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source.as_ref().map(|x| x.as_ref())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

pub struct Value(pub String);

impl Value {
    #[inline]
    pub fn parse<E, F>(&self, name: &str) -> Result<F>
    where
        E: StdError + 'static,
        F: FromStr<Err = E>,
    {
        self.0
            .parse()
            .map_err(|e| Error::parse_failed(name, Box::new(e)))
    }
}

pub enum Arg {
    Switch(String),
    Positional(Value),
}

pub struct Parser<'a> {
    args: &'a mut dyn Iterator<Item = OsString>,
    next_value: Option<String>,
    next_flags: Vec<char>,
    cur_is_flag: bool,
}

impl<'a> Parser<'a> {
    pub fn new(args: &'a mut dyn Iterator<Item = OsString>) -> Parser<'a> {
        Parser {
            args,
            next_value: None,
            next_flags: Vec::new(),
            cur_is_flag: false,
        }
    }

    fn next_in(&mut self) -> Result<Option<String>> {
        match self.args.next() {
            Some(arg_os) => match arg_os.into_string() {
                Ok(arg) => Ok(Some(arg)),
                _ => Err(Error::invalid_utf8()),
            },
            None => Ok(None),
        }
    }

    pub fn next_arg(&mut self) -> Result<Option<Arg>> {
        if let Some(ref value) = self.next_value {
            panic!("Next value was not consumed: '{}'", value);
        }
        if let Some(flag) = self.next_flags.pop() {
            self.cur_is_flag = true;
            return Ok(Some(Arg::Switch(format!("-{}", flag))));
        }
        self.cur_is_flag = false;
        let arg = match self.next_in()? {
            Some(arg) => arg,
            None => return Ok(None),
        };
        let mut chars = arg.chars();
        let res = match (chars.next(), chars.next(), chars.as_str()) {
            (Some('-'), Some('-'), "") => todo!("Trailing args"),
            (Some('-'), Some('-'), arg) => {
                let (arg, opt_value) = match arg.find('=') {
                    Some(i) => {
                        let (x, y) = arg.split_at(i);
                        (x, Some(y[1..].to_string()))
                    }
                    None => (arg, None),
                };
                self.next_value = opt_value;
                Arg::Switch(format!("--{}", arg))
            }
            (Some('-'), Some(c), rest) => {
                if rest.chars().next() == Some('=') {
                    self.next_value = Some(rest[3..].to_string());
                } else if rest.contains('=') {
                    return Err(Error::other(
                        "Can't have multiple flags and '=' in same argument",
                    ));
                } else {
                    self.next_flags.extend(rest.chars().rev());
                }
                Arg::Switch(format!("-{}", c))
            }
            _ => Arg::Positional(Value(arg)),
        };
        Ok(Some(res))
    }

    pub fn next_value(&mut self, name: &str) -> Result<String> {
        if self.cur_is_flag {
            Err(Error::other("Option used in a combined flag"))
        } else if !self.next_flags.is_empty() {
            Ok(self.next_flags.drain(..).rev().collect())
        } else if let Some(value) = self.next_value.take() {
            Ok(value)
        } else if let Some(value) = self.next_in()? {
            Ok(value)
        } else {
            Err(Error::missing_required_argument(name))
        }
    }

    pub fn parse_next<E, F>(&mut self, name: &str) -> Result<F>
    where
        E: StdError + 'static,
        F: FromStr<Err = E>,
    {
        Value(self.next_value(name)?).parse(name)
    }
}
