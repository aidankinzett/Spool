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
//! program or an env value is expanded to the home directory.
//!
//! A string without `%command%` that doesn't tokenise (a stray apostrophe, as in
//! `-name O'Brien`) falls back to plain whitespace splitting, which is how
//! launch arguments were split before `%command%` support. `WINEPREFIX` is
//! rejected: Spool restores and backs up saves from the prefix set in the
//! editor's Wine prefix field, so a prefix set here would put saves where
//! backups don't look.

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

/// Env vars that launch options may not set, with the reason shown to the user.
const RESERVED_ENV: &[(&str, &str)] = &[(
    "WINEPREFIX",
    "Set the prefix in the Wine prefix field instead of WINEPREFIX in launch options, so Spool backs up saves from the same prefix the game uses",
)];

/// Parses a launch-options string. With `%command%` present, errors on an
/// unbalanced quote, more than one `%command%`, or a reserved env var. Either
/// way, errors on `%command%` joined to other text (`mangohud%command%`).
pub fn parse(raw: &str) -> AppResult<LaunchOptions> {
    let tokens = match tokenize(raw) {
        Ok(tokens) => tokens,
        Err(_) if !raw.contains(COMMAND_TOKEN) => {
            return Ok(LaunchOptions {
                args: raw.split_whitespace().map(String::from).collect(),
                ..Default::default()
            })
        }
        Err(e) => return Err(e),
    };
    if tokens
        .iter()
        .any(|t| !t.quoted && !t.is_command && t.text.contains(COMMAND_TOKEN))
    {
        return Err(AppError::Other(
            "Put spaces around %command% in launch options".into(),
        ));
    }
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
            if let Some((name, value)) = env_assignment(&tok) {
                if let Some((_, why)) = RESERVED_ENV.iter().find(|(r, _)| *r == name) {
                    return Err(AppError::Other((*why).into()));
                }
                env.push((name, expand_home(&value)));
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
    /// Some part of the token was quoted.
    quoted: bool,
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
            quoted,
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

fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).to_string_lossy().into_owned(),
        _ => path.to_string(),
    }
}

/// Checks a launch-options string without launching, so the editor can show
/// the same error a launch would hit.
#[tauri::command]
pub fn validate_launch_options(raw: String) -> AppResult<()> {
    parse(&raw).map(|_| ())
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
        assert!(parse("mangohud%command%").is_err());
        assert!(parse("wrap %command%;").is_err());
        assert!(parse("WINEPREFIX=/tmp/pfx %command%").is_err());
    }

    #[test]
    fn unbalanced_quote_without_command_splits_on_whitespace() {
        let o = parse("-name O'Brien -mod \"x").unwrap();
        assert_eq!(o.args, s(&["-name", "O'Brien", "-mod", "\"x"]));
    }

    #[test]
    fn empty_quoted_argument_is_kept() {
        let o = parse(r#"-name "" -x"#).unwrap();
        assert_eq!(o.args, s(&["-name", "", "-x"]));
    }

    #[test]
    fn adjacent_quoted_segments_join_into_one_token() {
        let o = parse(r#"wrap %command% --path="C:\My Games"\save"#).unwrap();
        assert_eq!(o.args, s(&[r"--path=C:\My Games\save"]));
    }

    #[test]
    fn env_value_may_be_empty_or_contain_equals() {
        let o = parse("EMPTY= A=b=c %command%").unwrap();
        assert_eq!(
            o.env,
            vec![("EMPTY".into(), "".into()), ("A".into(), "b=c".into())]
        );
    }

    #[test]
    fn command_alone_has_no_wrapper_env_or_args() {
        let o = parse("%command%").unwrap();
        assert!(o.is_empty());
    }

    #[test]
    fn wineprefix_is_only_rejected_as_a_leading_assignment() {
        // After the wrapper starts, `WINEPREFIX=x` is just an argument to it.
        let o = parse("env WINEPREFIX=/tmp/x %command%").unwrap();
        assert_eq!(o.wrapper, s(&["env", "WINEPREFIX=/tmp/x"]));
        // A quoted name isn't an assignment either.
        assert!(parse("'WINEPREFIX=/tmp/x' %command%").is_ok());
        // Game args after %command% are the game's business.
        assert!(parse("%command% WINEPREFIX=/tmp/x").is_ok());
    }

    #[test]
    fn unquoted_command_inside_a_quoted_token_is_an_error_only_when_joined() {
        assert!(parse("'a %command%' %command%").is_ok());
        assert!(parse("--x=%command%").is_err());
    }

    #[test]
    fn tilde_without_a_slash_is_not_expanded() {
        let o = parse("~wrap %command%").unwrap();
        assert_eq!(o.wrapper, s(&["~wrap"]));
    }

    #[test]
    fn validate_command_mirrors_parse() {
        assert!(validate_launch_options("a %command% b".into()).is_ok());
        assert!(validate_launch_options("a %command% %command%".into()).is_err());
    }

    #[test]
    fn tilde_expands_in_env_values() {
        let home = dirs::home_dir().unwrap();
        let o = parse("DXVK_CONFIG_FILE=~/dxvk.conf %command%").unwrap();
        assert_eq!(o.env[0].1, home.join("dxvk.conf").to_string_lossy());
    }
}
