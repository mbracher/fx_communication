#![warn(rust_2018_idioms)]

use futures::stream::StreamExt;
use std::{env, str};
use std::collections::hash_map::Entry::{Occupied, Vacant};

use std::collections::HashMap;

use fx_communication::{FxCodec, Message,  Response};

use tokio_serial::{FlowControl, SerialPortBuilderExt};
use fx_communication::Command::{ReadWords, WriteWords};
use futures::SinkExt;

#[cfg(unix)]
const DEFAULT_TTY: &str = "/dev/tty.usbserial-CODWb116L16";
#[cfg(windows)]
const DEFAULT_TTY: &str = "COM2";



#[tokio::main]
async fn main() -> tokio_serial::Result<()> {
    let mut args = env::args();
    let tty_path = args.nth(1).unwrap_or_else(|| DEFAULT_TTY.into());

    let mut port = tokio_serial::new(tty_path, 9600)
        .flow_control(FlowControl::Hardware)
        .open_native_async()?;

    #[cfg(unix)]
    port.set_exclusive(false)
        .expect("Unable to set serial port exclusive to false");

    let (rx_port, tx_port) = tokio::io::split(port);
    let mut reader = tokio_util::codec::FramedRead::new(rx_port, FxCodec::new());
    let mut writer = tokio_util::codec::FramedWrite::new(tx_port, FxCodec::new());




    let mut register = HashMap::new();
    while let Some(message_result) = reader.next().await {
        match message_result {
            Ok(message) => {
                match message {
                    Message::Request(p) => {
                        //println!("Received Request: {:?}", &p);
                        match &p.command {
                            WriteWords(c) => {
                                let v = register.entry(c.head_device.clone()).or_insert(c.data.clone());
                                println!("replaced in register: {} old: {} new: {}", c.head_device,  v, &c.data);
                                *v = c.data.clone();

                                writer.send(Message::Ack(p.address)).await?;
                                writer.flush().await?;

                            },
                            ReadWords(c) => {
                                let v = register.entry(c.head_device.clone());

                                let r = match v {
                                    Occupied(e) => {
                                        e.get().clone()
                                    },
                                    Vacant(_e) => {
                                        "0000".to_string()
                                    }
                                };
                                //println!("read from register: {:?}", r);
                                writer.send(Message::Response(Response::new(p.address, r))).await?;
                                writer.flush().await?;
                                if let Some(message_result) = reader.next().await {
                                    match message_result {
                                        Ok(message) => {
                                            match message {
                                                Message::Ack(_r) => {
                                                    //println!("Received Ack: {:?}", r);
                                                },
                                                _ => {
                                                    println!("Received Unexpected Response Message: {:?}", &message);
                                                }
                                            }
                                        },
                                        Err(error) => {
                                            println!("error: {:?}", error);
                                        }
                                    }
                                }
                            }
                        }
                    },
                    Message::Ack(p) => println!("Received Unexpected Ack: {:?}", p),
                    Message::Nak(p) => println!("Received Unexpected Nak: {:?}", p),
                    Message::NakWithError(p) => println!("Received Unexpected NakWithError: {:?}", p),
                    Message::Response(p) =>  println!("Received Unexpected Response Message: {:?}", p),
                }
            },

            Err(error) => {
                println!("error: {:?}", error)
            },
        }


    }

    Ok(())
}