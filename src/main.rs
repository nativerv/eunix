#![feature(type_alias_impl_trait)]
#![feature(trait_alias)]

mod eunix;
mod machine;
mod util;

use itertools::Itertools;
use machine::{Machine, OperatingSystem};

use crate::{eunix::{e5fs::*, fs::{Filesystem, OpenFlags, OpenMode}, kernel::KernelParams, binfs::BinFilesytem}, machine::VirtualDeviceType};
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
    kernel: eunix::kernel::Kernel::new(machine.device_table(), KernelParams {
      init: String::from("/bin/init"),
    }),
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

  let binfs = &mut os.kernel.vfs.mount_points.get_mut("/bin").unwrap().driver;
  let inodes = &mut binfs.as_any().downcast_mut::<BinFilesytem>().unwrap().virtfs.inodes;
  // println!("{inodes:#?}");
  drop(inodes);
  let payloads = &mut binfs.as_any().downcast_mut::<BinFilesytem>().unwrap().virtfs.payloads;
  // println!("{payloads:#?}");
  binfs.create_file("/ls").unwrap();
  binfs.create_dir("/eblan").unwrap();
  binfs.create_file("/eblan/ls").unwrap();
  let stat = binfs.stat("/eblan/ls").unwrap();
  println!("{stat:?}");
  // println!("/bin/ls: {stat:?}", stat = os.kernel.vfs.stat("/bin/ls").unwrap());

  // This panics with lookup_path error of ENOENT (probably should actually `craete` file if create
  // is set to `true`, eh?)
  // let fd = os.kernel
  //   .open("/test-file.txt", OpenFlags { 
  //     mode: OpenMode::ReadWrite, 
  //     create: true, 
  //     append: true 
  //   })
  //   .unwrap();

  // os.kernel.close(fd).unwrap();

  // println!("mount_points: {:#?}", os.kernel.vfs().mount_points);
  // println!("processes: {:#?}", os.kernel.processes());


  use std::io::*;

  let mut command = String::new();
  loop {
    command.clear();
    print!("# ");
    stdout().flush().unwrap();
    stdin().read_line(&mut command).unwrap();
    let args = command
      .trim()
      .split(' ')
      .collect::<Vec<&str>>();

    match args[0] {
      "echo" => {
        let args = args.iter().skip(1).join(" ");
        println!("{args}");
      },
      "exit" => break,
      _ => {
        println!("{command}");
      }
    }
  }
}

// vim:ts=2 sw=2
