pub use miniclap_derive::MiniClap;
use std::ffi::OsString;

pub trait MiniClap: Sized {
    #[inline]
    fn parse() -> Self {
        Self::parse_from(std::env::args_os())
    }

    #[inline]
    fn parse_from<I, T>(args: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString>,
    {
        Self::parse_internal(&mut args.into_iter().map(|x| x.into()))
    }

    fn parse_internal(args: &mut dyn Iterator<Item = OsString>) -> Self;
}
