//! Tokens of the card DSL. Lines matter: a statement ends at a newline
//! unless it opens a block with `{`.

use crate::parse::ParseError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    /// A word: letters, digits, apostrophes, and hyphens (`can't`,
    /// `non-Aura`, `Assembly-Worker`, a UUID).
    Word(String),
    /// A quoted string.
    Str(String),
    /// An unsigned number.
    Num(u32),
    /// A signed number (`+1`, `-6`, `−6`) not followed by `/`.
    Signed(i32),
    /// `2/2`, `+1/+1`, `-2/-2`, `+2/-2`: power/toughness, with whether the
    /// parts were signed (a counter kind or a modification) or not (a base
    /// value).
    Pt {
        power: i32,
        toughness: i32,
        signed: bool,
    },
    /// A mana or tap symbol: the text between braces (`R`, `10`, `X`, `T`).
    Sym(String),
    LBrace,
    RBrace,
    Colon,
    Comma,
    Semi,
    /// `—` or a free-standing `-`: separates subtypes in a type line.
    Dash,
    Newline,
}

impl std::fmt::Display for Tok {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tok::Word(w) => write!(f, "{w}"),
            Tok::Str(s) => write!(f, "\"{s}\""),
            Tok::Num(n) => write!(f, "{n}"),
            Tok::Signed(n) => write!(f, "{n:+}"),
            Tok::Pt {
                power,
                toughness,
                signed,
            } => {
                if *signed {
                    write!(f, "{power:+}/{toughness:+}")
                } else {
                    write!(f, "{power}/{toughness}")
                }
            }
            Tok::Sym(s) => write!(f, "{{{s}}}"),
            Tok::LBrace => write!(f, "{{"),
            Tok::RBrace => write!(f, "}}"),
            Tok::Colon => write!(f, ":"),
            Tok::Comma => write!(f, ","),
            Tok::Semi => write!(f, ";"),
            Tok::Dash => write!(f, "—"),
            Tok::Newline => write!(f, "end of line"),
        }
    }
}

/// A token with the line it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spanned {
    pub tok: Tok,
    pub line: usize,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\'' || c == '-' || c == '’' || c == '_'
}

fn is_sym_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '/'
}

/// Try to read a `[+-]?N/[+-]?N` power/toughness at the start of `s`;
/// returns the token and the number of bytes consumed. A printed `*` half
/// (`pt */1`, defined by a characteristic-defining ability, CR 604.3) is
/// 0, as it is outside the battlefield (CR 208.2a).
fn lex_pt(text: &str) -> Option<(Tok, usize)> {
    let b = text.as_bytes();
    let mut i = 0;
    let sign = |b: &[u8], i: &mut usize| -> Option<i32> {
        match b.get(*i) {
            Some(b'+') => {
                *i += 1;
                Some(1)
            }
            Some(b'-') => {
                *i += 1;
                Some(-1)
            }
            _ => None,
        }
    };
    let mut star_halves = 0;
    let mut digits = |b: &[u8], i: &mut usize| -> Option<i32> {
        if b.get(*i) == Some(&b'*') {
            *i += 1;
            star_halves += 1;
            return Some(0);
        }
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        if *i == start {
            return None;
        }
        std::str::from_utf8(&b[start..*i]).ok()?.parse().ok()
    };
    let power_sign = sign(b, &mut i);
    let power = digits(b, &mut i)?;
    if b.get(i) != Some(&b'/') {
        return None;
    }
    i += 1;
    let toughness_sign = sign(b, &mut i);
    let toughness = digits(b, &mut i)?;
    if b.get(i).is_some_and(u8::is_ascii_alphanumeric) {
        return None;
    }
    let signed = power_sign.is_some() || toughness_sign.is_some();
    if signed && (power_sign.is_none() || toughness_sign.is_none()) {
        return None;
    }
    if star_halves > 1 || (star_halves == 1 && signed) {
        return None;
    }
    Some((
        Tok::Pt {
            power: power * power_sign.unwrap_or(1),
            toughness: toughness * toughness_sign.unwrap_or(1),
            signed,
        },
        i,
    ))
}

/// The longest prefix of `s` whose characters all satisfy `f`: a slice,
/// so a token is copied out once (the whole corpus is lexed at start-up).
fn prefix(s: &str, f: impl Fn(char) -> bool) -> &str {
    let end = s
        .char_indices()
        .find(|(_, c)| !f(*c))
        .map_or(s.len(), |(i, _)| i);
    &s[..end]
}

