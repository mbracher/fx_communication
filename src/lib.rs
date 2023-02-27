extern crate core;


use bytes::{BufMut, BytesMut, Buf};
use std::{cmp, io, str, usize};
use futures::{SinkExt, StreamExt};
use tokio::io::{ReadHalf, WriteHalf};
use tokio_serial::SerialStream;

// use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Encoder, Decoder, FramedRead, FramedWrite};
use crate::Command::{ReadWords, WriteWords};


#[derive(Debug, Copy, Clone)]
pub struct Address {
    pub station: u8,
    pub plc: u8,
}
impl Address {
    pub fn new(station: u8, plc: u8) -> Self {
        Address {
            station,
            plc
        }
    }
}

#[derive(Debug)]
pub struct NakWithError {
    address: Address,
    error_code: u8,
}
impl NakWithError {
    pub fn new(address: Address, error_code: u8) -> Self {
        NakWithError {
            address,
            error_code
        }
    }
}

#[derive(Debug)]
pub struct ReadWordsCommand {
    pub head_device: String, //exact 5 long
    pub number_of_device_points: u8,
}
impl ReadWordsCommand {
    pub fn new(head_device: String, number_of_device_points: u8) -> Self {
        ReadWordsCommand {
            head_device,
            number_of_device_points,
        }
    }
}

#[derive(Debug)]
pub struct WriteWordsCommand {
    pub head_device: String, //exact 5 long
    pub number_of_device_points: u8,
    pub data: String,
}
impl WriteWordsCommand {
    pub fn new(head_device: String, number_of_device_points: u8, data: String) -> Self {
        WriteWordsCommand {
            head_device,
            number_of_device_points,
            data,
        }
    }
}

#[derive(Debug)]
pub enum Command {
    ReadWords(ReadWordsCommand),
    WriteWords(WriteWordsCommand),
}

#[derive(Debug)]
pub struct Request {
    pub address: Address,
    pub command: Command,
    pub msg_wait_time: u8,
}

impl Request {
    pub fn new(address: Address, msg_wait_time: u8, command: Command) -> Self {
        Request {
            address,
            msg_wait_time,
            command,
        }
    }
}

#[derive(Debug)]
pub struct Response {
    pub address: Address,
    pub data: String,
}
impl Response {
    pub fn new(address: Address, data: String) -> Self {
        Response {
            address,
            data,
        }
    }
}

#[derive(Debug)]
pub enum Message {
    Request(Request),
    Ack(Address),
    Nak(Address),
    NakWithError(NakWithError),
    Response(Response),
}

const STX: u8 = 2;
const ETX: u8 = 3;
const ACK: u8 = 6;

const ENQ: u8 = 5;
const NAK: u8 = 21; //0x15
// const CR: u8 = 13; //0x0D \r
const LF: u8 = 10; //0x0A \n


pub struct FxCodec {
    next_index: usize,
    max_length: usize,
    is_discarding: bool,
}

impl FxCodec {
    pub fn new() -> FxCodec {
        FxCodec {
            next_index: 0,
            max_length: usize::MAX,
            is_discarding: false,
        }
    }

    fn discard(&mut self, newline_offset: Option<usize>, read_to: usize, buf: &mut BytesMut) {
        let discard_to = if let Some(offset) = newline_offset {
            // If we found a newline, discard up to that offset and
            // then stop discarding. On the next iteration, we'll try
            // to read a line normally.
            self.is_discarding = false;
            offset + self.next_index + 1
        } else {
            // Otherwise, we didn't find a newline, so we'll discard
            // everything we read. On the next iteration, we'll continue
            // discarding up to max_len bytes unless we find a newline.
            read_to
        };
        buf.advance(discard_to);
        self.next_index = 0;
    }
}

fn utf8(buf: &[u8]) -> Result<&str, io::Error> {
    str::from_utf8(buf)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Unable to decode input as UTF8"))
}

