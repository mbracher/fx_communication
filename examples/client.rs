#![warn(rust_2018_idioms)]

use futures::stream::StreamExt;
use std::{env, io, str};
use std::time::Duration;
use fx_communication::{FxCodec, Message, Address, NakWithError, Request, Response, Command, WriteWordsCommand, ReadWordsCommand, Client};
use tokio_util::codec::{Decoder, Encoder};
use tokio::io::{AsyncReadExt, AsyncWriteExt, AsyncWrite};
use bytes::BytesMut;
use futures::SinkExt;

use tokio_serial::{FlowControl, SerialPort, SerialPortBuilder, SerialPortBuilderExt};

#[cfg(unix)]
const DEFAULT_TTY: &str = "/dev/tty.usbserial-AWCUb116L16";
#[cfg(windows)]
const DEFAULT_TTY: &str = "COM1";

#[tokio::main]
async fn main() -> tokio_serial::Result<()> {
    let mut args = env::args();
    let tty_path = args.nth(1).unwrap_or_else(|| DEFAULT_TTY.into());


    let port = tokio_serial::new(tty_path, 9600)
        .open_native_async()?;

    let mut client = Client::new(5,255, port);
    let mut v1= 0i16;
    let mut v2= 10i32;

    loop {
        client.write_i16("D0106".to_string(), v1).await;
        //client.write_i32("D0105".to_string(), v2).await;
        tokio::time::sleep(Duration::from_millis(1)).await;
        print!(">");
        match client.read_i16("D0106".to_string()).await {
            Ok(result) => {
                println!("Read: {}", result);
            },
            Err(error) => {
                println!("Reed error: {:?}", error);
            }
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
        v1 = v1.wrapping_add(101);
        v2 = v2.wrapping_add(1);

    }
    // let (rx_port, tx_port) = tokio::io::split(port);
    // let mut reader = tokio_util::codec::FramedRead::new(rx_port, FxCodec::new());
    // let mut writer = tokio_util::codec::FramedWrite::new(tx_port, FxCodec::new());
    // let address = Address::new(77, 4);
    //
    // writer.send( Message::Request(Request::new(address,20, Command::WriteWords(WriteWordsCommand::new("D0105".to_string(), 1, "FE01".to_string()))))).await.expect("write enq");
    // if let Some(message_result) = reader.next().await {
    //     match message_result {
    //         Ok(message) => {
    //             match message {
    //                 Message::Ack(r) => {
    //                     println!("Response Ack: {:?}", r);
    //                 },
    //                 _ => {
    //                     println!("Unexpected Response Message: {:?}", &message);
    //                 }
    //             }
    //         },
    //         Err(error) => {
    //             println!("error: {:?}", error);
    //         }
    //     }
    // }
    //
    // writer.send( Message::Request(Request::new(address,20, Command::ReadWords(ReadWordsCommand::new("D0105".to_string(),1))))).await.expect("write enq");
    // if let Some(message_result) = reader.next().await {
    //     match message_result {
    //         Ok(message) => {
    //
    //             match message {
    //                 Message::Response(r) => {
    //                     println!("Response Message: {:?}", r);
    //                     writer.send(Message::Ack(address)).await;
    //
    //                 },
    //                 _ => {
    //                     println!("Unexpected Response Message: {:?}", &message);
    //                     writer.send(Message::Nak(address)).await;
    //                 }
    //             }
    //
    //         },
    //         Err(error) => {
    //             println!("error: {:?}", error);
    //         }
    //     }
    // }
    //
    // writer.send( Message::Request(Request::new(address,20, Command::WriteWords(WriteWordsCommand::new("D0105".to_string(), 1, "FFFF".to_string()))))).await.expect("write enq");
    // if let Some(message_result) = reader.next().await {
    //     match message_result {
    //         Ok(message) => {
    //             match message {
    //                 Message::Ack(r) => {
    //                     println!("Response Ack: {:?}", r);
    //                 },
    //                 _ => {
    //                     println!("Unexpected Response Message: {:?}", &message);
    //                 }
    //             }
    //         },
    //         Err(error) => {
    //             println!("error: {:?}", error);
    //         }
    //     }
    // }
    //
    // writer.send( Message::Request(Request::new(address,20, Command::ReadWords(ReadWordsCommand::new("D0105".to_string(),1))))).await.expect("write enq");
    // if let Some(message_result) = reader.next().await {
    //     match message_result {
    //         Ok(message) => {
    //
    //             match message {
    //                 Message::Response(r) => {
    //                     println!("Response Message: {:?}", r);
    //                     writer.send(Message::Ack(address)).await;
    //
    //                 },
    //                 _ => {
    //                     println!("Unexpected Response Message: {:?}", &message);
    //                     writer.send(Message::Nak(address)).await;
    //                 }
    //             }
    //
    //         },
    //         Err(error) => {
    //             println!("error: {:?}", error);
    //         }
    //     }
    // }
    //
    // writer.flush().await.expect("flush2");


    // Do not exit until all bytes are written
    tokio::time::sleep(Duration::from_secs(3)).await;
    Ok(())
}

//TODO: test on windows
//TODO: test serial2-rs