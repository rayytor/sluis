//! zapret's `/opt/zapret/config`: a shell file of `NAME=value` lines that
//! its init scripts source. Read and edited without disturbing anything
//! else in it: comments, spacing, quoting and the variables the app does not
//! manage all stay byte for byte as they were.
//!
//! The text is the only state. Reading finds each assignment's value the way
//! the shell would (quotes may run over several lines); [`Config::set`]
//! replaces just that value. Lines that are not assignments are kept and
//! otherwise ignored.

use std::fmt;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    text: String,
    vars: Vec<Var>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Var {
    name: Range<usize>,
    value: Range<usize>,
    /// The file ended inside a quote of this value.
    unterminated: bool,
}

/// One variable's value, as written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Value<'a> {
    raw: &'a str,
}

impl<'a> Value<'a> {
    /// The value exactly as written, quotes included.
    pub fn raw(&self) -> &'a str {
        self.raw
    }

    /// The value after shell quote removal, or `None` when the shell would
    /// expand something in it (`$VAR`, `$((...))`, backquotes).
    pub fn literal(&self) -> Option<String> {
        let mut out = String::new();
        let mut chars = self.raw.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some('\n') | None => {}
                    Some(next) => out.push(next),
                },
                '\'' => out.extend(chars.by_ref().take_while(|&c| c != '\'')),
                '"' => loop {
                    match chars.next() {
                        None | Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some('\n') => {}
                            Some(e @ ('$' | '`' | '"' | '\\')) => out.push(e),
                            Some(other) => {
                                out.push('\\');
                                out.push(other);
                            }
                            None => out.push('\\'),
                        },
                        Some('$' | '`') => return None,
                        Some(other) => out.push(other),
                    }
                },
                '$' | '`' => return None,
                other => out.push(other),
            }
        }
        Some(out)
    }
}

/// Something wrong in the file that the shell would read differently from
/// how it looks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// The file ends inside this variable's quotes.
    Unterminated { name: String },
    /// This variable's quotes run over lines that look like assignments of
    /// their own, so the shell never sets those variables. This is what
    /// line-by-line editors do to multi-line values.
    Swallowed {
        name: String,
        swallowed: Vec<String>,
    },
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unterminated { name } => write!(f, "{name}: the file ends inside its quotes"),
            Self::Swallowed { name, swallowed } => {
                write!(f, "{name}: its quotes swallow {}", swallowed.join(", "))
            }
        }
    }
}

impl Config {
    pub fn parse(text: impl Into<String>) -> Self {
        let text = text.into();
        let vars = scan(&text);
        Self { text, vars }
    }

    pub fn as_str(&self) -> &str {
        &self.text
    }

