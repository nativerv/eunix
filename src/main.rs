mod eunix;
mod machine;
mod util;

use crate::eunix::e5fs::*;
use std::{
  fs::File,
  io::{Read, Seek, SeekFrom},
  path::Path,
};

// use machine::{Machine, OperatingSystem};

pub fn main() {
  // let machine = Machine::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("machines/1/machine.yaml").to_str().unwrap());
  // let os = OperatingSystem {
  //   kernel: eunix::kernel::Kernel::new(machine.get_devices()),
  // };
  //
  // println!("Machine: {:?}", machine);
  // println!();
  // println!("OS: {:?}", os);
}
