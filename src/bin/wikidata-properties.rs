use std::{
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
};

use clap::Parser;
use regex::Regex;
use sparql_data_preparation::{lines, progress_bar};

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long)]
    file: PathBuf,

    #[clap(short, long)]
    output: PathBuf,
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let args = Args::parse();

    let num_lines = lines(&args.file)?.count();
    let mut lines = lines(&args.file)?;

    let header = lines.next().expect("file should have at least 1 line")?;
    assert_eq!(header.split_terminator('\t').collect::<Vec<_>>().len(), 4);

    let prop_pattern = Regex::new(r"http://www.wikidata.org/prop/direct(-normalized)?/(P\d+)")?;
    let label_pattern = Regex::new("^\"(.*)\"@en$")?;

    let mut props = Vec::new();

    let pbar = progress_bar("processing wikidata properties", num_lines as u64, false);
    for line in lines {
        pbar.inc(1);
        let line = line?;
        let splits: Vec<_> = line.split_terminator('\t').collect();
        assert_eq!(splits.len(), 4);

        let Some(prop) = prop_pattern.captures(splits[0]) else {
            continue;
        };

        let Some(label) = label_pattern.captures(splits[1]) else {
            continue;
        };

        let prop = prop.get(2).unwrap().as_str().to_string();
        let label = label.get(1).unwrap().as_str().to_string();
        let count = splits[2].parse::<usize>()?;

        // log::info!("prop: {}, label: {}", prop, label);
        props.push((prop, label, count));
    }
    pbar.finish_and_clear();
    log::info!("found {} properties", props.len());

    props.sort_by(|(_, _, a), (_, _, b)| a.cmp(b).reverse());

    let mut output = BufWriter::new(fs::File::create(args.output)?);
    for (prop, label, _) in props {
        writeln!(output, "{}\t{}", prop, label)?;
    }

    Ok(())
}