    /// The value the shell ends up with: the last assignment of `name`.
    pub fn get(&self, name: &str) -> Option<Value<'_>> {
        self.last(name).map(|v| Value {
            raw: &self.text[v.value.clone()],
        })
    }

    /// The names of all assigned variables, in file order, repeats included.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.vars.iter().map(|v| &self.text[v.name.clone()])
    }

    /// Sets `name` to the literal `value`, quoting it as needed. Replaces the
    /// value of the last assignment only; a new variable goes right after its
    /// commented-out template line (`#NAME=...`) if there is one, else at the
    /// end.
    pub fn set(&mut self, name: &str, value: &str) {
        let encoded = encode(value);
        if let Some(var) = self.last(name) {
            let range = var.value.clone();
            self.text.replace_range(range, &encoded);
        } else {
            let line = format!("{name}={encoded}\n");
            let at = template_line_end(&self.text, name).unwrap_or_else(|| {
                if !self.text.is_empty() && !self.text.ends_with('\n') {
                    self.text.push('\n');
                }
                self.text.len()
            });
            self.text.insert_str(at, &line);
        }
        self.vars = scan(&self.text);
    }

    pub fn problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        for var in &self.vars {
            let name = self.text[var.name.clone()].to_owned();
            let raw = &self.text[var.value.clone()];
            let swallowed: Vec<String> = raw
                .lines()
                .skip(1)
                .filter_map(|line| assignment_name(line.trim_start()))
                .map(str::to_owned)
                .collect();
            if !swallowed.is_empty() {
                problems.push(Problem::Swallowed {
                    name: name.clone(),
                    swallowed,
                });
            }
            if var.unterminated {
                problems.push(Problem::Unterminated { name });
            }
        }
        problems
    }

    /// Undoes [`Problem::Swallowed`] damage where the swallowing quote ends
    /// on an assignment line (`B="` in `A="` … `B="`): `A` and `B` get their
    /// values from `defaults` (zapret's `config.default`; empty if it has
    /// none) and the lines between become assignments of their own again.
    /// Returns the names whose values were restored, in file order.
    pub fn repair(&mut self, defaults: &Config) -> Vec<String> {
        let default = |name: &str| {
            defaults
                .get(name)
                .and_then(|v| v.literal())
                .unwrap_or_default()
        };
        let mut fixed = Vec::new();
        // last first, so the earlier ranges stay valid
        for var in self.vars.clone().iter().rev() {
            let raw = &self.text[var.value.clone()];
            let lines: Vec<&str> = raw.split('\n').collect();
            let [_, middle @ .., last] = lines.as_slice() else {
                continue;
            };
            // the closing quote must be the one that opened `B="`
            let Some(last_name) = assignment_name(last.trim_start()) else {
                continue;
            };
            let tail = &last.trim_start()[last_name.len()..];
            if tail != "=\"" && tail != "='" {
                continue;
            }
            let name = &self.text[var.name.clone()];
            let indent = &last[..last.len() - last.trim_start().len()];
            let mut replacement = encode(&default(name));
            replacement.push('\n');
            for line in middle {
                replacement.push_str(line);
                replacement.push('\n');
            }
            replacement.push_str(&format!(
                "{indent}{last_name}={}",
                encode(&default(last_name))
            ));
            fixed.splice(0..0, [name.to_owned(), last_name.to_owned()]);
            self.text.replace_range(var.value.clone(), &replacement);
        }
        self.vars = scan(&self.text);
        fixed
    }

    fn last(&self, name: &str) -> Option<&Var> {
        self.vars
            .iter()
            .rev()
            .find(|v| &self.text[v.name.clone()] == name)
    }
}

impl fmt::Display for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// `NAME` when `line` starts with `NAME=`.
fn assignment_name(line: &str) -> Option<&str> {
    let end = line.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))?;
    let name = &line[..end];
    (end > 0 && !name.starts_with(|c: char| c.is_ascii_digit()) && line[end..].starts_with('='))
        .then_some(name)
}

/// Where a line `#NAME=...` ends, to put a new `NAME=` right after it.
fn template_line_end(text: &str, name: &str) -> Option<usize> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line
            .strip_prefix('#')
            .is_some_and(|rest| assignment_name(rest) == Some(name))
        {
            if !line.ends_with('\n') {
                return None;
            }
            return Some(offset + line.len());
        }
        offset += line.len();
    }
    None
}

/// Bare when the shell needs no quotes, double-quoted otherwise.
fn encode(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.,:/+=@%-".contains(c))
    {
        return value.to_owned();
    }
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for c in value.chars() {
        if matches!(c, '"' | '\\' | '$' | '`') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    out
}

/// Finds every `NAME=value` that starts a line, scanning values the way the
/// shell does so that quoted newlines stay inside them.
fn scan(text: &str) -> Vec<Var> {
    let b = text.as_bytes();
    let mut vars = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        if let Some(name) = assignment_name(&text[i..]) {
            let name = i..i + name.len();
            let start = name.end + 1;
            let (end, unterminated) = scan_word(b, start);
            vars.push(Var {
                name,
                value: start..end,
                unterminated,
            });
            i = end;
        }
        // the rest of the line: comments, commands, anything else
        while i < b.len() && b[i] != b'\n' {
            i += 1;
        }
        i += 1;
    }
    vars
}

