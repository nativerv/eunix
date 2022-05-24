#![feature(type_alias_impl_trait)]
#![feature(trait_alias)]
#![feature(const_fmt_arguments_new)]

mod eunix;
mod machine;
mod util;

use itertools::Itertools;
use machine::{Machine, OperatingSystem};

use crate::{eunix::{e5fs::*, fs::{Filesystem, OpenFlags, OpenMode, FileModeType}, kernel::{KERNEL_MESSAGE_HEADER_ERR, KernelParams, Errno}, binfs::BinFilesytem}, machine::VirtualDeviceType};
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

  let binfs = &mut os.kernel.vfs.mount_points.get_mut("/bin").unwrap().driver.as_any().downcast_mut::<BinFilesytem>().unwrap();
  // let inodes = &mut binfs.virtfs.inodes;
  // println!("{inodes:#?}");
  // drop(inodes);
  // let payloads = &mut binfs.virtfs.payloads;
  // println!("{payloads:#?}");
  binfs.create_file("/ls").unwrap();
  binfs.create_dir("/eblan").unwrap();
  binfs.create_file("/eblan/ls").unwrap();
  binfs.write_binary("/ls", |args, kernel| {
    if let Some(pathname) = args.get(1) {
      let dir = kernel.vfs.read_dir(&pathname).expect("ls: we know that this is a dir");
      println!("{dir:?}");
      0
    } else {
      1
    }

    // println!("args = {args:?}");
    // if let Some(pathname) = args.get(1) {
    //   let stat = kernel.vfs.stat(pathname); 
    //   match kernel.vfs.read_dir(&pathname) {
    //     Ok(_) => {
    //       println!("{dir:?}");
    //       0
    //     },
    //     Err(Errno::ENOTDIR(_)) => {
    //       eprintln!("ls: ");
    //     },
    //   }
    // } else {
    //   1
    // }
  }).unwrap();
  let stat = binfs.stat("/eblan/ls").unwrap();
  // println!("{stat:#?}");
  // println!("{mount_points:#?}", mount_points = os.kernel.vfs.mount_points);

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
    // Shell vars
    let mut ifs = ' ';
    let mut ps1 = "# ";
    let mut pwd = "/";

    // A basic REPL prompt
    command.clear();
    print!("{ps1}");
    stdout().flush().unwrap();
    stdin().read_line(&mut command).unwrap();

    // Parse args
    let args = command
      .trim() // Trim leading newline
      .split(ifs) // Split by IFS (space)
      .collect::<Vec<&str>>(); // Collect as [arg0, arg1, arg2, ...]

    // Execute command
    // args[0] - program (or builtin) pathname/name 
    // args[1..] - arguments
    match args[0] {
      // Echo buintin
      "echo" => {
        let args = args[1..].join(" ");
        println!("{args}");
      },

      // Cd buintin
      "cd" => {
        let pathname = args[1];
        
        match os.kernel.vfs.lookup_path(pathname) {
          Ok(vinode) => {
            if vinode.mode.r#type() == FileModeType::Dir as u8 {
              pwd = pathname;
            } else {
              eprintln!("cd: not a directory: {pathname}")
            }
          },
          Err(Errno::ENOENT(_)) => {
            eprintln!("cd: no such file or directory: {pathname}")
          },
          Err(errno) => {
            eprintln!("cd: unexpected kernel error occured while looking for {pathname}: {errno:?}")
          },
        }
      },

      // Exit buintin
      "exit" => break,

      // No builtin matched - run pathname
      pathname => {
        match os.kernel.exec(pathname, args.as_ref()) {
          Ok(exit_code) => {
            println!("[{KERNEL_MESSAGE_HEADER_ERR}]: program finished with exit code {exit_code}");
          },
          Err(errno) => {
            println!("[{KERNEL_MESSAGE_HEADER_ERR}]: program crashed with ERRNO: {errno:?}");
          }
        }
      }
    }
  }
}

// vim:ts=2 sw=2
