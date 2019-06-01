use std::iter::Peekable;

pub(crate) fn expand_line(line: &str) -> Result<Vec<String>, &'static str> {
    let mut bytes = line.bytes().enumerate().peekable();
    let mut args = Vec::new();

    while let Some(&(_, b)) = bytes.peek() {
        match b {
            b' ' | b'\t' => {
                bytes.next();
            }
            _ => {
                if let Some(v) = arg(line, &mut bytes)? {
                    args.push(v.into())
                }
            }
        }
    }

    Ok(args)
}

fn arg<'a, I>(line: &'a str, bytes: &mut Peekable<I>) -> Result<Option<&'a str>, &'static str>
where
    // P: Peekable<I>,
    I: Iterator<Item = (usize, u8)>,
{
    let mut start = None;
    let mut end = None;

    // Skip over any leading whitespace
    while let Some(&(_, b)) = bytes.peek() {
        match b {
            b' ' | b'\t' => {
                bytes.next();
            }
            _ => break,
        }
    }

    while let Some(&(i, b)) = bytes.peek() {
        if start.is_none() {
            start = Some(i)
        }
        match b {
            // Evaluate a quoted string but do not return it
            // We pass in i, the index of a quote, but start a character later. This ensures
            // the production rules will produce strings with the quotes intact
            b'"' => {
                bytes.next();
                double_quoted(line, bytes, i)?;
            }
            b'\'' => {
                bytes.next();
                single_quoted(line, bytes, i)?;
            }
            // If we see a backslash, assume that it is leading up to an escaped character
            // and skip the next character
            b'\\' => {
                bytes.next();
                bytes.next();
            }
            // If we see a byte from the following set, we've definitely reached the end of
            // the argument
            b' ' | b'\t' => {
                end = Some(i);
                break;
            }
            // By default just pop the next byte: it will be part of the argument
            _ => {
                bytes.next();
            }
        }
    }

    match (start, end) {
        (Some(i), Some(j)) if i < j => Ok(Some(&line[i..j])),
        (Some(i), None) => Ok(Some(&line[i..])),
        _ => Ok(None),
    }
}

fn double_quoted<'a, I>(
    line: &'a str,
    bytes: &mut Peekable<I>,
    start: usize,
) -> Result<&'a str, &'static str>
where
    I: Iterator<Item = (usize, u8)>,
{
    while let Some(&(i, b)) = bytes.peek() {
        bytes.next();

        if b == b'"' {
            // We return an inclusive range to keep the quote type intact
            return Ok(&line[start..=i]);
        } else if b == b'\\' {
            // Skip the next character even if it's a quote, 
            bytes.next();
        }
    }

    Err("Unterminated double quote")
}

fn single_quoted<'a, I>(
    line: &'a str,
    bytes: &mut Peekable<I>,
    start: usize,
) -> Result<&'a str, &'static str>
where
    I: Iterator<Item = (usize, u8)>,
{
    while let Some(&(i, b)) = bytes.peek() {
        bytes.next();

        if b == b'\'' {
            // We return an inclusive range to keep the quote type intact
            return Ok(&line[start..=i]);
        };
    }

    Err("Unterminated single quote")
}