fn without_carriage_return(s: &[u8]) -> &[u8] {
    if let Some(&b'\r') = s.last() {
        &s[..s.len() - 1]
    } else {
        s
    }
}

impl Decoder for FxCodec {
    type Item = Message;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {

        loop {
            // Determine how far into the buffer we'll search for a newline. If
            // there's no max_length set, we'll read to the end of the buffer.
            let read_to = cmp::min(self.max_length.saturating_add(1), buf.len());

            let newline_offset = buf[self.next_index..read_to]
                .iter()
                .position(|b| *b == LF);

            if self.is_discarding {
                self.discard(newline_offset, read_to, buf);
            } else {
                return if let Some(offset) = newline_offset {
                    // Found a line!
                    let newline_index = offset + self.next_index;
                    self.next_index = 0;
                    let line = buf.split_to(newline_index + 1);
                    let first:u8 = line[0];
                    let line = &line[1..line.len() - 1];
                    let line = without_carriage_return(line);
                    let checksum = if first == STX || first == ENQ {
                        checksum(&line[..line.len()-2])
                    }
                    else {
                        0
                    };
                    let line = utf8(&line)?;
                    // println!("line: {}", line);
                    match first {
                        STX => {
                            let station = u8::from_str_radix(&line[0..2], 16).unwrap(); //TODO: check if bytes are there and conversion succeeded
                            let plc = u8::from_str_radix(&line[2..4], 16).unwrap();
                            let data = line[4..line.len()-3].to_string();
                            let etx = line[line.len()-3..line.len()-2].as_bytes()[0];
                            if etx != ETX {
                                return Err(io::Error::new(io::ErrorKind::Other, "ETX not found at expected position in Message"));
                            }
                            let checksum_in_message = u8::from_str_radix(&line[line.len()-2..], 16).unwrap();
                            if checksum_in_message != checksum {
                                return Err(io::Error::new(io::ErrorKind::Other, "Invalid checksum in Message"));
                            }
                            Ok(Some(Message::Response(
                                Response {
                                    address: Address {
                                        station,
                                        plc,
                                    },
                                    data
                                }
                            )))
                        },

                        ACK => {
                            //TDOO: check lengts and create acc with or without error code
                            if line.len() == 4 {
                                let station = u8::from_str_radix(&line[0..2], 16).unwrap(); //TODO: check if bytes are there and conversion succeeded
                                let plc = u8::from_str_radix(&line[2..4], 16).unwrap();
                                Ok(Some(Message::Ack(
                                    Address {
                                        station,
                                        plc
                                    }
                                )))
                            }
                            else {
                                Err(io::Error::new(io::ErrorKind::Other, "Invalid ACK Message"))
                            }
                        },
                        NAK => {
                            match line.len() {
                                4 => {
                                    let station = u8::from_str_radix(&line[0..2], 16).unwrap();
                                    let plc = u8::from_str_radix(&line[2..4], 16).unwrap();
                                    Ok(Some(Message::Nak(
                                        Address {
                                            station,
                                            plc
                                        }
                                    )))
                                },
                                6 => {
                                    let station = u8::from_str_radix(&line[0..2], 16).unwrap();
                                    let plc = u8::from_str_radix(&line[2..4], 16).unwrap();
                                    let error_code = u8::from_str_radix(&line[4..6], 16).unwrap();
                                    Ok(Some(Message::NakWithError(
                                        NakWithError {
                                            address: Address {
                                                station,
                                                plc
                                            },
                                            error_code
                                        }
                                    )))
                                },
                                _ => {
                                    Err(io::Error::new(io::ErrorKind::Other, "Invalid NAK Message"))
                                }
                            }

                        },

                        ENQ => {
                            if line.len() < 7 {
                                Err(io::Error::new(io::ErrorKind::Other, "Not complete header in ENQ Message"))
                            }
                            else {
                                let station = u8::from_str_radix(&line[0..2], 16).unwrap();
                                let plc = u8::from_str_radix(&line[2..4], 16).unwrap();
                                let command_code = &line[4..6];
                                let command = match command_code {
                                    "WW" => {
                                        let head_device = line[7..12].to_string();
                                        let number_of_device_points = u8::from_str_radix(&line[12..14], 16).unwrap();
                                        let data = line[14..line.len()-2].to_string();
                                        if data.len() != number_of_device_points as usize * 4 {
                                            return Err(io::Error::new(io::ErrorKind::Other, format!("Command {} data length not correct.", command_code)))
                                        }
                                        WriteWords(
                                            WriteWordsCommand {
                                                head_device,
                                                number_of_device_points,
                                                data,
                                            }
                                        )
                                    },
                                    "WR" => {
                                        let head_device = line[7..12].to_string();
                                        let number_of_device_points = u8::from_str_radix(&line[12..14], 16).unwrap();
                                        ReadWords(
                                            ReadWordsCommand {
                                                head_device,
                                                number_of_device_points,
                                            }
                                        )
                                    },
                                    _ => {
                                        return Err(io::Error::new(io::ErrorKind::Other, format!("Command {} not implemented", command_code)))
                                    }
                                };

                                let msg_wait_time = u8::from_str_radix(&line[6..7], 16).unwrap();

                                let checksum_in_message = u8::from_str_radix(&line[line.len()-2..], 16).unwrap();
                                if checksum != checksum_in_message {
                                    return Err(io::Error::new(io::ErrorKind::Other, "Invalid checksum in Message"))
                                }
                                Ok(Some(Message::Request(
                                    Request {
                                        address: Address {
                                            station,
                                            plc
                                        },
                                        command,
                                        msg_wait_time,
                                    }
                                )))
                            }
                        }

                        _ => Err(io::Error::new(io::ErrorKind::Other, "Invalid Message")), //TODO: discard until find a start character?
                    }
                } else if buf.len() > self.max_length {
                    // Reached the maximum length without finding a
                    // newline, return an error and start discarding on the
                    // next call.
                    self.is_discarding = true;
                    Err(io::Error::new(
                        io::ErrorKind::Other,
                        "line length limit exceeded",
                    ))
                } else {
                    // We didn't find a line or reach the length limit, so the next
                    // call will resume searching at the current offset.
                    self.next_index = read_to;
                    Ok(None)
                };
            }
        }
    }


}

