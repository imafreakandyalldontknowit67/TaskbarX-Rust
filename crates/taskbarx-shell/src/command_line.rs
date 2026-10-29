//! Microsoft CRT/CommandLineToArgvW-compatible quoting for individual arguments.
//! No shell expansion. Always quoting also preserves empty arguments.

pub(crate) fn quote(arg: &str) -> String {
    let mut result = String::from("\"");
    let mut slashes = 0;
    for c in arg.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            '\\',
            if c == '"' { slashes * 2 + 1 } else { slashes },
        ));
        result.push(c);
        slashes = 0;
    }
    result.extend(std::iter::repeat_n('\\', slashes * 2));
    result.push('"');
    result
}

#[cfg(any(windows, test))]
pub(crate) fn arguments(args: &[String]) -> String {
    args.iter().map(|a| quote(a)).collect::<Vec<_>>().join(" ")
}

/// Parse the argument tail (not argv[0]) of a persisted Exec action. Reject
/// unmatched quotes instead of guessing malformed/foreign task boundaries.
#[cfg(any(windows, test))]
pub(crate) fn parse(input: &str) -> Result<Vec<String>, String> {
    crate::text_valid(input)?;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut result = Vec::new();
    while i < chars.len() {
        while i < chars.len() && matches!(chars[i], ' ' | '\t') {
            i += 1;
        }
        if i == chars.len() {
            break;
        }
        let mut arg = String::new();
        let mut quoted = false;
        while i < chars.len() && (quoted || !matches!(chars[i], ' ' | '\t')) {
            let mut slashes = 0;
            while i < chars.len() && chars[i] == '\\' {
                slashes += 1;
                i += 1;
            }
            if i < chars.len() && chars[i] == '"' {
                arg.extend(std::iter::repeat_n('\\', slashes / 2));
                if slashes % 2 == 1 {
                    arg.push('"');
                } else if quoted && chars.get(i + 1) == Some(&'"') {
                    arg.push('"');
                    i += 1;
                } else {
                    quoted = !quoted;
                }
                i += 1;
            } else {
                arg.extend(std::iter::repeat_n('\\', slashes));
                if i < chars.len() && (quoted || !matches!(chars[i], ' ' | '\t')) {
                    arg.push(chars[i]);
                    i += 1;
                }
            }
        }
        if quoted {
            return Err("unbalanced quotes in stored arguments".into());
        }
        result.push(arg);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_preserves_argument_boundaries() {
        let args = [
            "",
            "simple",
            "two words",
            "\t",
            "a\"b",
            "C:\\space dir\\",
            "a\\\"b",
            "\\\\",
            "&<>'\u{96ea}\u{1f680}",
            "end\\\\",
            "\"",
        ]
        .map(str::to_string);
        assert_eq!(parse(&arguments(&args)).unwrap(), args);
        assert_eq!(quote("a\"b"), "\"a\\\"b\"");
        assert_eq!(quote("C:\\x \\"), "\"C:\\x \\\\\"");
        assert_eq!(parse("a b\t\"c d\"").unwrap(), ["a", "b", "c d"]);
        assert!(parse("\"bad").is_err());
    }

    #[test]
    fn exhaustive_small_argument_round_trips() {
        let alphabet = ['a', ' ', '\t', '\\', '"', '\u{96ea}'];
        for n in 0..1296 {
            let mut v = n;
            let mut arg = String::new();
            for _ in 0..4 {
                arg.push(alphabet[v % alphabet.len()]);
                v /= alphabet.len();
            }
            let args = vec!["first".into(), arg, String::new(), "last".into()];
            assert_eq!(parse(&arguments(&args)).unwrap(), args);
        }
    }
}
