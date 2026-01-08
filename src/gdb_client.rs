
use std::io;
use std::io::Read;
use std::io::Write;
use std::io::{Error, ErrorKind};
use std::net::TcpStream;

use quick_xml::Reader;
use quick_xml::events::Event;

pub trait GdbTransport {
    fn connect(&mut self) -> io::Result<()>;
    fn close(&mut self) -> io::Result<()>;
    fn send(&mut self, data: &[u8]) -> io::Result<()>;
    fn recv_exact(&mut self, buf: &mut [u8]) -> io::Result<()>;
}

pub struct TcpTransport {
    host: String,
    port: u16,
    stream: Option<TcpStream>,
}

impl TcpTransport {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            stream: None,
        }
    }

    fn stream(&mut self) -> io::Result<&mut TcpStream> {
        self.stream
            .as_mut()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotConnected, "not connected"))
    }
}

impl GdbTransport for TcpTransport {
    fn connect(&mut self) -> io::Result<()> {
        let stream = TcpStream::connect((&*self.host, self.port))?;
        stream.set_nodelay(true)?;
        self.stream = Some(stream);
        Ok(())
    }

    fn close(&mut self) -> io::Result<()> {
        self.stream.take(); // drop
        Ok(())
    }

    fn send(&mut self, data: &[u8]) -> io::Result<()> {
        self.stream()?.write_all(data)
    }

    fn recv_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        self.stream()?.read_exact(buf)
    }
}

pub struct GdbClient<T: GdbTransport> {
    transport: T,
    connected: bool,
}
#[allow(dead_code)]
impl<T: GdbTransport> GdbClient<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            connected: false,
        }
    }

    fn checksum(data: &[u8]) -> u8 {
        data.iter().fold(0u8, |s, b| s.wrapping_add(*b))
    }

    pub fn connect(&mut self) -> io::Result<()> {
        if self.connected {
            return Ok(());
        }

        self.transport.connect()?;
        self.connected = true;
        Ok(())
    }

    pub fn disconnect(&mut self) -> io::Result<()> {
        if !self.connected {
            return Ok(());
        }

        self.transport.close()?;
        self.connected = false;
        Ok(())
    }

    fn escape_binary(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len());
        for &b in data {
            match b {
                b'#' | b'$' | b'*' | b'}' => {
                    out.push(b'}');
                    out.push(b ^ 0x20);
                }
                _ => out.push(b),
            }
        }
        out
    }

    fn recv_byte(&mut self) -> io::Result<u8> {
        let mut b = [0u8];
        self.transport.recv_exact(&mut b)?;
        Ok(b[0])
    }
}

impl<T: GdbTransport> GdbClient<T> {
    fn read_packet(&mut self) -> io::Result<Vec<u8>> {
        // 等待 '$'
        loop {
            if self.recv_byte()? == b'$' {
                break;
            }
        }

        let mut payload = Vec::new();

        loop {
            let b = self.recv_byte()?;
            if b == b'#' {
                break;
            }
            payload.push(b);
        }

        // 丢弃 checksum
        let mut checksum = [0u8; 2];
        self.transport.recv_exact(&mut checksum)?;

        // ACK
        self.transport.send(b"+")?;

        Ok(payload)
    }
}

impl<T: GdbTransport> GdbClient<T> {
    pub fn send_cmd(&mut self, prefix: &str, binary: &[u8]) -> io::Result<Vec<u8>> {
        let mut body = Vec::new();
        body.extend_from_slice(prefix.as_bytes());
        body.extend_from_slice(binary);

        let csum = Self::checksum(&body);

        let mut pkt = Vec::new();
        pkt.push(b'$');
        pkt.extend_from_slice(&body);
        pkt.push(b'#');
        pkt.extend_from_slice(format!("{:02x}", csum).as_bytes());

        self.transport.send(&pkt)?;

        // 等 ACK
        match self.recv_byte()? {
            b'+' => {}
            b'-' => return Err(io::Error::new(io::ErrorKind::Other, "NACK")),
            b => {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("unexpected ACK: {}", b),
                ));
            }
        }

        self.read_packet()
    }
}

