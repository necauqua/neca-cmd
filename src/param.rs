use std::{
    borrow::Cow,
    fmt::{self, Display},
    mem,
    sync::Arc,
};

use compact_str::{CompactString, ToCompactString};

#[derive(Debug, Clone, PartialEq)]
pub enum Fragment {
    Text(Arc<str>),
    Var(Arc<str>, Option<Param>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamType {
    Any,
    Math,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    text: Arc<str>,
    tpe: ParamType,
    fragments: Vec<Fragment>,
}

impl Default for Param {
    fn default() -> Self {
        Self {
            text: Default::default(),
            tpe: ParamType::Any,
            fragments: vec![],
        }
    }
}

impl Param {
    pub fn simple(text: impl Into<Arc<str>>) -> Self {
        let text = text.into();
        Self {
            text,
            tpe: ParamType::Any,
            fragments: vec![],
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn tpe(&self) -> ParamType {
        self.tpe
    }

    pub fn references(&self) -> Vec<Arc<str>> {
        fn rec(fragments: &[Fragment], result: &mut Vec<Arc<str>>) {
            for fragment in fragments {
                if let Fragment::Var(name, optional_default) = fragment {
                    if let Some(def) = optional_default {
                        rec(&def.fragments, result);
                    }
                    result.push(name.clone());
                }
            }
        }

        let mut result = Vec::new();
        rec(&self.fragments, &mut result);
        result
    }

    pub fn expand_static(&self) -> Option<CompactString> {
        match &self.fragments[..] {
            [] => Some(self.text.to_compact_string()),
            [Fragment::Text(text)] => Some(text.to_compact_string()),
            _ => {
                // should rarely, if ever, happen
                let mut result = CompactString::const_new("");
                for fragment in &self.fragments {
                    if let Fragment::Text(text) = fragment {
                        result.push_str(text);
                    } else {
                        return None;
                    }
                }
                Some(result)
            }
        }
    }

    pub fn expand(&self, mut lookup: impl FnMut(&str) -> Option<String>) -> CompactString {
        if self.fragments.is_empty() {
            return self.text.to_compact_string();
        }
        self.expand_rec(&mut lookup)
    }

    fn expand_rec(&self, lookup: &mut impl FnMut(&str) -> Option<String>) -> CompactString {
        let mut result = CompactString::const_new("");
        for fragment in &self.fragments {
            match fragment {
                Fragment::Text(text) => result.push_str(text),
                Fragment::Var(name, default) => {
                    if let Some(param) = lookup(name) {
                        result.push_str(&param);
                        continue;
                    }
                    if let Some(default) = default {
                        result.push_str(&default.expand_rec(lookup));
                        continue;
                    }
                }
            }
        }
        result
    }

    pub fn parse(input: &str) -> Self {
        let (input, tpe) = match unwrap_string_literals(input) {
            Some(unwrapped) => (Cow::Owned(unwrapped), ParamType::Any),
            None => {
                if input.starts_with("(") && input.ends_with(")") {
                    (Cow::Borrowed(input), ParamType::Math)
                } else {
                    (Cow::Borrowed(input), ParamType::Any)
                }
            }
        };

        let mut fragments = Vec::new();
        let mut current = String::new();
        let mut chars = input.chars().peekable();

        while let Some(c) = chars.next() {
            if c != '%' {
                current.push(c);
                continue;
            }

            // escape % by doing %%
            if chars.next_if_eq(&'%').is_some() {
                current.push('%');
                continue;
            }

            if chars.next_if_eq(&'(').is_some() {
                let mut name = CompactString::const_new("");
                let mut default: Option<String> = None;
                let mut closed = false;
                let mut was_slash = false;

                for ch in chars.by_ref() {
                    let ch = if was_slash {
                        was_slash = false;
                        match ch {
                            'n' => '\n',
                            't' => '\t',
                            _ => ch,
                        }
                    } else {
                        if ch == '\\' {
                            was_slash = true;
                            continue;
                        }
                        if ch == ')' {
                            closed = true;
                            break;
                        }
                        ch
                    };

                    if let Some(default) = &mut default {
                        default.push(ch);
                    } else if ch == ':' {
                        default = Some(String::new());
                    } else {
                        name.push(ch);
                    }
                }

                if !closed {
                    current.push_str("%(");
                    current.push_str(&name);
                    continue;
                }
                if name.is_empty() {
                    current.push_str("%()");
                    continue;
                }

                if !current.is_empty() {
                    fragments.push(Fragment::Text(mem::take(&mut current).into()));
                }
                fragments.push(Fragment::Var(
                    name.into(),
                    default.map(|s| Param::parse(&s)),
                ));

                continue;
            }

            let mut name = CompactString::const_new("");

            if let Some('1'..'9') = chars.peek() {
                while let Some(ch) = chars.next_if(|&ch| ch.is_ascii_digit()) {
                    name.push(ch);
                }
            } else {
                while let Some(ch) =
                    chars.next_if(|&ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                {
                    name.push(ch);
                }
            }

            if !name.is_empty() {
                if !current.is_empty() {
                    fragments.push(Fragment::Text(mem::take(&mut current).into()));
                }
                fragments.push(Fragment::Var(name.into(), None));
                continue;
            }

            // treat lone (not followed by ident or a paren) '%' as itself
            current.push('%')
        }

        if !current.is_empty() {
            fragments.push(Fragment::Text(current.into()));
        }

        Self {
            text: input.into(),
            tpe,
            fragments,
        }
    }
}

impl Display for Param {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if matches!(self.tpe, ParamType::Math) {
            write!(f, "{}", self.text)
        } else if self.text.chars().all(|ch| ch.is_alphanumeric()) {
            f.write_str(&self.text)
        } else {
            write!(f, "{{ {} }}", self.text)
        }
    }
}

fn unwrap_string_literals(input: &str) -> Option<String> {
    let input = match input.strip_prefix('"') {
        Some(tail) => tail.strip_suffix('"'),
        None => input
            .strip_prefix('{')
            .and_then(|tail| tail.strip_suffix('}'))
            .map(|s| s.trim()),
    }?;
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next() {
                Some('n') => result.push('\n'),
                // Some('r') => result.push('\r'), // SSE does not support \r lol
                Some('t') => result.push('\t'),
                Some(ch) => result.push(ch),
                _ => (),
            },
            ch => result.push(ch),
        }
    }
    Some(result)
}

// vibe-coded the tests and then rewrote half of them anyway, meh
#[cfg(test)]
mod tests {
    use super::*;
    use Fragment as F;

    #[test]
    fn empty_input() {
        let input = "";
        let expected = vec![];
        assert_eq!(Param::parse(input).fragments, expected);
    }

    #[test]
    fn empty_var() {
        let input = "This is literal %(), not a variable";
        let expected = vec![F::Text("This is literal %(), not a variable".into())];
        assert_eq!(Param::parse(input).fragments, expected);
    }

    #[test]
    fn unclosed_var() {
        let input = "This is literal %(, not a weird variable";
        let expected = vec![F::Text("This is literal %(, not a weird variable".into())];
        assert_eq!(Param::parse(input).fragments, expected);
    }

    #[test]
    fn number() {
        let input = "Count to %123";
        let expected = vec![F::Text("Count to ".into()), F::Var("123".into(), None)];
        assert_eq!(Param::parse(input).fragments, expected);
    }

    #[test]
    fn named_short() {
        let input = "My name is %name";
        let expected = vec![F::Text("My name is ".into()), F::Var("name".into(), None)];
        assert_eq!(Param::parse(input).fragments, expected);
    }

    #[test]
    fn named_paren() {
        let input = "My name is %(name)";
        let expected = vec![F::Text("My name is ".into()), F::Var("name".into(), None)];
        assert_eq!(Param::parse(input).fragments, expected);
    }

    #[test]
    fn named_with_default() {
        let input = "Hello %(name:anonymous)!";
        let expected = vec![
            F::Text("Hello ".into()),
            F::Var("name".into(), Some(Param::parse("anonymous"))),
            F::Text("!".into()),
        ];
        assert_eq!(Param::parse(input).fragments, expected);
    }

    #[test]
    fn complex() {
        let input = "Welcome %i to %(app:MyApp), user number %42 with %role!";
        let expected = vec![
            F::Text("Welcome ".into()),
            F::Var("i".into(), None),
            F::Text(" to ".into()),
            F::Var("app".into(), Some(Param::parse("MyApp"))),
            F::Text(", user number ".into()),
            F::Var("42".into(), None),
            F::Text(" with ".into()),
            F::Var("role".into(), None),
            F::Text("!".into()),
        ];
        assert_eq!(Param::parse(input).fragments, expected);
    }

    #[test]
    fn bare_percent() {
        let input = "The price increased by 25% this year";
        let expected = vec![F::Text("The price increased by 25% this year".into())];
        assert_eq!(Param::parse(input).fragments, expected);
    }

    #[test]
    fn escapes() {
        let input = "Documentation: %%i for repeat count, %%(name) for named variables";
        let expected = vec![F::Text(
            "Documentation: %i for repeat count, %(name) for named variables".into(),
        )];
        assert_eq!(Param::parse(input).fragments, expected);
    }

    #[test]
    fn escaped_paren() {
        let input = "Funky default: %(var:(default\\))";
        let expected = vec![
            F::Text("Funky default: ".into()),
            F::Var("var".into(), Some(Param::parse("(default)"))),
        ];
        assert_eq!(Param::parse(input).fragments, expected);
    }
}
