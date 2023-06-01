use std::io::BufRead;
use std::path::Path;
use std::{fs, io::BufReader};

use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

pub fn lines(
    file: impl AsRef<Path>,
) -> anyhow::Result<impl Iterator<Item = anyhow::Result<String>>> {
    let file = fs::File::open(file)?;
    let file = BufReader::new(file);
    Ok(file.lines().map(|line| line.map_err(anyhow::Error::from)))
}

pub fn progress_bar(msg: &str, size: u64, hidden: bool) -> ProgressBar {
    let pb = ProgressBar::new(size)
        .with_style(
            ProgressStyle::with_template(
                "{msg}: {wide_bar} [{pos}/{len}] [{elapsed_precise}|{eta_precise}]",
            )
            .unwrap(),
        )
        .with_message(msg.to_string());
    if hidden {
        pb.set_draw_target(ProgressDrawTarget::hidden());
    }
    pb
}
