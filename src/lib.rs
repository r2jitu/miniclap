pub use miniclap_derive::MiniClap;
use std::error::Error as StdError;
use std::{cell::RefCell, ffi::OsString, marker::PhantomData, str::FromStr};

pub mod error;
pub use error::{Error, Result};

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

pub struct ArgHandlers<'a> {
    pub flags: &'a [FlagHandler<'a>],
    pub options: &'a [OptionHandler<'a>],
    pub positions: &'a [PositionalHandler<'a>],
}

pub struct FlagHandler<'a> {
    pub name: &'a str,
    pub switch: Switch<'a>,
    pub assign: &'a dyn assign::FlagAssign,
}

pub struct OptionHandler<'a> {
    pub name: &'a str,
    pub switch: Switch<'a>,
    pub assign: &'a dyn assign::StringAssign,
}

pub struct PositionalHandler<'a> {
    pub name: &'a str,
    pub is_multiple: bool,
    pub assign: &'a dyn assign::StringAssign,
}

pub enum Switch<'a> {
    Short(char),
    Long(&'a str),
    Both(char, &'a str),
}

mod assign {
    use crate::Result;

    pub trait FlagAssign {
        fn assign(&self) -> Result<()>;
    }

    pub trait StringAssign {
        fn assign(&self, name: &str, value: String) -> Result<()>;
    }
}

impl PartialEq<char> for Switch<'_> {
    fn eq(&self, other: &char) -> bool {
        match self {
            Switch::Short(c) | Switch::Both(c, _) => c == other,
            _ => false,
        }
    }
}

impl PartialEq<&'_ str> for Switch<'_> {
    fn eq(&self, other: &&str) -> bool {
        match self {
            Switch::Long(l) | Switch::Both(_, l) => l == other,
            _ => false,
        }
    }
}

impl<'a> ArgHandlers<'a> {
    fn flag_by_short(&self, c: char) -> Option<&FlagHandler<'a>> {
        self.flags.iter().find(|h| h.switch == c)
    }

    fn flag_by_long(&self, l: &str) -> Option<&FlagHandler<'a>> {
        self.flags.iter().find(|h| h.switch == l)
    }

    fn option_by_short(&self, c: char) -> Option<&OptionHandler<'a>> {
        self.options.iter().find(|h| h.switch == c)
    }

    fn option_by_long(&self, l: &str) -> Option<&OptionHandler<'a>> {
        self.options.iter().find(|h| h.switch == l)
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
                    (Some(handler), _, None) => handler.assign()?,
                    (_, Some(handler), Some(value)) => handler.assign(value)?,
                    (_, Some(handler), None) => handler.assign(next_value(handler.name, args)?)?,
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
                        handler.assign()?;
                        for c in rest.chars() {
                            match handlers.flag_by_short(c) {
                                Some(handler) => handler.assign()?,
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
                        handler.assign(value)?;
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
                    handler.assign(arg.to_string())?;
                    num_args += 1;
                } else {
                    return Err(Error::too_many_positional(arg));
                }
            }
        }
    }
    Ok(())
}

impl FlagHandler<'_> {
    fn assign(&self) -> Result<()> {
        self.assign.assign()
    }
}

impl OptionHandler<'_> {
    fn assign(&self, value: String) -> Result<()> {
        self.assign.assign(self.name, value)
    }
}

impl PositionalHandler<'_> {
    fn assign(&self, value: String) -> Result<()> {
        self.assign.assign(self.name, value)
    }
}

pub struct FlagAssign<F> {
    inner: RefCell<F>,
}

impl<F> FlagAssign<F> {
    pub fn new(assign: F) -> Self {
        Self {
            inner: RefCell::new(assign),
        }
    }
}

impl<F: FnMut()> assign::FlagAssign for FlagAssign<F> {
    #[inline]
    fn assign(&self) -> Result<()> {
        (&mut *self.inner.borrow_mut())();
        Ok(())
    }
}

pub struct ParsedAssign<T, F> {
    assign: RefCell<F>,
    _type: PhantomData<T>,
}

impl<'a, T, F> ParsedAssign<T, F> {
    pub fn new(assign: F) -> Self {
        Self {
            assign: RefCell::new(assign),
            _type: PhantomData,
        }
    }
}

impl<T, F> assign::StringAssign for ParsedAssign<T, F>
where
    T: FromStr,
    <T as FromStr>::Err: StdError + 'static,
    F: FnMut(T),
{
    #[inline]
    fn assign(&self, name: &str, value: String) -> Result<()> {
        let parsed: T = value
            .parse()
            .map_err(|e| Error::parse_failed(name, Box::new(e)))?;
        (&mut *self.assign.borrow_mut())(parsed);
        Ok(())
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
                    switch: Switch::Short('v'),
                    assign: &FlagAssign::new(|| verbose += 1),
                }],
                options: &[OptionHandler {
                    name: "num",
                    switch: Switch::Long("num"),
                    assign: &ParsedAssign::new(&mut |x| option = Some(x)),
                }],
                positions: &[PositionalHandler {
                    name: "foo",
                    is_multiple: false,
                    assign: &ParsedAssign::new(&mut |x| pos = Some(x)),
                }],
            },
        );
        assert!(res.is_ok());
        assert_eq!(verbose, 3);
        assert_eq!(option, Some(10));
        assert_eq!(pos, Some("hello".to_string()));
    }
}