impl<T: GdbTransport> GdbClient<T> {
    pub fn flash_erase(&mut self, addr: u32, len: u32) -> io::Result<()> {
        let resp = self.send_cmd(&format!("vFlashErase:{:x},{:x}", addr, len), &[])?;

        if resp != b"OK" {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("erase failed: {:?}", resp),
            ));
        }
        Ok(())
    }
}

const FLASH_WORD: usize = 4;

impl<T: GdbTransport> GdbClient<T> {
    pub fn flash_write(&mut self, addr: u32, data: &[u8], chunk: usize) -> io::Result<()> {
        let mut offset = 0usize;

        while offset < data.len() {
            let mut block = data[offset..usize::min(offset + chunk, data.len())].to_vec();

            if block.len() % FLASH_WORD != 0 {
                let pad = FLASH_WORD - (block.len() % FLASH_WORD);
                block.extend(std::iter::repeat(0xFF).take(pad));
            }

            let escaped = Self::escape_binary(&block);

            let resp = self.send_cmd(
                &format!("vFlashWrite:{:x}:", addr + offset as u32),
                &escaped,
            )?;

            if resp != b"OK" {
                return Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("write failed @0x{:x}", addr + offset as u32),
                ));
            }

            offset += block.len();
        }

        Ok(())
    }
}

impl<T: GdbTransport> GdbClient<T> {
    pub fn flash_done(&mut self) -> io::Result<()> {
        let resp = self.send_cmd("vFlashDone", &[])?;
        if resp != b"OK" {
            return Err(io::Error::new(io::ErrorKind::Other, "FlashDone failed"));
        }
        Ok(())
    }
}
#[allow(dead_code)]
impl<T: GdbTransport> GdbClient<T> {
    pub fn read_memory(&mut self, addr: u32, len: u32) -> io::Result<String> {
        let resp = self.send_cmd(&format!("m{:x},{:x}", addr, len), &[])?;
        Ok(String::from_utf8_lossy(&resp).into_owned())
    }
}

/// 一个简单的 MockTransport，用于测试 GdbClient
#[derive(Debug)]
#[allow(dead_code)]
pub struct MockTransport {
    pub sent_packets: Vec<Vec<u8>>,
    recv_buffer: Vec<u8>,
    recv_pos: usize,
    connected: bool,
}
#[allow(dead_code)]
impl MockTransport {
    pub fn new(responses: Vec<Vec<u8>>, connected: bool) -> Self {
        let mut recv_buffer = Vec::new();
        for r in responses {
            recv_buffer.extend_from_slice(&r);
        }

        Self {
            sent_packets: Vec::new(),
            recv_buffer,
            recv_pos: 0,
            connected: connected,
        }
    }

    pub fn rsp_packet(payload: &[u8]) -> Vec<u8> {
        let checksum: u8 = payload.iter().fold(0u8, |s, b| s.wrapping_add(*b));
        let mut v = Vec::new();
        v.push(b'$');
        v.extend_from_slice(payload);
        v.push(b'#');
        v.extend_from_slice(format!("{:02x}", checksum).as_bytes());
        v
    }
}

impl GdbTransport for MockTransport {
    fn connect(&mut self) -> io::Result<()> {
        self.connected = true;
        Ok(())
    }

    fn close(&mut self) -> io::Result<()> {
        self.connected = false;
        Ok(())
    }

    fn send(&mut self, data: &[u8]) -> io::Result<()> {
        if !self.connected {
            return Err(Error::new(ErrorKind::NotConnected, "mock not connected"));
        }
        self.sent_packets.push(data.to_vec());
        Ok(())
    }

