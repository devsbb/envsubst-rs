use std::char;
use std::env::{var, VarError};
use std::io::{BufRead, BufWriter, Write};

use anyhow::Result;

const START: char = b'{' as char;
const END: char = b'}' as char;
const VALID_CHARS: [char; 1] = [b'_' as char];

#[derive(Debug, PartialEq)]
enum State {
    TextOutput,
    ParsingVariable,
    OpenBraces,
}

#[derive(Debug, PartialEq)]
enum ParseCharResult {
    Consumed,
    Ignored,
}

pub struct Parser<R, W>
where
    R: BufRead,
    W: Write,
{
    input: R,
    output: BufWriter<W>,
    fail_when_not_found: bool,
    delimiter: char,

    current_variable_name: String,
    state: State,
}

impl<R, W> Parser<R, W>
where
    R: BufRead,
    W: Write,
{
    pub fn new(input: R, output: W, fail_when_not_found: bool, delimiter: Option<char>) -> Self {
        Self {
            input,
            output: BufWriter::new(output),
            fail_when_not_found,
            delimiter: delimiter.unwrap_or_else(default_delimiter),
            current_variable_name: "".to_owned(),
            state: State::TextOutput,
        }
    }

    pub fn process(&mut self) -> Result<()> {
        let mut line = String::new();
        let mut last_processed_line = 0;
        for line_number in 1.. {
            if self.input.read_line(&mut line)? == 0 {
                break;
            };
            last_processed_line = line_number;

            for current_char in line.chars() {
                self.parse_char(current_char)?;
            }
            if self.state == State::ParsingVariable {
                self.write_variable()?;
            }
            line.clear();
        }

        if self.state != State::TextOutput {
            anyhow::bail!(
                "Failed to parse a variable on line {} missing a '}}' after '{}'",
                last_processed_line,
                self.current_variable_name
            );
        }
        Ok(())
    }

    fn parse_char(&mut self, current_char: char) -> Result<()> {
        if self.start_parsing_variable(current_char)? == ParseCharResult::Consumed {
            return Ok(());
        }

        if self.check_braces_opening(current_char)? == ParseCharResult::Consumed {
            return Ok(());
        }

        if self.check_braces_ending(current_char)? == ParseCharResult::Consumed {
            return Ok(());
        }

        if self.check_whitespace(current_char)? == ParseCharResult::Consumed {
            return Ok(());
        }

        if self.state == State::ParsingVariable || self.state == State::OpenBraces {
            if VALID_CHARS.contains(&current_char) || current_char.is_alphabetic() {
                self.current_variable_name.push(current_char);
                return Ok(());
            }

            if self.state == State::OpenBraces {
                anyhow::bail!(
                    "Failed to parse variable {} with extra character '{}'",
                    &self.current_variable_name,
                    current_char
                );
            }
            self.write_variable()?;
            self.reset_state();
        }

        self.write_char(current_char)?;

        Ok(())
    }

    fn start_parsing_variable(&mut self, current_char: char) -> Result<ParseCharResult> {
        if current_char == self.delimiter {
            if self.state == State::ParsingVariable {
                anyhow::bail!("Variable is already being parsed")
            }
            self.state = State::ParsingVariable;
            return Ok(ParseCharResult::Consumed);
        }

        Ok(ParseCharResult::Ignored)
    }

    fn check_braces_opening(&mut self, current_char: char) -> Result<ParseCharResult> {
        if current_char != START {
            return Ok(ParseCharResult::Ignored);
        }

        if self.state == State::ParsingVariable {
            self.state = State::OpenBraces;
            return Ok(ParseCharResult::Consumed);
        }

        if self.state == State::OpenBraces {
            anyhow::bail!("Double open braces")
        }

        Ok(ParseCharResult::Ignored)
    }

    fn check_braces_ending(&mut self, current_char: char) -> Result<ParseCharResult> {
        if current_char != END {
            return Ok(ParseCharResult::Ignored);
        }

        if self.state == State::OpenBraces {
            self.write_variable()?;
            return Ok(ParseCharResult::Consumed);
        }

        if self.state == State::ParsingVariable {
            anyhow::bail!("Closing braces without opening");
        }

        Ok(ParseCharResult::Ignored)
    }

    fn check_whitespace(&mut self, current_char: char) -> Result<ParseCharResult> {
        if self.state != State::ParsingVariable && self.state != State::OpenBraces {
            return Ok(ParseCharResult::Ignored);
        }
        if current_char.is_ascii_whitespace() {
            if self.state == State::OpenBraces {
                anyhow::bail!("Braces not closed");
            }
            self.write_variable()?;
            self.write_char(current_char)?;
            return Ok(ParseCharResult::Consumed);
        }
        Ok(ParseCharResult::Ignored)
    }

    fn write_variable(&mut self) -> Result<()> {
        let result = match var(&self.current_variable_name) {
            Ok(result) => result,
            Err(VarError::NotPresent) => {
                if self.fail_when_not_found {
                    anyhow::bail!("The variable {} is not set", self.current_variable_name)
                }
                "".to_owned()
            }
            Err(error) => {
                return Err(anyhow::Error::new(error).context(format!(
                    "failed to read contents of variable {}",
                    &self.current_variable_name
                )));
            }
        };

        self.output.write_all(result.as_bytes())?;
        self.reset_state();
        Ok(())
    }

    fn reset_state(&mut self) {
        self.state = State::TextOutput;
        self.current_variable_name.clear();
    }

    fn write_char(&mut self, current_char: char) -> Result<()> {
        // TODO: No way to access bytes from char?
        self.output.write_all(current_char.to_string().as_bytes())?;
        Ok(())
    }
}

