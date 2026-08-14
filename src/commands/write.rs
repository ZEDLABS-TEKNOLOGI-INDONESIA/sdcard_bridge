use crate::protocol::{Cmd, StatusReply};
use crate::serial_link::SerialLink;
use anyhow::{bail, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::io::Read;

const CHUNK_SIZE: usize = 512;

pub fn run(link: &mut SerialLink, local_path: &str, remote_path: &str) -> Result<()> {
    let mut file = File::open(local_path)?;
    let file_size = file.metadata()?.len() as u32;

    let mut init_payload = file_size.to_ne_bytes().to_vec();
    init_payload.extend_from_slice(remote_path.as_bytes());

    let reply = link.transact(Cmd::WriteFileInit, init_payload)?;
    let status = StatusReply::parse(&reply.payload)?;
    if !status.ok {
        bail!("gagal membuka file di device: {}", status.message);
    }

    let pb = ProgressBar::new(file_size as u64);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
        )?
        .progress_chars("#>-"),
    );

    let mut buf = [0u8; CHUNK_SIZE];
    let mut index: u32 = 0;

    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }

        let mut payload = index.to_ne_bytes().to_vec();
        payload.extend_from_slice(&buf[..n]);

        let reply = link.transact(Cmd::WriteChunk, payload)?;
        let status = StatusReply::parse(&reply.payload)?;
        if !status.ok {
            bail!("gagal menulis chunk {index}: {}", status.message);
        }

        pb.inc(n as u64);
        index += 1;
    }

    pb.finish_with_message("selesai");
    println!("Tersimpan ke SD card sebagai {remote_path}");
    Ok(())
}
