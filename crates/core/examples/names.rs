//! Prints the name of each strategy read from stdin, one per line:
//!   cargo run -p sluis-core --example names < tests/data/strategies-blockcheck.txt

use std::io::BufRead;

use sluis_core::{Strategy, describe};

fn main() {
    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
        match Strategy::parse(&line) {
            Ok(strategy) => println!("{}\n    {line}", describe(&strategy)),
            Err(e) => println!("(rejected: {e})\n    {line}"),
        }
    }
}
