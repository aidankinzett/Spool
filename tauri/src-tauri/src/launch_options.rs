//! Per-game launch options, in the same format as Steam's "Launch Options".
//!
//! Without a `%command%` token the whole string is extra arguments appended
//! after the executable (`-windowed -nolauncher`). With one, the string is a
//! template for the full command line:
//!
//! ```text
//! PROTON_LOG=1 gamemoderun mangohud %command% -windowed
//! └── env ───┘ └──── wrapper ─────┘          └ args ─┘
//! ```
//!
//! Leading `NAME=value` tokens become environment variables, the remaining
//! tokens before `%command%` become a wrapper program that the game command is
//! appended to, and tokens after it are the game's arguments. On a Proton
//! launch `%command%` stands for the whole `umu-run game.exe` invocation, as it
//! does for Proton under Steam.
//!
//! Tokenising is whitespace splitting with `'…'` / `"…"` grouping and no
//! backslash escapes, so Windows paths survive unquoted. There's no shell: no
//! globbing, no `$VAR` expansion, no `;` / `&&`. A leading `~/` on the wrapper
//! program is expanded to the home directory.

use crate::error::{AppError, AppResult};

/// The placeholder for the game's own command line.
const COMMAND_TOKEN: &str = "%command%";

/// A parsed launch-options string. All parts are empty for a blank string.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LaunchOptions {
    /// Environment variables set on the spawned process (wrapper or game).
    pub env: Vec<(String, String)>,
    /// Wrapper program + its arguments, run with the game command appended.
    pub wrapper: Vec<String>,
    /// Arguments passed to the game after its executable.
    pub args: Vec<String>,
}

impl LaunchOptions {
    pub fn is_empty(&self) -> bool {
        self.env.is_empty() && self.wrapper.is_empty() && self.args.is_empty()
    }
}

/// Parses a launch-options string. Errors on an unbalanced quote or more than
/// one `%command%`.
pub fn parse(raw: &str) -> AppResult<LaunchOptions> {
    let tokens = tokenize(raw)?;
    let mut positions = tokens.iter().enumerate().filter(|(_, t)| t.is_command);
    let Some((cmd_at, _)) = positions.next() else {
        return Ok(LaunchOptions {
            args: tokens.into_iter().map(|t| t.text).collect(),
            ..Default::default()
        });
    };
    if positions.next().is_some() {
        return Err(AppError::Other(
            "Launch options can only contain %command% once".into(),
        ));
    }

    let mut tokens = tokens.into_iter();
    let before: Vec<Token> = tokens.by_ref().take(cmd_at).collect();
    let args = tokens.skip(1).map(|t| t.text).collect();

    let mut env = Vec::new();
    let mut wrapper = Vec::new();
    for tok in before {
        if wrapper.is_empty() {
            if let Some(pair) = env_assignment(&tok) {
                env.push(pair);
                continue;
            }
        }
        wrapper.push(tok.text);
    }
    if let Some(program) = wrapper.first_mut() {
        *program = expand_home(program);
    }

    Ok(LaunchOptions { env, wrapper, args })
}

struct Token {
    text: String,
    /// The token was a bare, unquoted `%command%`.
    is_command: bool,
    /// The `=` of a `NAME=value` came before any quote, so the name is literal.
    unquoted_name: bool,
}

fn tokenize(raw: &str) -> AppResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = raw.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            return Ok(tokens);
        }
        let mut text = String::new();
        let mut quoted = false;
        let mut unquoted_name = false;
        while let Some(c) = chars.next_if(|c| !c.is_whitespace()) {
            match c {
                '"' | '\'' => {
                    quoted = true;
                    loop {
                        match chars.next() {
                            Some(q) if q == c => break,
                            Some(other) => text.push(other),
                            None => {
                                return Err(AppError::Other(format!(
                                    "Launch options have an unclosed {c} quote"
                                )))
                            }
                        }
                    }
                }
                '=' if !quoted && !text.contains('=') => {
                    unquoted_name = true;
                    text.push(c);
                }
                _ => text.push(c),
            }
        }
        tokens.push(Token {
            is_command: !quoted && text == COMMAND_TOKEN,
            text,
            unquoted_name,
        });
    }
}

