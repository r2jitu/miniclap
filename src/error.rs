use crate::Switch;
use std::error::Error as StdError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum ErrorKind {
    ParseFailed,
    UnknownSwitch,
    TooManyPositional,
    MissingRequiredArgument,
    MissingValue,
    UnexpectedValue,
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

    pub fn unknown_switch(switch: Switch) -> Error {
        Error {
            message: format!("Did not recognize argument '{}'", switch),
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

    pub fn missing_value(switch: Switch) -> Error {
        Error {
            message: format!("Missing value for '{}'", switch),
            kind: ErrorKind::MissingValue,
            source: None,
        }
    }

    pub fn unexpected_value(switch: Switch) -> Error {
        Error {
            message: format!("Flag '{}' cannot take a value", switch),
            kind: ErrorKind::UnexpectedValue,
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