pub fn default_delimiter() -> char {
    b'$' as char
}

#[cfg(test)]
mod tests {
    use std::env::set_var;
    use std::io::{BufReader, Cursor};

    use crate::parser::Parser;
    use std::panic;

    fn render(template: &str, expected: &str, fail_when_not_found: bool, delimiter: Option<char>) {
        let mut input = BufReader::new(Cursor::new(template));
        let mut output = Cursor::new(Vec::new());
        {
            let mut parser = Parser::new(&mut input, &mut output, fail_when_not_found, delimiter);
            parser.process().unwrap();
        }
        let output = String::from_utf8(output.into_inner()).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_simple_variable() {
        set_var("TEST_SIMPLE", "simple return");
        render("$TEST_SIMPLE", "simple return", true, None);
    }

    #[test]
    fn test_simple_variable_with_delimiter() {
        set_var("TEST_SIMPLE", "simple return");
        render("👻TEST_SIMPLE", "simple return", true, Some('👻'));
    }

    #[test]
    fn test_simple_quoted_variable() {
        set_var("TEST_SIMPLE", "simple return");
        render("'$TEST_SIMPLE'", "'simple return'", true, None);
    }

    #[test]
    fn test_with_braces() {
        set_var("TEST_BRACES", "braces return");
        render("${TEST_BRACES}", "braces return", true, None);
    }

    #[test]
    fn test_with_quoted_braces() {
        set_var("TEST_BRACES", "braces return");
        render("'${TEST_BRACES}'", "'braces return'", true, None);
    }

    #[test]
    fn test_mixed() {
        set_var("TEST_SIMPLE", "simple return");
        set_var("TEST_BRACES", "braces return");
        render(
            "simple: $TEST_SIMPLE\nbraces: ${TEST_BRACES}",
            "simple: simple return\nbraces: braces return",
            true,
            None,
        );
    }

    #[test]
    fn test_missing() {
        for template in &["$TEST_MISSING", "${TEST_MISSING}"] {
            render(template, "", false, None);
        }
    }

    #[test]
    fn test_open_braces() {
        let mut input = BufReader::new(Cursor::new("${OPEN_BRACES"));
        let mut output = Cursor::new(Vec::new());

        let mut parser = Parser::new(&mut input, &mut output, true, None);
        let result = parser.process();
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert_eq!(
            error.to_string(),
            "Failed to parse a variable on line 1 missing a '}' after 'OPEN_BRACES'"
        );
    }
}
