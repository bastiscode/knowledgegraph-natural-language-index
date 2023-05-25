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
    assert_eq!(header.split_terminator('\t').collect::<Vec<_>>().len(), 5);

    let ent_pattern = Regex::new(r"http://www.wikidata.org/entity/(Q\d+)")?;
    let label_pattern = Regex::new("^\"(.*)\"@en$")?;

    let mut props = Vec::new();

    let pbar = progress_bar("processing wikidata entities", num_lines as u64, false);
    for line in lines {
        pbar.inc(1);
        let line = line?;
        let splits: Vec<_> = line.split_terminator('\t').collect();
        assert_eq!(splits.len(), 5);

        let Some(ent) = ent_pattern.captures(splits[0]) else {
            continue;
        };

        let Some(ent_label) = label_pattern.captures(splits[1]) else {
            continue;
        };

        let Some(inst) = ent_pattern.captures(splits[2]) else {
            continue;
        };

        let Some(inst_label) = label_pattern.captures(splits[3]) else {
            continue;
        };

        let ent = ent.get(1).unwrap().as_str().to_string();
        let ent_label = ent_label.get(1).unwrap().as_str().to_string();
        let inst = inst.get(1).unwrap().as_str().to_string();
        let inst_label = inst_label.get(1).unwrap().as_str().to_string();
        let count = splits[4].parse::<usize>()?;

        log::info!(
            "entity: {}, label: {} (instance: {}, label: {})",
            ent,
            ent_label,
            inst,
            inst_label
        );
        props.push((ent, ent_label, inst, inst_label, count));
    }
    pbar.finish_and_clear();
    log::info!("found {:,} entities", props.len());

    props.sort_by(|(_, _, _, _, a), (_, _, _, _, b)| a.cmp(b).reverse());

    let mut output = BufWriter::new(fs::File::create(args.output)?);
    for (ent, ent_label, inst, inst_label, _) in props {
        writeln!(output, "{}\t{}\t{}\t{}", ent, ent_label, inst, inst_label)?;
    }

    Ok(())
}
