use crate::{App, ArgOsIterator, Error, FlagHandler, OptionHandler, Result, Switch};
use std::ffi::OsString;

struct Parser<'a> {
    args: ArgOsIterator<'a>,
    app: &'a App<'a>,
    num_args: usize,
    is_trailing: bool,
}

impl<'a> Parser<'a> {
    fn new(args: ArgOsIterator<'a>, app: &'a App<'a>) -> Self {
        Parser {
            args,
            app,
            num_args: 0,
            is_trailing: false,
        }
    }

    fn next_value(&mut self, switch: Switch) -> Result<String> {
        match self.args.next().map(OsString::into_string) {
            Some(Ok(value)) => Ok(value),
            Some(Err(_)) => Err(Error::invalid_utf8()),
            None => Err(Error::missing_value(switch)),
        }
    }

    fn parse_long(&mut self, arg: &str) -> Result<()> {
        // Split at '=' if it exists.
        let (arg, opt_value) = match arg.find('=') {
            Some(i) => {
                let (x, y) = arg.split_at(i);
                (x, Some(y[1..].to_string()))
            }
            None => (arg, None),
        };
        match (
            self.app.flag_by_long(arg),
            self.app.option_by_long(arg),
            opt_value,
        ) {
            (Some(h), _, None) => h.assign(),
            (Some(_), _, Some(_)) => Err(Error::unexpected_value(Switch::Long(arg))),
            (_, Some(h), Some(value)) => h.assign(value),
            (_, Some(h), None) => h.assign(self.next_value(Switch::Long(arg))?),
            _ => Err(Error::unknown_switch(Switch::Long(arg))),
        }
    }

    fn parse_short_flag(c: char, rest: &str, h: &FlagHandler) -> Result<()> {
        if rest.starts_with('=') {
            Err(Error::unexpected_value(Switch::Short(c)))
        } else {
            h.assign()
        }
    }

    fn parse_short_option(&mut self, c: char, rest: &str, h: &OptionHandler) -> Result<()> {
        let value = match rest.chars().next() {
            None => self.next_value(Switch::Short(c))?,
            Some('=') => rest[1..].to_string(),
            _ => rest.to_string(),
        };
        h.assign(value)
    }

    fn parse_short(&mut self, c: char, rest: &str) -> Result<()> {
        match (self.app.flag_by_short(c), self.app.option_by_short(c)) {
            (Some(h), _) => {
                Self::parse_short_flag(c, rest, h)?;
                let chars = &mut rest.chars();
                while let Some(c) = chars.next() {
                    match (self.app.flag_by_short(c), self.app.option_by_short(c)) {
                        (Some(h), _) => Self::parse_short_flag(c, chars.as_str(), h)?,
                        (_, Some(h)) => return self.parse_short_option(c, chars.as_str(), h),
                        _ => return Err(Error::unknown_switch(Switch::Short(c))),
                    }
                }
                Ok(())
            }
            (_, Some(h)) => self.parse_short_option(c, rest, h),
            _ => Err(Error::unknown_switch(Switch::Short(c))),
        }
    }

    fn parse_positional(&mut self, arg: &str) -> Result<()> {
        let h_by_index = self.app.positions.get(self.num_args);
        let h_last = self.app.positions.last().filter(|h| h.is_multiple);
        match h_by_index.or(h_last) {
            Some(h) => {
                self.num_args += 1;
                h.assign(arg.to_string())
            }
            None => Err(Error::too_many_positional(arg)),
        }
    }

    fn parse(mut self) -> Result<()> {
        while let Some(arg_os) = self.args.next() {
            let arg: &str = &arg_os.to_str().ok_or_else(Error::invalid_utf8)?;

            // Match on the first two characters and remainder
            let mut chars = arg.chars();
            match (self.is_trailing, chars.next(), chars.next(), chars.as_str()) {
                (false, Some('-'), Some('-'), "") => self.is_trailing = true,
                (false, Some('-'), Some('-'), arg) => self.parse_long(arg)?,
                (false, Some('-'), Some(c), rest) => self.parse_short(c, rest)?,
                _ => self.parse_positional(arg)?,
            }
        }
        Ok(())
    }
}

pub fn parse_args(_command: &str, args: ArgOsIterator, app: &App) -> Result<()> {
    Parser::new(args, app).parse()
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn simple() {
        let mut verbose = 0;
        let mut option = None;
        let mut pos = None;
        let res = parse_args(
            "foo",
            &mut ["--num=10", "-vvv", "hello"].iter().map(OsString::from),
            &App {
                flags: &[FlagHandler {
                    name: "verbose",
                    switch: Switch::Short('v'),
                    assign: &FlagAssign::new(|| verbose += 1),
                }],
                options: &[OptionHandler {
                    name: "num",
                    switch: Switch::Long("num"),
                    assign: &ParsedAssign::new(|x| option = Some(x)),
                }],
                positions: &[PositionalHandler {
                    name: "foo",
                    is_multiple: false,
                    assign: &ParsedAssign::new(|x| pos = Some(x)),
                }],
            },
        );
        assert!(res.is_ok());
        assert_eq!(verbose, 3);
        assert_eq!(option, Some(10));
        assert_eq!(pos, Some("hello".to_string()));
    }
}