/// `NAME=value` with a valid shell variable name, else `None`.
fn env_assignment(tok: &Token) -> Option<(String, String)> {
    if !tok.unquoted_name {
        return None;
    }
    let (name, value) = tok.text.split_once('=')?;
    let mut chars = name.chars();
    let valid = chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| (name.to_string(), value.to_string()))
}

fn expand_home(program: &str) -> String {
    match (program.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => program.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn blank_is_empty() {
        assert!(parse("").unwrap().is_empty());
        assert!(parse("   ").unwrap().is_empty());
    }

    #[test]
    fn without_command_everything_is_args() {
        let o = parse("-windowed  -nolauncher FOO=1").unwrap();
        assert_eq!(o.args, s(&["-windowed", "-nolauncher", "FOO=1"]));
        assert!(o.env.is_empty() && o.wrapper.is_empty());
    }

    #[test]
    fn wrapper_command_and_args() {
        let o = parse("/home/u/.local/bin/linuwux %command% -dx11").unwrap();
        assert_eq!(o.wrapper, s(&["/home/u/.local/bin/linuwux"]));
        assert_eq!(o.args, s(&["-dx11"]));
        assert!(o.env.is_empty());
    }

    #[test]
    fn leading_assignments_are_env() {
        let o = parse("PROTON_LOG=1 DXVK_HUD=fps gamemoderun mangohud %command%").unwrap();
        assert_eq!(
            o.env,
            vec![
                ("PROTON_LOG".into(), "1".into()),
                ("DXVK_HUD".into(), "fps".into())
            ]
        );
        assert_eq!(o.wrapper, s(&["gamemoderun", "mangohud"]));
        assert!(o.args.is_empty());
    }

    #[test]
    fn env_only() {
        let o = parse("WINEDLLOVERRIDES=\"dinput8=n,b\" %command%").unwrap();
        assert_eq!(
            o.env,
            vec![("WINEDLLOVERRIDES".into(), "dinput8=n,b".into())]
        );
        assert!(o.wrapper.is_empty());
    }

    #[test]
    fn assignment_after_wrapper_is_a_wrapper_arg() {
        let o = parse("env FOO=1 %command%").unwrap();
        assert!(o.env.is_empty());
        assert_eq!(o.wrapper, s(&["env", "FOO=1"]));
    }

    #[test]
    fn quoted_name_or_invalid_name_is_not_env() {
        let o = parse("\"A=1\" 1B=2 %command%").unwrap();
        assert!(o.env.is_empty());
        assert_eq!(o.wrapper, s(&["A=1", "1B=2"]));
    }

    #[test]
    fn quotes_group_and_windows_paths_survive() {
        let o = parse(r#""C:\Program Files\Wrap\wrap.exe" %command% -name 'Big Bob'"#).unwrap();
        assert_eq!(o.wrapper, s(&[r"C:\Program Files\Wrap\wrap.exe"]));
        assert_eq!(o.args, s(&["-name", "Big Bob"]));
    }

    #[test]
    fn quoted_command_is_literal() {
        let o = parse("wrap '%command%'").unwrap();
        assert_eq!(o.args, s(&["wrap", "%command%"]));
        assert!(o.wrapper.is_empty());
    }

    #[test]
    fn tilde_expands_on_the_wrapper_program_only() {
        let home = dirs::home_dir().unwrap();
        let o = parse("~/bin/wrap ~/x %command%").unwrap();
        assert_eq!(o.wrapper[0], home.join("bin/wrap").to_string_lossy());
        assert_eq!(o.wrapper[1], "~/x");
    }

    #[test]
    fn errors() {
        assert!(parse("wrap %command% %command%").is_err());
        assert!(parse("wrap \"unterminated %command%").is_err());
    }
}
