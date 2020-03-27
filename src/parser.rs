use std::char;
use std::env::{var, VarError};
use std::io::{BufRead, BufWriter, Write};

use anyhow::Result;

const VARIABLE: char = b'$' as char;
const START: char = b'{' as char;
const END: char = b'}' as char;
const VALID_CHARS: [char; 1] = [b'_' as char];

pub struct Parser<R, W>
where
    R: BufRead,
    W: Write,
{
    input: R,
    output: BufWriter<W>,
    fail_when_not_found: bool,

    current_variable_name: String,
    parsing_variable: bool,
    open_braces: bool,
}

impl<R, W> Parser<R, W>
where
    R: BufRead,
    W: Write,
{
    pub fn new(input: R, output: W, fail_when_not_found: bool) -> Self {
        Self {
            input,
            output: BufWriter::new(output),
            fail_when_not_found,
            current_variable_name: "".to_owned(),
            parsing_variable: false,
            open_braces: false,
        }
    }

    pub fn process(&mut self) -> Result<()> {
        let mut line = String::new();
        let mut line_number = 0;
        loop {
            if self.input.read_line(&mut line)? == 0 {
                break;
            };
            line_number += 1;

            for current_char in line.chars() {
                self.parse_char(current_char)?;
            }
            if self.parsing_variable && !self.open_braces {
                self.write_variable()?;
            }
            line.clear();
        }

        if self.parsing_variable {
            anyhow::bail!(
                "Failed to parse a variable on line {} missing a '}}' after '{}'",
                line_number,
                self.current_variable_name
            );
        }
        Ok(())
    }

    fn parse_char(&mut self, current_char: char) -> Result<()> {
        if self.start_parsing_variable(&current_char)? {
            return Ok(());
        }

        if self.check_braces_opening(&current_char)? {
            return Ok(());
        }

        if self.check_braces_ending(&current_char)? {
            return Ok(());
        }

        if self.check_whitespace(&current_char)? {
            return Ok(());
        }

        if self.parsing_variable {
            if VALID_CHARS.contains(&current_char) || current_char.is_alphabetic() {
                self.current_variable_name.push(current_char);
                return Ok(());
            }

            if self.open_braces {
                anyhow::bail!(
                    "Failed to parse variable {} with extra character '{}'",
                    &self.current_variable_name,
                    current_char
                );
            }
            self.write_variable()?;
            self.reset_state();
        }

        self.write_char(&current_char)?;

        Ok(())
    }

    fn start_parsing_variable(&mut self, current_char: &char) -> Result<bool> {
        if *current_char == VARIABLE {
            if self.parsing_variable {
                anyhow::bail!("Variable is already being parsed")
            }
            self.parsing_variable = true;
            return Ok(true);
        }

        Ok(false)
    }

    fn check_braces_opening(&mut self, current_char: &char) -> Result<bool> {
        if *current_char == START && self.parsing_variable {
            if self.open_braces {
                anyhow::bail!("Double open braces")
            }
            self.open_braces = true;
            return Ok(true);
        }

        Ok(false)
    }

    fn check_braces_ending(&mut self, current_char: &char) -> Result<bool> {
        if *current_char == END && self.parsing_variable {
            if !self.open_braces {
                anyhow::bail!("Closing braces without opening");
            }
            self.write_variable()?;
            return Ok(true);
        }

        Ok(false)
    }

    fn check_whitespace(&mut self, current_char: &char) -> Result<bool> {
        if current_char.is_ascii_whitespace() && self.parsing_variable {
            if self.open_braces {
                anyhow::bail!("Braces not closed");
            }
            self.write_variable()?;
            self.write_char(current_char)?;
            return Ok(true);
        }
        Ok(false)
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
        self.open_braces = false;
        self.parsing_variable = false;
        self.current_variable_name.clear();
    }

    fn write_char(&mut self, current_char: &char) -> Result<()> {
        // TODO: No way to access bytes from char?
        self.output.write_all(current_char.to_string().as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::env::set_var;
    use std::io::{BufReader, Cursor};

    use crate::parser::Parser;

    fn render(template: &str, expected: &str, fail_when_not_found: bool) {
        let mut input = BufReader::new(Cursor::new(template));
        let mut output = Cursor::new(Vec::new());
        {
            let mut parser = Parser::new(&mut input, &mut output, fail_when_not_found);
            parser.process().unwrap();
        }
        let output = String::from_utf8(output.into_inner()).unwrap();
        assert_eq!(output, expected);
    }

    #[test]
    fn test_simple_variable() {
        set_var("TEST_SIMPLE", "simple return");
        render("$TEST_SIMPLE", "simple return", true);
    }

    #[test]
    fn test_simple_quoted_variable() {
        set_var("TEST_SIMPLE", "simple return");
        render("'$TEST_SIMPLE'", "'simple return'", true);
    }

    #[test]
    fn test_with_braces() {
        set_var("TEST_BRACES", "braces return");
        render("${TEST_BRACES}", "braces return", true);
    }

    #[test]
    fn test_with_quoted_braces() {
        set_var("TEST_BRACES", "braces return");
        render("'${TEST_BRACES}'", "'braces return'", true);
    }

    #[test]
    fn test_mixed() {
        set_var("TEST_SIMPLE", "simple return");
        set_var("TEST_BRACES", "braces return");
        render(
            "simple: $TEST_SIMPLE\nbraces: ${TEST_BRACES}",
            "simple: simple return\nbraces: braces return",
            true,
        );
    }

    #[test]
    fn test_missing() {
        for template in &["$TEST_MISSING", "${TEST_MISSING}"] {
            render(template, "", false);
        }
    }
}
