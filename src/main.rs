mod eunix;
mod machine;
mod util;

use machine::{Machine, OperatingSystem};

use crate::eunix::{e5fs::*, fs::Filesystem};
use std::{
  fs::File,
  io::{Read, Seek, SeekFrom},
  path::Path,
};

// use machine::{Machine, OperatingSystem};

pub fn main() {
  let machine = Machine::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("machines/1/machine.yaml").to_str().unwrap());
  let mut os = OperatingSystem {
    kernel: eunix::kernel::Kernel::new(machine.device_table()),
  };

  println!("ping from main 1");
  os.kernel.mount("", "/dev", eunix::fs::FilesystemType::devfs).unwrap();
  println!("ping from main 2");

  println!("ping from main 3");
  let dev_dir = os.kernel.vfs.read_dir("/dev").unwrap();
  println!("ping from main 4");
  println!("dev_dir: {:#?}", dev_dir);

  os.kernel.mount("/dev/sda", "/", eunix::fs::FilesystemType::e5fs).unwrap();
  

  println!("Machine: {:#?}", machine);
  println!();
  println!("OS: {:#?}", os);
  println!();
  println!("mount_points: {:#?}", os.kernel.vfs().mount_points);
}

// vim:ts=2 sw=2
