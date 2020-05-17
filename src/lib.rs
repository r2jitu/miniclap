pub use miniclap_derive::MiniClap;
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
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: String) -> Error {
        Error { message }
    }

    pub fn exit(&self) -> ! {
        eprintln!("error: {}", self.message);
        std::process::exit(1)
    }
}

impl From<&str> for Error {
    fn from(x: &str) -> Self {
        Error::new(x.to_string())
    }
}

impl From<String> for Error {
    fn from(x: String) -> Self {
        Error::new(x)
    }
}
