#![feature(type_alias_impl_trait)]
#![feature(trait_alias)]
#![feature(const_fmt_arguments_new)]

mod eunix;
mod machine;
mod util;
mod binaries;

use fancy_regex::Regex;
use machine::{Machine, OperatingSystem};
use std::io::*;
use crate::{eunix::{e5fs::*, fs::{Filesystem, FileModeType, VFS}, kernel::{KERNEL_MESSAGE_HEADER_ERR, KernelParams, Errno}, binfs::BinFilesytem}, machine::VirtualDeviceType, binaries::EXIT_SUCCESS};
use std::path::Path;

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

  // E5FSFilesystem::mkfs(sda1_realpath, 0.05, 4096).unwrap();

  os.kernel.mount("", "/dev", eunix::fs::FilesystemType::devfs).unwrap();
  os.kernel.mount("", "/bin", eunix::fs::FilesystemType::binfs).unwrap();
  os.kernel.mount("/dev/sda", "/", eunix::fs::FilesystemType::e5fs).unwrap();

  // let eunix_inode = os.kernel.vfs.create_file("/mnt").unwrap();
  // let mnt_inode = os.kernel.vfs.create_dir("/mnt").unwrap();
  // assert_eq!(mnt_inode.number, 1, "mnt_inode should be 1");
  // let mnt_eblan_inode = os.kernel.vfs.create_dir("/mnt/eblan").unwrap();
  // assert_eq!(mnt_eblan_inode.number, 2, "mnt_eblan_inode should be 2");

  let binfs = os.kernel.vfs.mount_points.get_mut("/bin").unwrap().driver.as_any().downcast_mut::<BinFilesytem>().unwrap();
  binfs.create_dir("/eblan").unwrap();
  binfs.create_file("/eblan/ls").unwrap();

  binfs.add_bins(vec![
    (String::from("/ls"),           binaries::ls),        // [x]
    (String::from("/stat"),         binaries::stat),      // [x]
    (String::from("/df"),           binaries::df),        // [ ]
    (String::from("/du"),           binaries::du),        // [ ]
    (String::from("/cat"),          binaries::cat),       // [x]
    (String::from("/mkfs.e5fs"),    binaries::mkfs_e5fs), // [x]
    (String::from("/mkdir"),        binaries::mkdir),     // [x]
    (String::from("/rmdir"),        binaries::rmdir),     // [ ]
    (String::from("/touch"),        binaries::touch),     // [x]
    (String::from("/rm"),           binaries::rm),        // [x]
    (String::from("/mv"),           binaries::mv),        // [ ]
    (String::from("/cp"),           binaries::cp),        // [ ]
    (String::from("/write"),        binaries::write),     // [x]
    (String::from("/ed"),           binaries::ed),        // [x]
    (String::from("/chmod"),        binaries::chmod),     // [ ]
    (String::from("/chown"),        binaries::chown),     // [ ]
    (String::from("/uname"),        binaries::uname),     // [ ]
    (String::from("/mount"),        binaries::mount),     // [x]
    (String::from("/lsblk"),        binaries::lsblk),     // [x]
    (String::from("/id"),           binaries::id),        // [ ]
    (String::from("/whoami"),       binaries::whoami),    // [ ]
    (String::from("/su"),           binaries::su),        // [ ]
    (String::from("/useradd"),      binaries::useradd),   // [ ]
    (String::from("/usermod"),      binaries::usermod),   // [ ]
    (String::from("/userdel"),      binaries::userdel),   // [ ]
    (String::from("/groupmod"),     binaries::groupmod),  // [ ]
    (String::from("/groupdel"),     binaries::groupdel),  // [ ]
  ]).expect("we know that we have enough inodes and there is no dublicates");

  // Shell vars
  let mut ifs = ' ';
  let mut ps1 = String::from("(0) # ");
  let mut pwd = String::from("/");
  let mut path = String::from("/usr/bin:/bin");

  let mut command = String::new();

  loop {
    // A basic REPL prompt
    command.clear();
    print!("{}", ps1);
    stdout().flush().unwrap();
    stdin().read_line(&mut command).unwrap();

    // Parse args
    let args = command
      .trim() // Trim leading newline
      .split(ifs) // Split by IFS (space)
      .collect::<Vec<&str>>(); // Collect as [arg0, arg1, arg2, ...]

    /* Execute command
     * args[0] - program (or builtin) pathname/name 
     * args[1..] - arguments 
    */
    match args[0] {
      /* Echo buintin */
      "echo" => {
        let args = args[1..].join(" ");
        println!("{args}");
      },

      /* Cd buintin */
      "cd" => {
        let pathname = args[1];
        
        match os.kernel.vfs.lookup_path(pathname) {
          Ok(vinode) => {
            if vinode.mode.file_type() == FileModeType::Dir as u8 {
              pwd = pathname.to_owned();
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

      /* Pwd (print working directory) buintin */
      "pwd" => {
        println!("{pwd}");
      },

      /* Exit buintin */
      "exit" => break,

      /* No builtin matched - run pathname */
      command => {
        // Calculate pathname
        // Match command against PATH: 
        // if (found in PATH) -> return new pathname
        // otherwise          -> return command literally
        let pathname = if Regex::new("^[_\\.a-zA-Z][^\\/\\n]*$")
          .unwrap()
          .is_match(command)
          .unwrap()
        {
          if let Some(pathname) = path
            .split(':')
            .find_map(|location_pathname| {
              let pathname = format!("{location_pathname}/{command}");
              os.kernel.vfs.lookup_path(&pathname).ok().and_then(|_| Some(pathname))
            })
          {
            pathname
          } else {
            command.to_string()
          }
        } else {
          command.to_string()
        };
        
        // Execute calculated pathname
        match os.kernel.exec(&pathname, args.as_ref()) {
          Ok(exit_code) => {
            // println!("[{KERNEL_MESSAGE_HEADER_ERR}]: program finished with exit code {exit_code}");
            ps1 = if exit_code == EXIT_SUCCESS {
              // format!("# ")
              format!("({exit_code}) # ")
            } else {
              format!("({exit_code}) # ")
            }
          },
          Err(Errno::ENOENT(_)) => {
            println!("sh: no such file or directory: {pathname}");
          },
          Err(errno) => {
            println!("[{KERNEL_MESSAGE_HEADER_ERR}]: kernel can't exec {pathname}: ERRNO: {errno:?}");
          },
        }
      }
    }
  }
}

// vim:ts=2 sw=2
