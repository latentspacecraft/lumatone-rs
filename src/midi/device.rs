#![allow(dead_code)]

use std::{error::Error};
use log::warn;
use midir::{MidiInput, MidiOutput, MidiIO, MidiInputConnection, MidiOutputConnection};
use tokio::sync::mpsc;

use super::sysex::EncodedSysex;

#[derive(Debug)]
pub struct LumatoneDevice {
  out_port_name: String,
  in_port_name: String, 
}

pub struct LumatoneIO {
  input_conn: MidiInputConnection<()>,
  output_conn: MidiOutputConnection,

  pub incoming_messages: mpsc::Receiver<EncodedSysex>,
}

impl LumatoneDevice {
  pub fn new(output_port_name: &str, input_port_name: &str) -> LumatoneDevice {
    LumatoneDevice {
      out_port_name: output_port_name.to_string(),
      in_port_name: input_port_name.to_string(),
    }
  }

  pub fn connect(&self) -> Result<LumatoneIO, Box<dyn Error>> {
    let client_name = "lumatone-rs";
    let input = MidiInput::new(client_name)?;
    let output = MidiOutput::new(client_name)?;

    let in_port = get_port_by_name(&input, &self.in_port_name)?;
    let out_port = get_port_by_name(&output, &self.out_port_name)?;

    let buf_size = 32;
    let (incoming_tx, incoming_messages) = mpsc::channel(buf_size);

    let input_conn = input.connect(&in_port, &self.in_port_name, move |_,msg,_| {
      let msg = msg.to_vec();
      if let Err(err) = incoming_tx.blocking_send(msg) {
        warn!("error sending incoming message on channel: {err}");
      }
    }, ())?;

    let output_conn = output.connect(&out_port, &self.out_port_name)?;

    let io = LumatoneIO {
      input_conn,
      output_conn,
      incoming_messages,
    };
    Ok(io)
  }

}

impl LumatoneIO {
  pub fn send(&mut self, msg: &[u8]) -> Result<(), midir::SendError> {
    self.output_conn.send(msg)
  }
}

fn get_port_by_name<IO: MidiIO> (io: &IO, name: &str) -> Result<IO::Port, Box<dyn Error>> {
  for p in io.ports() {
    let port_name = io.port_name(&p)?;
    if port_name == name {
      return Ok(p);
    }
  }
  Err(format!("no port found with name {name}").into())
}