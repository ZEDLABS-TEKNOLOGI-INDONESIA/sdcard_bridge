use crate::protocol::Cmd;
use crate::serial_link::SerialLink;
use anyhow::Result;

pub fn run(link: &mut SerialLink, path: &str) -> Result<()> {
    let frame = link.transact(Cmd::ListDir, path.as_bytes().to_vec())?;
    let listing = String::from_utf8_lossy(&frame.payload);

    if listing.trim().is_empty() {
        println!("(kosong)");
    } else {
        print!("{listing}");
    }
    Ok(())
}
