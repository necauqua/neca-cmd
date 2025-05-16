use compact_str::CompactString;
use lazy_regex::regex_if;
use std::{
    collections::VecDeque,
    fmt::{self, Display, Write},
};

pub mod calc;
pub mod param;

use param::Param;

#[derive(Debug, Clone, Copy)]
pub enum Symbol {
    SuffixTilde,
    Tilde,
    Plus,
    Exclam,
    Hash,
    Question,
    None,
}

impl Symbol {
    pub fn write_command(&self, f: &mut fmt::Formatter<'_>, command: &str) -> fmt::Result {
        if matches!(self, Self::SuffixTilde) {
            f.write_str(command)?;
            return f.write_char('~');
        }
        f.write_str(match self {
            Self::Tilde => "~",
            Self::Plus => "+",
            Self::Exclam => "!",
            Self::Hash => "#",
            Self::Question => "?",
            _ => "",
        })?;
        f.write_str(command)
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub name: CompactString,
    pub symbol: Symbol,
}

impl Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.symbol.write_command(f, &self.name)
    }
}

impl Token {
    pub fn split_inline_number(&self) -> Option<(&str, &str)> {
        // eh, in this instance regex is a nice oneliner instead of an enormous hand-rolled parser
        // and the whole inline numbers thing is a hack anyway
        regex_if!(r#"^(?<name>.*?)(?<n>-?\d+[sk]?)$"#, &*self.name, (name, n))
    }
}

#[derive(Debug, Clone)]
pub struct Command {
    pub token: Token,
    pub params: VecDeque<Param>,
}

impl fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.token.name)?;
        for param in &self.params {
            write!(f, ":{param}")?;
        }
        f.write_str("~")?;
        Ok(())
    }
}

