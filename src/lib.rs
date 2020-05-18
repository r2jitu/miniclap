pub use miniclap_derive::MiniClap;
use std::error::Error as StdError;
use std::ffi::OsString;

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

/// Split a switch argument at '=' if it exists.
pub fn __split_arg_value(arg: &mut &str) -> Option<String> {
    arg.find('=').map(|i| {
        let (x, y) = arg.split_at(i);
        *arg = x;
        y[1..].into()
    })
}

/// Gets an argument value from after '=' or from the next argument in the list.
pub fn __get_value(
    name: &str,
    value: Option<String>,
    args: &mut dyn Iterator<Item = ::std::ffi::OsString>,
) -> Result<String> {
    value.map_or_else(
        || {
            let value_os = args
                .next()
                .ok_or_else(|| Error::missing_required_argument(name))?;
            value_os.into_string().map_err(|_| Error::invalid_utf8())
        },
        Ok,
    )
}