impl Encoder<Message> for FxCodec {
    // type Item = Message;
    type Error = io::Error;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {

        match item {
            Message::Response(p) =>  {
                dst.reserve(9 + p.data.len());
                dst.put_u8(STX); //STX
                dst.put(format!("{:02X}", p.address.station).as_bytes());
                dst.put(format!("{:02X}", p.address.plc).as_bytes());
                dst.put(p.data.as_bytes());
                dst.put_u8(ETX);
                let checksum = checksum(&dst[1..]);
                dst.put(format!("{:02X}", checksum).as_bytes());
                dst.put_u8(LF);
                Ok(())
            },
            Message::Ack(p) => {
                dst.reserve(6);
                dst.put_u8(ACK); //ACK
                dst.put(format!("{:02X}", p.station).as_bytes());
                dst.put(format!("{:02X}", p.plc).as_bytes());
                dst.put_u8(LF);
                Ok(())
            },
            Message::Nak(p) => {
                dst.reserve(6);
                dst.put_u8(NAK); //ACK
                dst.put(format!("{:02X}", p.station).as_bytes());
                dst.put(format!("{:02X}", p.plc).as_bytes());
                dst.put_u8(LF);
                Ok(())
            },
            Message::NakWithError(p) => {
                dst.reserve(8);
                dst.put_u8(NAK);
                dst.put(format!("{:02X}", p.address.station).as_bytes());
                dst.put(format!("{:02X}", p.address.plc).as_bytes());
                dst.put(format!("{:02X}", p.error_code).as_bytes());
                dst.put_u8(LF);
                Ok(())
            },
            Message::Request(p) => {
                let command_size = match &p.command {
                    WriteWords(c) => {
                        //TODO: could check if data.len is correct with number_of_device_points
                        4 + c.data.len()
                    }
                    ReadWords(_) => {
                        4
                    }
                };
                dst.reserve(9 + command_size); //TODO: take in to account the length of data
                dst.put_u8(ENQ); //ACK

                dst.put(format!("{:02X}", p.address.station).as_bytes());
                dst.put(format!("{:02X}", p.address.plc).as_bytes());

                match &p.command {
                    WriteWords(_) => {
                        dst.put("WW".as_bytes());
                    },
                    ReadWords(_) => {
                        dst.put("WR".as_bytes());
                    },
                };

                dst.put(format!("{:02X}", p.msg_wait_time)[1..2].as_bytes());

                match &p.command {
                    WriteWords(c) => {
                        dst.put(format!("{}", c.head_device).as_bytes());  //TODO: make sure its 5 and valid
                        dst.put(format!("{:02X}", c.number_of_device_points).as_bytes());
                        dst.put(format!("{}", c.data).as_bytes()); //TODO: make sure its valid
                    },
                    ReadWords(c) => {
                        dst.put(format!("{}", c.head_device).as_bytes());  //TODO: make sure its 5 and valid
                        dst.put(format!("{:02X}", c.number_of_device_points).as_bytes());
                    }
                };

                let checksum = checksum(&dst[1..]);
                dst.put(format!("{:02X}", checksum).as_bytes());

                dst.put_u8(LF);
                Ok(())
            },
        }

    }
}

