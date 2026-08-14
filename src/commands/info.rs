use crate::protocol::Cmd;
use crate::serial_link::SerialLink;
use anyhow::Result;

pub fn run(link: &mut SerialLink) -> Result<()> {
    let frame = link.transact(Cmd::SdInfo, vec![])?;
    let p = &frame.payload;

    let card_type = p[0];
    let size_mb = u32::from_ne_bytes(p[1..5].try_into().unwrap());
    let sector_size = u32::from_ne_bytes(p[5..9].try_into().unwrap());

    let type_name = match card_type {
        0 => "SD",
        1 => "SDHC/SDXC",
        _ => "Unknown",
    };

    println!("Card type   : {type_name}");
    println!("Capacity    : {size_mb} MB (~{:.2} GB)", size_mb as f64 / 1024.0);
    println!("Sector size : {sector_size} bytes");

    Ok(())
}
