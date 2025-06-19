use crate::brief_display::BriefDisplay;
use anyhow::Context;
use cbor_data::Cbor;
use clap::Parser;
use std::{
    fs::File,
    io::{stdin, stdout},
};

mod brief_display;

#[derive(Parser, Debug)]
struct Args {
    /// Input file to process; use "-" for stdin
    #[clap(short, long, default_value = "-")]
    input: String,

    /// Output file to write; use "-" for stdout
    #[clap(short, long, default_value = "-")]
    output: String,

    /// Output each CBOR item as a diagnostic string
    #[clap(short, long)]
    string: bool,

    /// Censored property names
    ///
    /// Any properties with this name will be silently removed from the output.
    #[clap(short, long)]
    censored_properties: Vec<String>,

    /// Maximum depth of the output
    ///
    /// The output will be truncated at this depth, meaning that an array or map
    /// at this depth will be cleared.
    #[clap(short = 'd', long, default_value = "10")]
    max_depth: usize,

    /// Maximum length of an array to be displayed
    ///
    /// Further elements will be cleared from the array.
    #[clap(short, long, default_value = "10")]
    array_length: usize,

    /// Maximum length of a text string to be displayed
    ///
    /// Further characters will be cleared from the string.
    #[clap(short, long, default_value = "10")]
    text_length: usize,

    /// Do not print any diagnostic output to stderr
    #[clap(short, long)]
    quiet: bool,
}

fn main() {
    let args = Args::parse();

    let mut input = if args.input == "-" {
        Box::new(stdin()) as Box<dyn std::io::Read>
    } else {
        Box::new(
            File::open(&args.input)
                .context(format!("opening input file `{}`", args.input))
                .unwrap(),
        )
    };
    let mut output = if args.output == "-" {
        Box::new(stdout()) as Box<dyn std::io::Write>
    } else {
        Box::new(
            File::create(&args.output)
                .context(format!("opening output file `{}`", args.output))
                .unwrap(),
        )
    };

    let mut read_buf = Vec::new();
    read_buf.resize(1048576, 0u8);
    let read_buf = read_buf.as_mut_slice();

    let mut data_buf = Vec::new();

    let mut count = 0;
    while let Ok(n) = input.read(read_buf) {
        if n == 0 {
            break;
        }
        data_buf.extend_from_slice(&read_buf[..n]);
        let mut rest = &data_buf[..];
        while let Ok((cbor, r)) = Cbor::checked_prefix(rest) {
            count += 1;
            if args.string {
                writeln!(
                    &mut output,
                    "{}",
                    BriefDisplay {
                        cbor,
                        max_depth: args.max_depth,
                        array_length: args.array_length,
                        censored_properties: &args.censored_properties,
                        text_length: args.text_length,
                    }
                )
                .unwrap();
            }
            rest = r;
        }
        if rest.len() < data_buf.len() {
            let len = rest.len();
            let start = data_buf.len() - len;
            data_buf.copy_within(start.., 0);
            data_buf.truncate(len);
        }
    }

    if !args.quiet {
        eprintln!("Processed {} items", count);
    }
}