fn checksum(data: &[u8]) -> u8 {
    let mut checksum = 0u8;
    for byte in data.iter() {
        checksum = checksum.wrapping_add(*byte);
    }
    checksum
}

pub struct Client {
    pub address: Address,
    pub msg_wait_time: u8,
    reader: FramedRead<ReadHalf<SerialStream>,FxCodec>,
    writer: FramedWrite<WriteHalf<SerialStream>,FxCodec>,
}
impl Client {
    pub fn new(station: u8, plc: u8, transport: SerialStream) -> Self {
        let (rx_port, tx_port) = tokio::io::split(transport);
        let reader = tokio_util::codec::FramedRead::new(rx_port, FxCodec::new());
        let writer = tokio_util::codec::FramedWrite::new(tx_port, FxCodec::new());
        Client {
            address: Address {
                station,
                plc,
            },
            msg_wait_time: 0,
            reader,
            writer,
        }
    }

    pub async fn write_i16(&mut self, head_devide: String, value: i16) { //TODO: return the errors
        let data = format!("{:04X}", value);
        self.writer.send(Message::Request(Request::new(self.address, self.msg_wait_time, Command::WriteWords(WriteWordsCommand::new(head_devide, 1, data))))).await.expect("write enq");
        if let Some(message_result) = self.reader.next().await { //TODO: handle the timeout
            match message_result {
                Ok(message) => {
                    match message {
                        Message::Ack(_r) => {
                            //println!("Response Ack: {:?}", r);
                        },
                        _ => {
                            println!("Unexpected Response Message: {:?}", &message);
                        }
                    }
                },
                Err(error) => {
                    println!("error: {:?}", error);
                }
            }
        }
    }

    pub async fn write_i32(&mut self, head_devide: String, value: i32) { //TODO: return the errors
        let data = format!("{:08X}", value);
        //println!("write_i32 send request");
        self.writer.send(Message::Request(Request::new(self.address, self.msg_wait_time, Command::WriteWords(WriteWordsCommand::new(head_devide, 2, data))))).await.expect("write enq");
        self.writer.flush().await.expect("write enq");
        //println!("write_i32 wait result");
        if let Some(message_result) = self.reader.next().await { //TODO: handle the timeout
            match message_result {
                Ok(message) => {
                    match message {
                        Message::Ack(_r) => {
                            //println!("Response Ack: {:?}", r);
                        },
                        _ => {
                            println!("Unexpected Response Message: {:?}", &message);
                        }
                    }
                },
                Err(error) => {
                    println!("error: {:?}", error);
                }
            }
        }
    }

