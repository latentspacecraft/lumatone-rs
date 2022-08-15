#![allow(dead_code)]


// TODO: 
// - [ ] structs for lumatone commands
// - [ ] encoder to convert commands to/from sysex byte stream

use super::constants::{BoardIndex, CommandId, MANUFACTURER_ID};
use std::error::Error;
use num_traits::FromPrimitive;

// index into sysex data of various fields
pub const MANU_0: usize = 0x0;
pub const MANU_1: usize = 0x1;
pub const MANU_3: usize = 0x2;
pub const BOARD_IND: usize = 0x3;
pub const CMD_ID: usize = 0x4;
pub const MSG_STATUS: usize = 0x5;
pub const CALIB_MODE: usize = 0x5;
pub const PAYLOAD_INIT: usize = 0x6;

const SYSEX_START: u8 = 0xf0;
const SYSEX_END: u8 = 0xf7;

pub type EncodedSysex = Vec<u8>;

pub fn create_sysex(board_index: BoardIndex, cmd: CommandId, data: Vec<u8>) -> EncodedSysex {
  // FIXME: add sysex start / end bytes
  let mut sysex: Vec<u8> = vec![SYSEX_START];
  sysex.extend(MANUFACTURER_ID.iter());
  sysex.push(board_index.into());
  sysex.push(cmd.into());
  sysex.extend(data.iter());
  sysex.push(SYSEX_END);
  sysex
}

pub fn create_sysex_toggle(board_index: BoardIndex, cmd: CommandId, state: bool) -> EncodedSysex {
  let s: u8 = if state { 1 } else { 0 };
  create_sysex(board_index, cmd, vec![s])
}

pub fn create_extended_key_color_sysex(
  board_index: BoardIndex,
  cmd: CommandId,
  key_index: u8,
  red: u8,
  green: u8,
  blue: u8
) -> EncodedSysex {
  let mut colors = encode_rgb(red, green, blue);
  let mut data = vec![key_index];
  data.append(&mut colors);
  create_sysex(board_index, cmd, data)
}

pub fn create_extended_macro_color_sysex(
  cmd: CommandId,
  red: u8,
  green: u8,
  blue: u8
) -> EncodedSysex {
  let colors = encode_rgb(red, green, blue);
  create_sysex(BoardIndex::Server, cmd, colors)
}

/**
 * Returns the given RGB values, encoded into 6 u8's with the lower 4 bits set.
 */
fn encode_rgb(red: u8, green: u8, blue: u8) -> Vec<u8> {
  let red_hi = red >> 4;
  let red_lo = red & 0xf;
  let green_hi = green >> 4;
  let green_lo = green & 0xf;
  let blue_hi = blue >> 4;
  let blue_lo = blue & 0xf;
  vec![ red_hi, red_lo, green_hi, green_lo, blue_hi, blue_lo ]
}



pub fn strip_sysex_markers<'a>(msg: &'a [u8]) -> &'a [u8] {
  if msg.len() == 0 {
    return &msg;
  }

  let start = if msg[0] == SYSEX_START { 1 } else { 0 };
  let mut end = msg.len()-1;
  if msg[end] == SYSEX_END {
    end -= 1;
  }
  &msg[start..=end]
}

pub fn is_lumatone_message(msg: &[u8]) -> bool {
  let msg = strip_sysex_markers(msg);

  if msg.len() < 3 {
    return false
  }
  for (a, b) in MANUFACTURER_ID.iter().zip(msg.iter()) {
    if *a != *b {
      return false
    }
  }
  return true
}

pub fn message_payload<'a>(msg: &'a [u8]) -> Result<&'a [u8], Box<dyn Error>> {
  let msg = strip_sysex_markers(msg);
  if msg.len() < PAYLOAD_INIT {
    return Err("message too short, unable to extract payload".into())
  }
  Ok(&msg[PAYLOAD_INIT..])
}

pub fn message_command_id(msg: &[u8]) -> Result<CommandId, Box<dyn Error>> {
  let msg = strip_sysex_markers(msg);
  if msg.len() <= CMD_ID {
    return Err("message too short - unable to determine command id".into());
  }
  let cmd_id = msg[CMD_ID];
  let cmd: Option<CommandId> = FromPrimitive::from_u8(cmd_id);
  cmd.ok_or("unknown command id".into())
}