#[allow(clippy::too_many_lines)]
pub fn lex(text: &str) -> Result<Vec<Spanned>, ParseError> {
    // About one token per five bytes of card text.
    let mut out: Vec<Spanned> = Vec::with_capacity(text.len() / 4);
    for (idx, raw) in text.lines().enumerate() {
        let line = idx + 1;
        let mut rest = raw;
        let mut any = false;
        loop {
            rest = rest.trim_start_matches([' ', '\t', '\r']);
            let Some(c) = rest.chars().next() else {
                break;
            };
            if c == '#' || rest.starts_with("//") {
                break;
            }
            let push = |out: &mut Vec<Spanned>, tok: Tok| out.push(Spanned { tok, line });
            if let Some((tok, n)) = lex_pt(rest) {
                push(&mut out, tok);
                rest = &rest[n..];
                any = true;
                continue;
            }
            let n = match c {
                '{' => {
                    let inner = prefix(&rest[1..], is_sym_char);
                    if !inner.is_empty() && rest[1 + inner.len()..].starts_with('}') {
                        push(&mut out, Tok::Sym(inner.to_string()));
                        inner.len() + 2
                    } else {
                        push(&mut out, Tok::LBrace);
                        1
                    }
                }
                '}' => {
                    push(&mut out, Tok::RBrace);
                    1
                }
                ':' => {
                    push(&mut out, Tok::Colon);
                    1
                }
                ',' => {
                    push(&mut out, Tok::Comma);
                    1
                }
                ';' => {
                    push(&mut out, Tok::Semi);
                    1
                }
                '—' => {
                    push(&mut out, Tok::Dash);
                    '—'.len_utf8()
                }
                '"' => {
                    let Some(end) = rest[1..].find('"') else {
                        return Err(ParseError {
                            line,
                            message: "unterminated string".to_string(),
                        });
                    };
                    push(&mut out, Tok::Str(rest[1..=end].to_string()));
                    end + 2
                }
                '+' | '-' | '−' => {
                    let w = c.len_utf8();
                    let digits = prefix(&rest[w..], |c| c.is_ascii_digit());
                    if digits.is_empty() {
                        if c == '-' {
                            push(&mut out, Tok::Dash);
                            1
                        } else {
                            return Err(ParseError {
                                line,
                                message: format!("unexpected {c:?}"),
                            });
                        }
                    } else {
                        let n: i32 = digits.parse().map_err(|_| ParseError {
                            line,
                            message: format!("number too large: {digits}"),
                        })?;
                        push(&mut out, Tok::Signed(if c == '+' { n } else { -n }));
                        w + digits.len()
                    }
                }
                c if c.is_ascii_digit() => {
                    let word = prefix(rest, is_word_char);
                    if word.chars().all(|c| c.is_ascii_digit()) {
                        let n: u32 = word.parse().map_err(|_| ParseError {
                            line,
                            message: format!("number too large: {word}"),
                        })?;
                        push(&mut out, Tok::Num(n));
                    } else {
                        push(&mut out, Tok::Word(word.to_string()));
                    }
                    word.len()
                }
                c if c.is_alphabetic() => {
                    let word = prefix(rest, is_word_char);
                    push(&mut out, Tok::Word(word.to_string()));
                    word.len()
                }
                _ => {
                    return Err(ParseError {
                        line,
                        message: format!("unexpected character {c:?}"),
                    });
                }
            };
            rest = &rest[n..];
            any = true;
        }
        if any {
            out.push(Spanned {
                tok: Tok::Newline,
                line,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        lex(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn lexes_symbols_and_pt() {
        assert_eq!(
            toks("cost {1}{G} pt 2/2 # c"),
            vec![
                Tok::Word("cost".into()),
                Tok::Sym("1".into()),
                Tok::Sym("G".into()),
                Tok::Word("pt".into()),
                Tok::Pt {
                    power: 2,
                    toughness: 2,
                    signed: false
                },
                Tok::Newline
            ]
        );
        assert_eq!(
            toks("+1/+1 -2/-2 +2/-2 +1 −6 {"),
            vec![
                Tok::Pt {
                    power: 1,
                    toughness: 1,
                    signed: true
                },
                Tok::Pt {
                    power: -2,
                    toughness: -2,
                    signed: true
                },
                Tok::Pt {
                    power: 2,
                    toughness: -2,
                    signed: true
                },
                Tok::Signed(1),
                Tok::Signed(-6),
                Tok::LBrace,
                Tok::Newline
            ]
        );
    }

    #[test]
    fn lexes_words() {
        assert_eq!(
            toks("type Creature — Assembly-Worker can't 2b93787d-47ea-4b8b-912d-ffc5d5440171"),
            vec![
                Tok::Word("type".into()),
                Tok::Word("Creature".into()),
                Tok::Dash,
                Tok::Word("Assembly-Worker".into()),
                Tok::Word("can't".into()),
                Tok::Word("2b93787d-47ea-4b8b-912d-ffc5d5440171".into()),
                Tok::Newline
            ]
        );
    }
}
