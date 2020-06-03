use crate::*;
use std::ffi::OsString;

fn next_value(switch: Switch, args: ArgOsIterator) -> Result<String> {
    match args.next().map(OsString::into_string) {
        Some(Ok(value)) => Ok(value),
        Some(Err(_)) => Err(Error::invalid_utf8()),
        None => Err(Error::missing_value(switch)),
    }
}

fn parse_long<'a>(arg: &str, args: ArgOsIterator, hs: &ArgHandlers<'a>) -> Result<()> {
    // Split at '=' if it exists.
    let (arg, opt_value) = match arg.find('=') {
        Some(i) => {
            let (x, y) = arg.split_at(i);
            (x, Some(y[1..].to_string()))
        }
        None => (arg, None),
    };
    match (hs.flag_by_long(arg), hs.option_by_long(arg), opt_value) {
        (Some(h), _, None) => h.assign(),
        (Some(_), _, Some(_)) => Err(Error::unexpected_value(Switch::Long(arg))),
        (_, Some(h), Some(value)) => h.assign(value),
        (_, Some(h), None) => h.assign(next_value(Switch::Long(arg), args)?),
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

fn parse_short_option(c: char, rest: &str, args: ArgOsIterator, h: &OptionHandler) -> Result<()> {
    let value = match rest.chars().next() {
        None => next_value(Switch::Short(c), args)?,
        Some('=') => rest[1..].to_string(),
        _ => rest.to_string(),
    };
    h.assign(value)
}

fn parse_short<'a>(c: char, rest: &str, args: ArgOsIterator, hs: &ArgHandlers<'a>) -> Result<()> {
    match (hs.flag_by_short(c), hs.option_by_short(c)) {
        (Some(handler), _) => {
            parse_short_flag(c, rest, handler)?;
            let chars = &mut rest.chars();
            while let Some(c) = chars.next() {
                match (hs.flag_by_short(c), hs.option_by_short(c)) {
                    (Some(handler), _) => parse_short_flag(c, chars.as_str(), handler)?,
                    (_, Some(handler)) => {
                        return parse_short_option(c, chars.as_str(), args, handler)
                    }
                    _ => return Err(Error::unknown_switch(Switch::Short(c))),
                }
            }
            Ok(())
        }
        (_, Some(handler)) => parse_short_option(c, rest, args, handler),
        _ => Err(Error::unknown_switch(Switch::Short(c))),
    }
}

fn parse_positional<'a>(arg: &str, num_args: &mut usize, hs: &ArgHandlers<'a>) -> Result<()> {
    let handler = match (hs.positions.get(*num_args), hs.positions.last()) {
        (Some(handler), _) => Some(handler),
        (_, Some(handler)) if handler.is_multiple => Some(handler),
        _ => None,
    };
    if let Some(handler) = handler {
        *num_args += 1;
        handler.assign(arg.to_string())
    } else {
        Err(Error::too_many_positional(arg))
    }
}

pub fn parse_args<'a>(args: ArgOsIterator, hs: &ArgHandlers<'a>) -> Result<()> {
    let mut num_args = 0;
    let _bin_name = args.next();
    while let Some(arg_os) = args.next() {
        let arg: &str = &arg_os.to_str().ok_or_else(Error::invalid_utf8)?;

        // Match on the first two characters and remainder
        let mut chars = arg.chars();
        match (chars.next(), chars.next(), chars.as_str()) {
            (Some('-'), Some('-'), "") => todo!("Trailing args"),
            (Some('-'), Some('-'), arg) => parse_long(arg, args, hs)?,
            (Some('-'), Some(c), rest) => parse_short(c, rest, args, hs)?,
            _ => parse_positional(arg, &mut num_args, hs)?,
        }
    }
    Ok(())
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
