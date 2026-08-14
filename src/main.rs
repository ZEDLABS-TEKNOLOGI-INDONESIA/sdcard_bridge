mod commands;
mod protocol;
mod serial_link;

use clap::{Parser, Subcommand};
use serial_link::SerialLink;
use anyhow::Result;

const DEFAULT_BAUD: u32 = 921600;

#[derive(Parser)]
#[command(name = "sdcard_bridge", version, about = "SD card bridge via ESP32 (read/write/flash)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Serial port ESP32, mis. /dev/ttyUSB0
    #[arg(long, global = true)]
    port: Option<String>,

    /// Baud rate serial
    #[arg(long, global = true, default_value_t = DEFAULT_BAUD)]
    baud: u32,
}

#[derive(Subcommand)]
enum Commands {
    /// Tampilkan info kapasitas & tipe SD card
    Info,
    /// List isi direktori di SD card
    List {
        path: String,
    },
    /// Baca file dari SD card ke lokal
    Read {
        remote_path: String,
        local_path: String,
    },
    /// Tulis file lokal ke SD card
    Write {
        local_path: String,
        remote_path: String,
    },
    /// Flash image OS (.img / .img.xz) ke SD card secara raw
    Flash {
        image_path: String,
        /// Verifikasi tiap block setelah ditulis (lebih lambat, lebih aman)
        #[arg(long)]
        verify: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let port_name = cli
        .port
        .unwrap_or_else(|| "/dev/ttyUSB0".to_string());

    let mut link = SerialLink::open(&port_name, cli.baud)?;

    match cli.command {
        Commands::Info => commands::info::run(&mut link)?,
        Commands::List { path } => commands::list::run(&mut link, &path)?,
        Commands::Read { remote_path, local_path } => {
            commands::read::run(&mut link, &remote_path, &local_path)?
        }
        Commands::Write { local_path, remote_path } => {
            commands::write::run(&mut link, &local_path, &remote_path)?
        }
        Commands::Flash { image_path, verify } => {
            commands::flash::run(&mut link, &image_path, verify)?
        }
    }

    Ok(())
}
