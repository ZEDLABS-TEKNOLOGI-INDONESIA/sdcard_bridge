use crate::protocol::Cmd;
use crate::serial_link::SerialLink;
use anyhow::{bail, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::io::Write;

const CHUNK_SIZE: u32 = 512;

pub fn run(link: &mut SerialLink, remote_path: &str, local_path: &str) -> Result<()> {
    let init = link.transact(Cmd::ReadFileInit, remote_path.as_bytes().to_vec())?;
    if init.payload.len() < 8 {
        bail!("unexpected READ_FILE_INIT reply");
    }
    let file_size = u32::from_ne_bytes(init.payload[0..4].try_into().unwrap());
    let chunk_count = u32::from_ne_bytes(init.payload[4..8].try_into().unwrap());

    let pb = ProgressBar::new(file_size as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
        )?
        .progress_chars("#>-"),
    );

    let mut out = File::create(local_path)?;
    let mut remaining = file_size;

    for index in 0..chunk_count {
        let frame = link.transact(Cmd::ReadChunk, index.to_ne_bytes().to_vec())?;
        let to_write = remaining.min(CHUNK_SIZE) as usize;
        out.write_all(&frame.payload[..to_write])?;
        remaining -= to_write as u32;
        pb.inc(to_write as u64);
    }

    pb.finish_with_message("selesai");
    println!("Tersimpan ke {local_path}");
    Ok(())
}