/// The end of the shell word starting at `i`, and whether it ran into the
/// end of the file inside quotes.
fn scan_word(b: &[u8], mut i: usize) -> (usize, bool) {
    let len = b.len();
    // runs to `close`, skipping backslash escapes when `escapes`
    let until = |mut i: usize, close: u8, escapes: bool| -> Option<usize> {
        while i < len {
            match b[i] {
                b'\\' if escapes => i += 2,
                c if c == close => return Some(i + 1),
                _ => i += 1,
            }
        }
        None
    };
    while i < len {
        match b[i] {
            b' ' | b'\t' | b'\n' | b';' | b'&' | b'|' => return (i, false),
            b'\\' => i = (i + 2).min(len),
            b'\'' => match until(i + 1, b'\'', false) {
                Some(next) => i = next,
                None => return (len, true),
            },
            b'"' => match until(i + 1, b'"', true) {
                Some(next) => i = next,
                None => return (len, true),
            },
            b'`' => match until(i + 1, b'`', true) {
                Some(next) => i = next,
                None => return (len, true),
            },
            b'$' if b.get(i + 1) == Some(&b'(') => {
                let mut depth = 0;
                while i < len {
                    match b[i] {
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                if i >= len {
                    return (len, true);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    (len, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_like_the_shell() {
        let c = Config::parse(
            "# comment\nA=1\nB=\"two words\"\nC='it''s'\nD=\"x\n  y\"  # trailing\n  E=$((1+$A))\nF=\"a\\\"b\\$c\"\nA=last\n",
        );
        assert_eq!(c.get("A").unwrap().literal().as_deref(), Some("last"));
        assert_eq!(c.get("B").unwrap().literal().as_deref(), Some("two words"));
        assert_eq!(c.get("C").unwrap().literal().as_deref(), Some("its"));
        assert_eq!(c.get("D").unwrap().literal().as_deref(), Some("x\n  y"));
        assert_eq!(c.get("E").unwrap().raw(), "$((1+$A))");
        assert_eq!(c.get("E").unwrap().literal(), None);
        assert_eq!(c.get("F").unwrap().literal().as_deref(), Some("a\"b$c"));
        assert!(c.get("comment").is_none());
        assert!(c.problems().is_empty());
    }

    #[test]
    fn set_changes_only_the_value() {
        let text = "# keep\nNFQWS_OPT=\"--old\"  # note\nOTHER=1\n";
        let mut c = Config::parse(text);
        c.set("NFQWS_OPT", "--dpi-desync=fake <HOSTLIST>");
        assert_eq!(
            c.as_str(),
            "# keep\nNFQWS_OPT=\"--dpi-desync=fake <HOSTLIST>\"  # note\nOTHER=1\n"
        );
        c.set("OTHER", "2");
        assert_eq!(c.get("OTHER").unwrap().raw(), "2");
        c.set("NFQWS_OPT", "a\"b $x `y` \\z");
        assert_eq!(
            c.get("NFQWS_OPT").unwrap().literal().as_deref(),
            Some("a\"b $x `y` \\z")
        );
    }

    #[test]
    fn new_variables_go_after_their_template() {
        let mut c = Config::parse("#DISABLE_IPV4=1\n# do not work with ipv6\nX=1");
        c.set("DISABLE_IPV4", "0");
        c.set("NEW", "");
        assert_eq!(
            c.as_str(),
            "#DISABLE_IPV4=1\nDISABLE_IPV4=0\n# do not work with ipv6\nX=1\nNEW=\"\"\n"
        );
    }

    #[test]
    fn repair_leaves_legitimate_multi_line_values_alone() {
        let text = "A=\"\nx\nB=1\"\nC=2\n";
        let mut c = Config::parse(text);
        assert!(c.repair(&Config::parse("")).is_empty());
        assert_eq!(c.as_str(), text);
    }

    #[test]
    fn reports_unterminated_quotes() {
        let c = Config::parse("A=\"open\nB=1\n");
        assert_eq!(
            c.problems(),
            [
                Problem::Swallowed {
                    name: "A".into(),
                    swallowed: vec!["B".into()]
                },
                Problem::Unterminated { name: "A".into() }
            ]
        );
        assert_eq!(c.as_str(), "A=\"open\nB=1\n");
    }
}
