#![feature(iter_advance_by)]
use std::{env, error::Error, fs::File, io::BufReader};

pub mod nds;

fn main() -> Result<(), Box<dyn Error>> {
    let file = env::args()
        .skip(1)
        .next()
        .ok_or(anyhow::anyhow!("No file provided"))?;

    println!("File: {}", file);

    nds::extract(BufReader::new(File::open(file)?))?;

    Ok(())
}
