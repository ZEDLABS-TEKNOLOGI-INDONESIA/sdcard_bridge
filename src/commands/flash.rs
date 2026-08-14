use crate::protocol::{Cmd, StatusReply};
use crate::serial_link::SerialLink;
use anyhow::{bail, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::io::{BufReader, Read};
use xz2::read::XzDecoder;

const BLOCK_SIZE: usize = 512;

pub fn run(link: &mut SerialLink, image_path: &str, verify: bool) -> Result<()> {
    let file = File::open(image_path)?;
    let reader: Box<dyn Read> = if image_path.ends_with(".xz") {
        Box::new(XzDecoder::new(BufReader::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };
    let mut reader = reader;

    println!("Menulis image ke SD card (raw block write)...");
    println!("PERINGATAN: pastikan target SD card benar. Proses ini akan menimpa seluruh isi kartu.");

    let pb = ProgressBar::new_spinner();
    pb.set_style(ProgressStyle::with_template("{spinner:.green} {msg}")?);

    let mut buf = [0u8; BLOCK_SIZE];
    let mut lba: u32 = 0;
    let mut total_bytes: u64 = 0;

    loop {
        let n = read_full_block(&mut reader, &mut buf)?;
        if n == 0 {
            break;
        }
        // pad last block with zeros if shorter than BLOCK_SIZE
        if n < BLOCK_SIZE {
            for b in &mut buf[n..] {
                *b = 0;
            }
        }

        let mut payload = lba.to_ne_bytes().to_vec();
        payload.extend_from_slice(&buf);

        let reply = link.transact(Cmd::RawBlockWrite, payload)?;
        let status = StatusReply::parse(&reply.payload)?;
        if !status.ok {
            bail!("gagal menulis block LBA {lba}: {}", status.message);
        }

        if verify {
            let verify_reply = link.transact(Cmd::RawBlockRead, lba.to_ne_bytes().to_vec())?;
            if verify_reply.payload != buf {
                bail!("verifikasi gagal pada block LBA {lba} (data tidak cocok)");
            }
        }

        total_bytes += n as u64;
        lba += 1;
        pb.set_message(format!("{:.2} MB ditulis", total_bytes as f64 / 1_048_576.0));
        pb.tick();
    }

    let finalize = link.transact(Cmd::FlashFinalize, vec![])?;
    let status = StatusReply::parse(&finalize.payload)?;
    if !status.ok {
        bail!("finalize gagal: {}", status.message);
    }

    pb.finish_with_message(format!("Selesai — {:.2} MB ditulis", total_bytes as f64 / 1_048_576.0));
    Ok(())
}

/// Read up to buf.len() bytes, looping until buf is full or EOF is reached.
fn read_full_block(reader: &mut dyn Read, buf: &mut [u8]) -> Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = reader.read(&mut buf[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}