impl Command {
    pub fn parse(word: &str) -> Option<Self> {
        let (word, tpe) = word
            .strip_suffix('~')
            .map(|w| (w, Symbol::SuffixTilde))
            .or(word.strip_prefix('~').map(|w| (w, Symbol::Tilde)))
            .or(word.strip_prefix('+').map(|w| (w, Symbol::Plus)))
            .or(word.strip_prefix('!').map(|w| (w, Symbol::Exclam)))
            .or(word.strip_prefix('#').map(|w| (w, Symbol::Hash)))
            .or(word.strip_prefix('?').map(|w| (w, Symbol::Question)))
            .unwrap_or((word, Symbol::None));

        let mut args = split_balanced(word, &[':']);

        let name = args.pop_front().unwrap();

        if name.is_empty() {
            return None;
        }

        // disallow commands ending in `:`, and disallow commands without symbols not containing a single :
        if args.back().is_some_and(|s| s.is_empty())
            || matches!(tpe, Symbol::None) && args.is_empty()
        {
            return None;
        }

        Some(Self {
            token: Token { name, symbol: tpe },
            params: args.iter().map(|arg| Param::parse(arg)).collect(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct Statement {
    pub original: String,
    pub parallel: Vec<Vec<Command>>,
    /// True if the message did not contain any non-command non-whitespace text
    pub pure: bool,
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, group) in self.parallel.iter().enumerate() {
            if i > 0 {
                f.write_str(" | ")?;
            }
            for (j, command) in group.iter().enumerate() {
                if j > 0 {
                    f.write_str(" ")?;
                }
                write!(f, "{command}")?;
            }
        }
        Ok(())
    }
}

impl Statement {
    pub fn parse(content: &str) -> Self {
        let mut pure = true;

        let trimmed = content.trim_matches('\u{34f}'); // 7tv spam utf tag

        if let Some(rest) = trimmed.strip_prefix("!") {
            let mut args = split_balanced(rest, &[' ']);

            let name = args.pop_front().unwrap();

            if name.is_empty() {
                return Self {
                    original: content.to_owned(),
                    parallel: vec![vec![]],
                    pure,
                };
            }
            if !name.contains(':') {
                let command = Command {
                    token: Token {
                        name,
                        symbol: Symbol::Exclam,
                    },
                    params: args.iter().map(|arg| Param::parse(arg)).collect(),
                };
                return Self {
                    original: content.to_owned(),
                    parallel: vec![vec![command]],
                    pure,
                };
            }
        }

        Self {
            original: content.to_owned(),
            parallel: split_balanced(trimmed, &['|'])
                .into_iter()
                .map(|group| {
                    split_balanced(&group, &[' ', ','])
                        .iter()
                        .filter_map(|s| {
                            let s = s.trim().trim_matches('\u{34f}');
                            if s.is_empty() {
                                return None;
                            }
                            let parsed = Command::parse(s);
                            pure &= parsed.is_some();
                            parsed
                        })
                        .collect()
                })
                .filter(|g: &Vec<_>| !g.is_empty())
                .collect::<Vec<_>>(),
            pure,
        }
    }

    pub fn is_noop(&self) -> bool {
        self.parallel.iter().all(|seq| seq.is_empty())
    }
}

impl From<&str> for Statement {
    fn from(content: &str) -> Self {
        Self::parse(content)
    }
}

// split that considers "string literals"
fn split_balanced(input: &str, seps: &[char]) -> VecDeque<CompactString> {
    let mut result = VecDeque::new();
    let mut current = CompactString::const_new("");
    let mut in_string = false;
    let mut brace_depth = 0;
    let mut paren_depth = 0;

    let mut chars = input.chars();

    while let Some(ch) = chars.next() {
        if (in_string || brace_depth != 0) && ch == '\\' {
            if let Some(ch2) = chars.next() {
                current.push('\\');
                current.push(ch2);
            }
            continue;
        }
        match ch {
            '"' if brace_depth == 0 => in_string = !in_string,
            '“' if brace_depth == 0 => in_string = !in_string,
            '”' if brace_depth == 0 => in_string = !in_string,
            '{' if !in_string => brace_depth += 1,
            '}' if !in_string && brace_depth > 0 => brace_depth -= 1,
            '(' if !in_string => paren_depth += 1,
            ')' if !in_string && paren_depth > 0 => paren_depth -= 1,
            _ => {}
        }
        if !in_string && brace_depth == 0 && paren_depth == 0 && seps.contains(&ch) {
            result.push_back(std::mem::take(&mut current));
        } else {
            current.push(ch);
        }
    }

    // If any structure is still open, split the inside as well
    if in_string || brace_depth != 0 || paren_depth != 0 {
        let mut part = CompactString::const_new("");
        for ch in current.chars() {
            if seps.contains(&ch) {
                result.push_back(std::mem::take(&mut part));
            } else {
                part.push(ch);
            }
        }
        result.push_back(part);
    } else {
        result.push_back(current);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_empty_input() {
        assert_eq!(split_balanced("", &[',']), vec![""]);
    }

    #[test]
    fn split_multiple_separators() {
        assert_eq!(split_balanced("foo,,bar", &[',']), vec!["foo", "", "bar"]);
    }

    #[test]
    fn split_only_separators() {
        assert_eq!(split_balanced(",,", &[',']), vec!["", "", ""]);
    }

    #[test]
    fn split_unbalanced_braces() {
        assert_eq!(split_balanced("{foo,bar", &[',']), vec!["{foo", "bar"]);
    }

    #[test]
    fn split_unbalanced_parens() {
        assert_eq!(
            split_balanced("foo(bar,baz", &[',']),
            vec!["foo(bar", "baz"]
        );
    }

    #[test]
    fn split_unclosed_string() {
        assert_eq!(split_balanced("\"foo,bar", &[',']), vec!["\"foo", "bar"]);
    }

    #[test]
    fn split_escaped_quote() {
        assert_eq!(
            split_balanced("\"foo\\\",bar\",baz", &[',']),
            vec!["\"foo\\\",bar\"", "baz"]
        );
    }

    #[test]
    fn split_nested_braces() {
        assert_eq!(
            split_balanced("{foo,{bar,baz}},qux", &[',']),
            vec!["{foo,{bar,baz}}", "qux"]
        );
    }

    #[test]
    fn split_nested_parens() {
        assert_eq!(
            split_balanced("(foo,(bar,baz)),qux", &[',']),
            vec!["(foo,(bar,baz))", "qux"]
        );
    }

    #[test]
    fn split_multiple_different_separators() {
        assert_eq!(
            split_balanced("foo|,bar", &['|', ',']),
            vec!["foo", "", "bar"]
        );
    }

    #[test]
    fn split_escaped_quote_at_end_of_string() {
        assert_eq!(
            split_balanced("\"foo\\\"bar\\\"\"", &[',']),
            vec!["\"foo\\\"bar\\\"\""]
        );
    }

    #[test]
    fn split_escaped_quote_at_start_of_string() {
        assert_eq!(
            split_balanced("\"\\\"foo,bar\"", &[',']),
            vec!["\"\\\"foo,bar\""]
        );
    }

    #[test]
    fn parsing() {
        let message = "Hello, this is left~ and right:.3~ and mouse:123:321~ | and then test~ test2~ and ~nope";
        let parsed = Statement::parse(message);

        insta::assert_snapshot!(parsed, @"left~ right:{ .3 }~ mouse:123:321~ | test~ test2~ nope~");
    }

    #[test]
    fn only_symbol_non_command() {
        assert!(Statement::parse("~").is_noop());
    }

    #[test]
    fn only_argument_non_command() {
        assert!(Statement::parse(":foo~").is_noop());
    }

    #[test]
    fn split_inline_number() {
        let token = Token {
            name: "foo-123s".into(),
            symbol: Symbol::None,
        };
        let split = token.split_inline_number();
        assert_eq!(split, Some(("foo", "-123s")));
    }

    #[test]
    fn pluses() {
        let message = "Hello, this is +left and +right:.3 and +mouse:123:321 | and then +test +test2 and +wut~";
        let parsed = Statement::parse(message);

        insta::assert_snapshot!(parsed, @"left~ right:{ .3 }~ mouse:123:321~ | test~ test2~ +wut~");
    }

    #[test]
    fn exclams() {
        let message = "Hello, this is !left and !right:.3 and !mouse:123:321 | and then !test !test2 and !wut~";
        let parsed = Statement::parse(message);

        insta::assert_snapshot!(parsed, @"left~ right:{ .3 }~ mouse:123:321~ | test~ test2~ !wut~");
    }

    #[test]
    fn nothings() {
        let message =
            "Hello, this is left and right:.3 and mouse:123:321 | and then test test2 and wut";
        let parsed = Statement::parse(message);

        insta::assert_snapshot!(parsed, @"right:{ .3 }~ mouse:123:321~");
    }

    #[test]
    fn strings() {
        let message = r#"print:"hello space"~ +hah:"and | pipe" | ~nope:123 | and-also-escapes:" \"incredible\", lol"~ "#;

        let parsed = Statement::parse(message);

        insta::assert_snapshot!(parsed, @r#"print:{ hello space }~ hah:{ and | pipe }~ | nope:123~ | and-also-escapes:{  "incredible", lol }~"#);
    }

    #[test]
    fn braces() {
        let message = r#"print:{ hello space }~ +hah:{ and | pipe } | ~nope:123 | and-also-escapes:{  { incredible }, lol }~ "#;

        let parsed = Statement::parse(message);

        insta::assert_snapshot!(parsed, @"print:{ hello space }~ hah:{ and | pipe }~ | nope:123~ | and-also-escapes:{ { incredible }, lol }~");
    }

    #[test]
    fn pure_cmd() {
        assert!(Statement::parse("U3s~ | w1s~ l600~").pure);
    }

    #[test]
    fn seventv_spam_suffix() {
        assert!(Statement::parse("+lh 󠀀").pure);
    }

    #[test]
    fn empty_arg() {
        let parsed = Statement::parse("command::second-arg~");

        insta::assert_snapshot!(parsed, @"command::{ second-arg }~");
    }

    #[test]
    fn funky() {
        let parsed = Statement::parse(" test:some-text-with-{ braces and shit }-in-it ");

        insta::assert_snapshot!(parsed, @"test:{ some-text-with-{ braces and shit }-in-it }~");
    }

    #[test]
    fn percent_full_arg() {
        let parsed = Statement::parse(" w:%(1:default) ");

        insta::assert_snapshot!(parsed, @"w:{ %(1:default) }~");
    }

    #[test]
    fn url_double_arg() {
        let parsed = Statement::parse(" sr:https://youtu.be/dQw4w9WgXcQ ");

        insta::assert_snapshot!(parsed, @"sr:https:{ //youtu.be/dQw4w9WgXcQ }~");
    }
}
