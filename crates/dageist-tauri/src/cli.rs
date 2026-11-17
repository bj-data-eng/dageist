//! The command line: which of the binary's jobs to run.
//!
//! The binary has no command-line design yet, so its few arguments are read
//! here with the standard library. With none it opens the application; the
//! subcommand `bridge` serves the host's commands to a browser instead.

use std::ffi::OsString;
use std::fmt;

/// The port the bridge listens on unless it is given one.
pub const DEFAULT_BRIDGE_PORT: u16 = 7878;

/// What the usage message says, after the problem.
pub const USAGE: &str = "usage: dageist [bridge [--port <port>]]\n\
    \n  dageist                    open the application\
    \n  dageist bridge [--port N]  serve the host's commands to a browser on 127.0.0.1 (default port 7878)";

/// What the binary was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Invocation {
    /// Open the application window.
    Application,
    /// Serve the host's commands over HTTP on the loopback address, without a window.
    Bridge { port: u16 },
}

/// Why the arguments are not a command the binary knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageError {
    /// An argument that is not valid Unicode.
    NotUnicode,
    /// A first argument that names no subcommand.
    UnknownCommand(String),
    /// An option the subcommand does not take.
    UnknownOption(String),
    /// `--port` without a value.
    MissingPort,
    /// A `--port` value that is not a port number, 0 to 65535.
    InvalidPort(String),
    /// `--port` given twice.
    RepeatedPort,
}

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotUnicode => write!(formatter, "an argument is not valid Unicode"),
            Self::UnknownCommand(name) => write!(formatter, "unknown command `{name}`"),
            Self::UnknownOption(name) => write!(formatter, "unknown option `{name}`"),
            Self::MissingPort => write!(formatter, "`--port` needs a port number"),
            Self::InvalidPort(value) => {
                write!(formatter, "`{value}` is not a port number from 0 to 65535")
            }
            Self::RepeatedPort => write!(formatter, "`--port` is given more than once"),
        }
    }
}

impl std::error::Error for UsageError {}

/// Reads the arguments after the program's name.
///
/// # Errors
///
/// Returns the first argument that is not part of a command the binary knows.
pub fn parse(arguments: impl IntoIterator<Item = OsString>) -> Result<Invocation, UsageError> {
    let mut arguments = arguments.into_iter().map(|argument| {
        argument
            .into_string()
            .map_err(|_unreadable| UsageError::NotUnicode)
    });
    let Some(first) = arguments.next().transpose()? else {
        return Ok(Invocation::Application);
    };
    if first != "bridge" {
        return Err(UsageError::UnknownCommand(first));
    }
    let mut port = None;
    while let Some(argument) = arguments.next().transpose()? {
        let value = if argument == "--port" {
            arguments
                .next()
                .transpose()?
                .ok_or(UsageError::MissingPort)?
        } else if let Some(value) = argument.strip_prefix("--port=") {
            value.to_owned()
        } else {
            return Err(UsageError::UnknownOption(argument));
        };
        let number = value
            .parse::<u16>()
            .map_err(|_invalid| UsageError::InvalidPort(value))?;
        if port.replace(number).is_some() {
            return Err(UsageError::RepeatedPort);
        }
    }
    Ok(Invocation::Bridge {
        port: port.unwrap_or(DEFAULT_BRIDGE_PORT),
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{DEFAULT_BRIDGE_PORT, Invocation, UsageError, parse};

    fn arguments(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    #[test]
    fn no_arguments_open_the_application() {
        assert_eq!(parse(arguments(&[])), Ok(Invocation::Application));
    }

    #[test]
    fn the_bridge_listens_on_its_default_port_unless_given_one() {
        assert_eq!(
            parse(arguments(&["bridge"])),
            Ok(Invocation::Bridge {
                port: DEFAULT_BRIDGE_PORT
            })
        );
    }

    #[test]
    fn the_bridge_takes_a_port_as_a_separate_argument_or_after_an_equals_sign() {
        let expected = Ok(Invocation::Bridge { port: 9000 });

        assert_eq!(parse(arguments(&["bridge", "--port", "9000"])), expected);
        assert_eq!(parse(arguments(&["bridge", "--port=9000"])), expected);
    }

    #[test]
    fn port_zero_asks_the_system_for_a_free_port() {
        assert_eq!(
            parse(arguments(&["bridge", "--port", "0"])),
            Ok(Invocation::Bridge { port: 0 })
        );
    }

    #[test]
    fn a_port_that_is_not_a_number_in_range_is_refused_with_its_text() {
        for text in ["http", "65536", "-1", "", "8 080"] {
            assert_eq!(
                parse(arguments(&["bridge", "--port", text])),
                Err(UsageError::InvalidPort(text.to_owned())),
                "port {text:?}"
            );
        }
    }

    #[test]
    fn a_port_option_without_a_value_is_refused() {
        assert_eq!(
            parse(arguments(&["bridge", "--port"])),
            Err(UsageError::MissingPort)
        );
    }

    #[test]
    fn a_port_given_twice_is_refused() {
        assert_eq!(
            parse(arguments(&["bridge", "--port", "1", "--port=2"])),
            Err(UsageError::RepeatedPort)
        );
    }

    #[test]
    fn an_unknown_option_or_command_is_refused_by_name() {
        assert_eq!(
            parse(arguments(&["bridge", "--host", "0.0.0.0"])),
            Err(UsageError::UnknownOption("--host".to_owned()))
        );
        assert_eq!(
            parse(arguments(&["bridge", "9000"])),
            Err(UsageError::UnknownOption("9000".to_owned()))
        );
        assert_eq!(
            parse(arguments(&["serve"])),
            Err(UsageError::UnknownCommand("serve".to_owned()))
        );
        assert_eq!(
            parse(arguments(&["--port", "9000"])),
            Err(UsageError::UnknownCommand("--port".to_owned()))
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_argument_that_is_not_unicode_is_refused() {
        use std::os::unix::ffi::OsStringExt;

        assert_eq!(
            parse([OsString::from_vec(vec![0xff, 0xfe])]),
            Err(UsageError::NotUnicode)
        );
    }
}