    fn recv_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        if !self.connected {
            return Err(Error::new(ErrorKind::NotConnected, "mock not connected"));
        }

        if self.recv_pos + buf.len() > self.recv_buffer.len() {
            return Err(Error::new(ErrorKind::UnexpectedEof, "mock: no more data"));
        }

        buf.copy_from_slice(&self.recv_buffer[self.recv_pos..self.recv_pos + buf.len()]);
        self.recv_pos += buf.len();
        Ok(())
    }
}

fn parse_hex_u64(s: &str) -> Result<u64, std::num::ParseIntError> {
    let s = s.trim();

    let s = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);

    u64::from_str_radix(s, 16)
}

fn parse_flash_regions_from_xml(xml: &[u8]) -> io::Result<Vec<FlashRegion>> {
    let mut reader = Reader::from_reader(xml);
    reader.trim_text(true);

    let mut regions = Vec::new();
    let mut buf = Vec::new();

    let mut cur: Option<FlashRegion> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) if e.name().as_ref() == b"memory" => {
                let mut start = 0;
                let mut length = 0;
                let mut is_flash = false;

                for a in e.attributes().flatten() {
                    match a.key.as_ref() {
                        b"type" => {
                            is_flash = a.value.as_ref() == b"flash";
                        }
                        b"start" => {
                            start =
                                parse_hex_u64(str::from_utf8(a.value.as_ref()).unwrap()).unwrap();
                        }
                        b"length" => {
                            length =
                                parse_hex_u64(str::from_utf8(a.value.as_ref()).unwrap()).unwrap();
                        }
                        _ => {}
                    }
                }

                if is_flash {
                    cur = Some(FlashRegion {
                        start,
                        length,
                        blocksize: None,
                    });
                }
            }

            Ok(Event::Text(e)) => {
                if let Some(r) = cur.as_mut() {
                    let txt = e.unescape().unwrap();
                    if let Ok(bs) = parse_hex_u64(&txt) {
                        r.blocksize = Some(bs);
                    }
                }
            }

            Ok(Event::End(e)) if e.name().as_ref() == b"memory" => {
                if let Some(r) = cur.take() {
                    regions.push(r);
                }
            }

            Ok(Event::Eof) => break,

            Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e)),

            _ => {}
        }

        buf.clear();
    }

    Ok(regions)
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct FlashRegion {
    pub start: u64,
    pub length: u64,
    pub blocksize: Option<u64>,
}

