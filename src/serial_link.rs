use crate::protocol::{Cmd, Frame, HEADER_LEN};
use anyhow::{bail, Context, Result};
use serialport::SerialPort;
use std::io::{Read, Write};
use std::time::Duration;

pub struct SerialLink {
    port: Box<dyn SerialPort>,
}

impl SerialLink {
    pub fn open(port_name: &str, baud: u32) -> Result<Self> {
        let port = serialport::new(port_name, baud)
            .timeout(Duration::from_secs(5))
            .open()
            .with_context(|| format!("failed to open serial port {port_name}"))?;
        Ok(Self { port })
    }

    /// Send a frame and block until a response frame is received.
    pub fn transact(&mut self, cmd: Cmd, payload: Vec<u8>) -> Result<Frame> {
        self.send(cmd, payload)?;
        self.recv()
    }

    pub fn send(&mut self, cmd: Cmd, payload: Vec<u8>) -> Result<()> {
        let frame = Frame::new(cmd, payload);
        self.port
            .write_all(&frame.encode())
            .context("failed writing frame to serial port")?;
        Ok(())
    }

    pub fn recv(&mut self) -> Result<Frame> {
        let mut header = [0u8; HEADER_LEN];
        self.read_exact_retry(&mut header)?;

        let cmd_byte = header[0];
        let len_bytes: [u8; 4] = header[1..5].try_into().unwrap();
        let len = u32::from_be_bytes(len_bytes) as usize;

        if len > 8 * 1024 * 1024 {
            bail!("frame length too large: {len} bytes (possible corrupt stream)");
        }

        let mut payload = vec![0u8; len];
        if len > 0 {
            self.read_exact_retry(&mut payload)?;
        }

        let mut crc_bytes = [0u8; 4];
        self.read_exact_retry(&mut crc_bytes)?;

        if !Frame::verify_crc(cmd_byte, &len_bytes, &payload, &crc_bytes) {
            bail!("CRC mismatch on received frame (cmd=0x{:02X})", cmd_byte);
        }

        let cmd = Cmd::from_u8(cmd_byte)?;
        Ok(Frame::new(cmd, payload))
    }

    fn read_exact_retry(&mut self, buf: &mut [u8]) -> Result<()> {
        self.port
            .read_exact(buf)
            .context("timed out or failed reading from serial port")
    }
}
