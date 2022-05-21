#![feature(type_alias_impl_trait)]
#![feature(trait_alias)]

mod eunix;
mod machine;
mod util;

use machine::{Machine, OperatingSystem};

use crate::{eunix::{e5fs::*, fs::{Filesystem, OpenFlags, OpenMode}}, machine::VirtualDeviceType};
use std::{
  fs::File,
  io::{Read, Seek, SeekFrom},
  path::Path,
};

// use machine::{Machine, OperatingSystem};

pub fn main() {
  let machine = Machine::new(
    Path::new(env!("CARGO_MANIFEST_DIR")).join("machines/1/machine.yaml")
      .to_str()
      .unwrap()
  );
  let mut os = OperatingSystem {
    kernel: eunix::kernel::Kernel::new(machine.device_table()),
  };

  let (sda1_realpath, _) = machine
    .device_table()
    .devices
    .iter()
    .take(1)
    .find(|(_realpath, &dev_type)| dev_type == VirtualDeviceType::BlockDevice)
    .unwrap();

  E5FSFilesystem::mkfs(sda1_realpath, 0.05, 4096).unwrap();

  os.kernel.mount("", "/dev", eunix::fs::FilesystemType::devfs).unwrap();
  os.kernel.mount("", "/bin", eunix::fs::FilesystemType::binfs).unwrap();
  os.kernel.mount("/dev/sda", "/", eunix::fs::FilesystemType::e5fs).unwrap();

  let fd = os.kernel
    .open("/test-file.txt", OpenFlags { 
      mode: OpenMode::ReadWrite, 
      create: true, 
      append: true 
    })
    .unwrap();

  os.kernel.close(fd).unwrap();

  println!("mount_points: {:#?}", os.kernel.vfs().mount_points);
  println!("processes: {:#?}", os.kernel.processes());
}

// vim:ts=2 sw=2
