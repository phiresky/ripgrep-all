use std::borrow::Cow;

use anyhow::Result;

// from https://github.com/phiresky/timetrackrs/blob/1c3df09ba2c1fda6065f2927045bd28dea0738d3/src/expand.rs

pub fn find_byte(needle: u8, haystack: &[u8]) -> Option<usize> {
    #[cfg(not(feature = "perf-literal"))]
    fn imp(needle: u8, haystack: &[u8]) -> Option<usize> {
        haystack.iter().position(|&b| b == needle)
    }

    #[cfg(feature = "perf-literal")]
    fn imp(needle: u8, haystack: &[u8]) -> Option<usize> {
        use memchr::memchr;
        memchr(needle, haystack)
    }

    imp(needle, haystack)
}

pub fn expand_str_ez<'a, F>(replacement: &'a str, lambda: F) -> Result<String>
where
    F: Fn(&str) -> Result<Cow<'a, str>>,
{
    let mut dst = String::new();
    expand_str_lambda(lambda, replacement, &mut dst)?;
    Ok(dst)
}

pub fn expand_str_lambda<'a, F>(cap: F, replacement: &'a str, dst: &mut String) -> Result<()>
where
    F: Fn(&str) -> Result<Cow<'a, str>>,
{
    let mut replacement = replacement;
    while !replacement.is_empty() {
        match find_byte(b'$', replacement.as_bytes()) {
            None => break,
            Some(i) => {
                dst.push_str(&replacement[..i]);
                replacement = &replacement[i..];
            }
        }
        if replacement.as_bytes().get(1).is_some_and(|&b| b == b'$') {
            dst.push('$');
            replacement = &replacement[2..];
            continue;
        }
        debug_assert!(!replacement.is_empty());
        let cap_ref = match find_cap_ref(replacement.as_bytes()) {
            Some(cap_ref) => cap_ref,
            None => {
                dst.push('$');
                replacement = &replacement[1..];
                continue;
            }
        };
        replacement = &replacement[cap_ref.end..];
        dst.push_str(cap(cap_ref.cap)?.as_ref());
    }
    dst.push_str(replacement);
    Ok(())
}

/// `CaptureRef` represents a reference to a capture group inside some text.
/// The reference is either a capture group name or a number.
///
/// It is also tagged with the position in the text following the
/// capture reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CaptureRef<'a> {
    cap: &'a str,
    end: usize,
}

/// Parses a possible reference to a capture group name in the given text,
/// starting at the beginning of `replacement`.
///
/// If no such valid reference could be found, None is returned.
fn find_cap_ref(replacement: &[u8]) -> Option<CaptureRef> {
    let mut i = 0;
    let rep: &[u8] = replacement;
    if rep.len() <= 1 || rep[0] != b'$' {
        return None;
    }
    i += 1;
    if rep[i] == b'{' {
        return find_cap_ref_braced(rep, i + 1);
    }
    let mut cap_end = i;
    while rep.get(cap_end).is_some_and(is_valid_cap_letter) {
        cap_end += 1;
    }
    if cap_end == i {
        return None;
    }
    // We just verified that the range 0..cap_end is valid ASCII, so it must
    // therefore be valid UTF-8. If we really cared, we could avoid this UTF-8
    // check with either unsafe or by parsing the number straight from &[u8].
    let cap = std::str::from_utf8(&rep[i..cap_end]).expect("valid UTF-8 capture name");
    Some(CaptureRef { cap, end: cap_end })
}

fn find_cap_ref_braced(rep: &[u8], mut i: usize) -> Option<CaptureRef> {
    let start = i;
    while rep.get(i).is_some_and(|&b| b != b'}') {
        i += 1;
    }
    if rep.get(i).is_none_or(|&b| b != b'}') {
        return None;
    }
    // When looking at braced names, we don't put any restrictions on the name,
    // so it's possible it could be invalid UTF-8. But a capture group name
    // can never be invalid UTF-8, so if we have invalid UTF-8, then we can
    // safely return None.
    let cap = match std::str::from_utf8(&rep[start..i]) {
        Err(_) => return None,
        Ok(cap) => cap,
    };
    Some(CaptureRef { cap, end: i + 1 })
}

/// Returns true if and only if the given byte is allowed in a capture name.
fn is_valid_cap_letter(b: &u8) -> bool {
    matches!(b, b'0'..=b'9' | b'a'..=b'z' | b'A'..=b'Z' | b'_')
}
