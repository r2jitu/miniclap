pub use miniclap_derive::MiniClap;
use std::error::Error as StdError;
use std::{cell::RefCell, ffi::OsString, marker::PhantomData, str::FromStr};

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

    pub fn unknown_switch(name: &str) -> Error {
        Error {
            message: format!("Did not recognize argument '{}'", name),
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

    pub fn unexpected_value(arg_name: &str) -> Error {
        Error {
            message: format!("Flag '{}' cannot take a value", arg_name),
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

pub struct ArgHandlers<'a> {
    pub flags: &'a [FlagHandler<'a>],
    pub options: &'a [OptionHandler<'a>],
    pub positions: &'a [PositionalHandler<'a>],
}

pub struct FlagHandler<'a> {
    pub name: &'a str,
    pub short: Option<char>,
    pub long: Option<&'a str>,
    pub assign: &'a RefCell<dyn FnMut() -> Result<()> + 'a>,
}

pub struct OptionHandler<'a> {
    pub name: &'a str,
    pub short: Option<char>,
    pub long: Option<&'a str>,
    pub assign: &'a dyn Assign,
}

pub struct PositionalHandler<'a> {
    pub name: &'a str,
    pub is_multiple: bool,
    pub assign: &'a dyn Assign,
}

impl<'a> ArgHandlers<'a> {
    fn flag_by_short(&self, c: char) -> Option<&FlagHandler<'a>> {
        self.flags.iter().find(|h| h.short == Some(c))
    }

    fn flag_by_long(&self, l: &str) -> Option<&FlagHandler<'a>> {
        self.flags.iter().find(|h| h.long == Some(l))
    }

    fn option_by_short(&self, c: char) -> Option<&OptionHandler<'a>> {
        self.options.iter().find(|h| h.short == Some(c))
    }

    fn option_by_long(&self, l: &str) -> Option<&OptionHandler<'a>> {
        self.options.iter().find(|h| h.long == Some(l))
    }
}

fn next_value(name: &str, args: &mut dyn Iterator<Item = ::std::ffi::OsString>) -> Result<String> {
    match args.next().map(OsString::into_string) {
        Some(Ok(value)) => Ok(value),
        Some(Err(_)) => Err(Error::invalid_utf8()),
        None => Err(Error::missing_required_argument(name)),
    }
}

pub fn parse_args<'a>(
    args: &mut dyn Iterator<Item = OsString>,
    handlers: &ArgHandlers<'a>,
) -> Result<()> {
    let mut num_args = 0;
    let _bin_name = args.next();
    while let Some(arg_os) = args.next() {
        let arg: &str = &arg_os.to_str().ok_or_else(Error::invalid_utf8)?;

        // Match on the first two characters and remainder
        let mut chars = arg.chars();
        match (chars.next(), chars.next(), chars.as_str()) {
            (Some('-'), Some('-'), "") => todo!("Trailing args"),

            // Long argument
            (Some('-'), Some('-'), arg) => {
                // Split at '=' if it exists.
                let (arg, opt_value) = match arg.find('=') {
                    Some(i) => {
                        let (x, y) = arg.split_at(i);
                        (x, Some(y[1..].into()))
                    }
                    None => (arg, None),
                };
                match (
                    handlers.flag_by_long(arg),
                    handlers.option_by_long(arg),
                    opt_value,
                ) {
                    (Some(_), _, Some(_)) => return Err(Error::unexpected_value(arg)),
                    (Some(handler), _, None) => (&mut *handler.assign.borrow_mut())()?,
                    (_, Some(handler), Some(value)) => handler.assign.assign(value)?,
                    (_, Some(handler), None) => {
                        let value = next_value(handler.name, args)?;
                        handler.assign.assign(value)?
                    }
                    _ => return Err(Error::unknown_switch(&format!("--{}", arg))),
                }
            }

            // Short argument
            (Some('-'), Some(c), rest) => {
                match (handlers.flag_by_short(c), handlers.option_by_short(c)) {
                    // One or more flags
                    (Some(handler), _) => {
                        if rest.contains('=') {
                            return Err(Error::unexpected_value(&format!("-{}", c)));
                        }
                        (&mut *handler.assign.borrow_mut())()?;
                        for c in rest.chars() {
                            match handlers.flag_by_short(c) {
                                Some(handler) => (&mut *handler.assign.borrow_mut())()?,
                                None => return Err(Error::unknown_switch(&format!("-{}", c))),
                            }
                        }
                    }
                    // One option
                    (_, Some(handler)) => {
                        let value = match rest.chars().next() {
                            None => next_value(handler.name, args)?,
                            Some('=') => rest[1..].to_string(),
                            _ => rest.to_string(),
                        };
                        handler.assign.assign(value)?;
                    }
                    _ => return Err(Error::unknown_switch(&format!("-{}", c))),
                }
            }

            // Positional argument
            _ => {
                let handler = match (handlers.positions.get(num_args), handlers.positions.last()) {
                    (Some(handler), _) => Some(handler),
                    (_, Some(handler)) if handler.is_multiple => Some(handler),
                    _ => None,
                };
                if let Some(handler) = handler {
                    let value = arg.to_string();
                    handler.assign.assign(value)?;
                    num_args += 1;
                } else {
                    return Err(Error::too_many_positional(arg));
                }
            }
        }
    }
    Ok(())
}

pub trait Assign {
    fn assign(&self, value: String) -> Result<()>;
}

pub struct ParsedAssign<'a, T, F> {
    name: &'a str,
    assign: RefCell<F>,
    _type: PhantomData<T>,
}

impl<'a, T, F> ParsedAssign<'a, T, F> {
    pub fn new(name: &'a str, assign: F) -> Self {
        Self {
            name,
            assign: RefCell::new(assign),
            _type: PhantomData,
        }
    }
}

impl<T, F> Assign for ParsedAssign<'_, T, F>
where
    T: FromStr,
    <T as FromStr>::Err: StdError + 'static,
    F: FnMut(T) -> Result<()>,
{
    #[inline]
    fn assign(&self, value: String) -> Result<()> {
        let parsed: T = value
            .parse()
            .map_err(|e| Error::parse_failed(self.name, Box::new(e)))?;
        (&mut *self.assign.borrow_mut())(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let mut verbose = 0;
        let mut option = None;
        let mut pos = None;
        let res = parse_args(
            &mut ["foo", "--num=10", "-vvv", "hello"]
                .iter()
                .map(OsString::from),
            &ArgHandlers {
                flags: &[FlagHandler {
                    name: "verbose",
                    short: Some('v'),
                    long: None,
                    assign: &RefCell::new(|| Ok(verbose += 1)),
                }],
                options: &[OptionHandler {
                    name: "num",
                    short: None,
                    long: Some("num"),
                    assign: &ParsedAssign::new("num", &mut |x| Ok(option = Some(x))),
                }],
                positions: &[PositionalHandler {
                    name: "foo",
                    is_multiple: false,
                    assign: &ParsedAssign::new("foo", &mut |x| Ok(pos = Some(x))),
                }],
            },
        );
        assert!(res.is_ok());
        assert_eq!(verbose, 3);
        assert_eq!(option, Some(10));
        assert_eq!(pos, Some("hello".to_string()));
    }
}
