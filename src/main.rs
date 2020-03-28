use std::fs::File;
use std::io::{BufRead, BufReader, stdin, stdout, Write};
use std::path::PathBuf;

use anyhow::Result;
use structopt::StructOpt;

use envsubst::Parser;

#[derive(Debug, StructOpt)]
struct Config {
    #[structopt(long, short)]
    pub input: Option<PathBuf>,
    #[structopt(long, short)]
    pub output: Option<PathBuf>,
    #[structopt(long, short, help = "Fail if a variable could not be found")]
    pub fail: bool,
}

fn main() -> Result<()> {
    let config: Config = Config::from_args();
    let input: Box<dyn BufRead> = if let Some(input_file) = config.input {
        Box::new(BufReader::new(File::open(input_file)?))
    } else {
        eprintln!("No input file specified, falling back to stdin");
        Box::new(BufReader::new(stdin()))
    };
    let output: Box<dyn Write> = if let Some(output_file) = config.output {
        Box::new(File::create(output_file)?)
    } else {
        eprintln!("No output file specified, falling back to stdout");

        Box::new(stdout())
    };
    let mut parser = Parser::new(input, output, config.fail);
    parser.process()?;
    Ok(())
}
