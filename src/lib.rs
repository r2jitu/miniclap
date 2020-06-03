pub use miniclap_derive::MiniClap;
use std::error::Error as StdError;
use std::{cell::RefCell, ffi::OsString, marker::PhantomData, str::FromStr};

pub mod error;
pub use error::{Error, Result};

mod parse;
pub use parse::parse_args;

pub type ArgOsIterator<'a> = &'a mut dyn Iterator<Item = OsString>;

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

    fn __parse_internal(args: ArgOsIterator) -> Result<Self>;
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

#[derive(Copy, Clone)]
pub enum Switch<'a> {
    Short(char),
    Long(&'a str),
    Both(char, &'a str),
}

impl std::fmt::Display for Switch<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Switch::Short(c) => write!(f, "-{}", c),
            Switch::Long(l) => write!(f, "--{}", l),
            Switch::Both(c, l) => write!(f, "-{}/--{}", c, l),
        }
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

mod assign {
    use crate::Result;

    pub trait FlagAssign {
        fn assign(&self) -> Result<()>;
    }

    pub trait StringAssign {
        fn assign(&self, name: &str, value: String) -> Result<()>;
    }
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