    pub async fn read_i32(&mut self, head_devide: String) -> Result<i32, io::Error> { //TODO: return the errors
        //println!("read_i32 send request");
        self.writer.send( Message::Request(Request::new(self.address, self.msg_wait_time, Command::ReadWords(ReadWordsCommand::new(head_devide,2))))).await?;//.expect("write enq");
        self.writer.flush().await?;
        //println!("read_i32 wait result");
        if let Some(message_result) = self.reader.next().await {
            match message_result {
                Ok(message) => {

                    match message {
                        Message::Response(r) => {
                            //println!("Response Message: {:?}", r);
                            let v= u32::from_str_radix(&r.data, 16).unwrap();
                            self.writer.send(Message::Ack(self.address)).await?;
                            self.writer.flush().await?;
                            return Ok(v as i32);
                        },
                        _ => {
                            println!("Unexpected Response Message: {:?}", &message);
                            self.writer.send(Message::Nak(self.address)).await?;
                            self.writer.flush().await?;
                            return Err(io::Error::new(io::ErrorKind::Other, format!("No Ack Received on Read but got: {:?}", &message)));

                        }
                    }

                },
                Err(error) => {
                    println!("error: {:?}", error);
                    return Err(error);
                }
            }
        }
        return Err(io::Error::new(io::ErrorKind::Other, format!("No Response Recevied")));
    }
    pub async fn read_i16(&mut self, head_devide: String) -> Result<i16, io::Error> { //TODO: return the errors
        //println!("read_i32 send request");
        self.writer.send( Message::Request(Request::new(self.address, self.msg_wait_time, Command::ReadWords(ReadWordsCommand::new(head_devide,1))))).await?;//.expect("write enq");
        self.writer.flush().await?;
        //println!("read_i32 wait result");
        if let Some(message_result) = self.reader.next().await {
            match message_result {
                Ok(message) => {

                    match message {
                        Message::Response(r) => {
                            println!("Response Message: {:?}", r);
                            let v= u16::from_str_radix(&r.data, 16).unwrap();
                            self.writer.send(Message::Ack(self.address)).await?;
                            self.writer.flush().await?;
                            return Ok(v as i16);
                        },
                        _ => {
                            println!("Unexpected Response Message: {:?}", &message);
                            self.writer.send(Message::Nak(self.address)).await?;
                            self.writer.flush().await?;
                            return Err(io::Error::new(io::ErrorKind::Other, format!("No Ack Received on Read but got: {:?}", &message)));

                        }
                    }

                },
                Err(error) => {
                    println!("error: {:?}", error);
                    return Err(error);
                }
            }
        }
        return Err(io::Error::new(io::ErrorKind::Other, format!("No Response Recevied")));
    }

}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_WriteWordsCommand() {
        let mut codec = FxCodec::new();
        let mut buf = BytesMut::with_capacity(1000);

        let address = Address::new(0, 255);
        let message = Message::Request(Request::new(address,0, Command::WriteWords(WriteWordsCommand::new("M0640".to_string(), 2, "2347AB96".to_string()))));

        codec.encode(message, &mut buf);
        let b = buf.split();
        let restult = b.as_ref();

        assert_eq!(restult, b"\x0500FFWW0M0640022347AB9605\n");
    }

    #[test]
    fn check_checksum() {
        let result = checksum(b"05FFBRAX004005");
        assert_eq!(result, 0x47);
    }

    #[test]
    fn hex2i16() {
        let v :i16 = i16::MAX;
        let v = v.wrapping_add(100);
        assert_eq!(v, i16::MIN + 100 - 1);
        let data = format!("{:04X}", v);
        assert_eq!(data, "8063");
        let result= u16::from_str_radix(&data, 16).unwrap();
        let result = result as i16;
        assert_eq!(result, v);
    }


}