impl<T: GdbTransport> GdbClient<T> {
    pub fn get_flash_info(&mut self) -> io::Result<Vec<FlashRegion>> {
        let resp = self.send_cmd("qXfer:memory-map:read::0,fff", &[])?;

        if resp.is_empty() || resp[0] == b'E' {
            return Err(io::Error::new(io::ErrorKind::Other, "qXfer failed"));
        }

        let xml: &[u8] = &resp[1..]; // 去掉 m / l

        parse_flash_regions_from_xml(xml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checksum(data: &[u8]) -> u8 {
        data.iter().fold(0u8, |s, b| s.wrapping_add(*b))
    }

    #[test]
    fn test_send_cmd_ok() {
        let responses = vec![
            vec![b'+'],                       // ACK
            MockTransport::rsp_packet(b"OK"), // 响应包
        ];

        let transport = MockTransport::new(responses, true);
        let mut client = GdbClient::new(transport);

        let resp = client.send_cmd("qSupported", &[]).unwrap();
        assert_eq!(resp, b"OK");

        // 校验发送的数据
        let sent = &client.transport.sent_packets[0];

        assert_eq!(sent[0], b'$');
        assert!(sent.len() >= 4);
        assert_eq!(sent[sent.len() - 3], b'#');
        let body = &sent[1..sent.len() - 3]; // 去掉 $ 和 #cc
        let csum = &sent[sent.len() - 2..];

        assert_eq!(
            format!("{:02x}", checksum(body)),
            String::from_utf8_lossy(csum)
        );
    }
    #[test]
    fn test_flash_erase_ok() {
        let responses = vec![vec![b'+'], MockTransport::rsp_packet(b"OK")];

        let transport = MockTransport::new(responses, true);
        let mut client = GdbClient::new(transport);

        client.flash_erase(0x0800_0000, 0x1000).unwrap();

        let sent = &client.transport.sent_packets[0];
        let sent_str = String::from_utf8_lossy(sent);

        assert!(sent_str.contains("vFlashErase:8000000,1000"));
    }

    #[test]
    fn test_flash_write_escape() {
        let responses = vec![vec![b'+'], MockTransport::rsp_packet(b"OK")];

        let transport = MockTransport::new(responses, true);
        let mut client = GdbClient::new(transport);

        // 含需要 escape 的字符
        let data = [b'$', b'#', b'*', b'}'];

        client.flash_write(0x0800_0000, &data, 16).unwrap();

        let sent = &client.transport.sent_packets[0];
        let body = &sent[1..sent.len() - 3];

        // escape 后应为： }^ }# }* }}
        assert!(body.windows(2).any(|w| w == [b'}', b'$' ^ 0x20]));
        assert!(body.windows(2).any(|w| w == [b'}', b'#' ^ 0x20]));
        assert!(body.windows(2).any(|w| w == [b'}', b'*' ^ 0x20]));
        assert!(body.windows(2).any(|w| w == [b'}', b'}' ^ 0x20]));
    }

    #[test]
    fn test_flash_done() {
        let responses = vec![vec![b'+'], MockTransport::rsp_packet(b"OK")];

        let transport = MockTransport::new(responses, true);
        let mut client = GdbClient::new(transport);

        client.flash_done().unwrap();

        let sent = &client.transport.sent_packets[0];
        let sent_str = String::from_utf8_lossy(sent);

        assert!(sent_str.starts_with("$vFlashDone#"));
    }

    #[test]
    fn test_read_memory() {
        let responses = vec![vec![b'+'], MockTransport::rsp_packet(b"00112233aabbccdd")];

        let transport = MockTransport::new(responses, true);
        let mut client = GdbClient::new(transport);

        let resp = client.read_memory(0x2000_0000, 8).unwrap();
        assert_eq!(resp, "00112233aabbccdd");

        let sent = &client.transport.sent_packets[0];
        let sent_str = String::from_utf8_lossy(sent);

        assert!(sent_str.contains("m20000000,8"));
    }

    #[test]
    fn test_nack_error() {
        let responses = vec![
            vec![b'-'], // NACK
        ];

        let transport = MockTransport::new(responses, true);
        let mut client = GdbClient::new(transport);

        let err = client.send_cmd("qSupported", &[]).unwrap_err();
        assert!(err.to_string().contains("NACK"));
    }

    #[test]
    fn test_client_connect_disconnect() {
        let transport = MockTransport::new(Vec::new(), false);
        let mut client = GdbClient::new(transport);

        assert!(!client.connected);

        client.connect().unwrap();
        assert!(client.connected);

        client.disconnect().unwrap();
        assert!(!client.connected);
    }
    #[test]
    fn test_parse_flash_regions_from_xml() {
        let xml = br#"
<memory-map>
  <memory type="ram" start="0x00000000" length="0x08000000"/>
  <memory type="flash" start="0x08000000" length="0x8000">
    <property name="blocksize">0x400</property>
  </memory>
  <memory type="ram" start="0x08008000" length="0xf7ff8000"/>
</memory-map>
"#;

        let regions = parse_flash_regions_from_xml(xml).unwrap();

        assert_eq!(regions.len(), 1);

        let r = &regions[0];
        assert_eq!(r.start, 0x0800_0000);
        assert_eq!(r.length, 0x8000);
        assert_eq!(r.blocksize, Some(0x400));
    }
}
