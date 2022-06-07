#![feature(type_alias_impl_trait)]
#![feature(trait_alias)]
#![feature(const_fmt_arguments_new)]
#![feature(let_chains)]

mod eunix;
mod machine;
mod util;
mod binaries;

use fancy_regex::Regex;
use machine::{Machine, OperatingSystem};
use sha2::{Sha256, Digest};
use std::io::*;
use crate::{eunix::{fs::{Filesystem, FileModeType, EVERYTHING, Id}, kernel::{KERNEL_MESSAGE_HEADER_ERR, KernelParams, Errno, ROOT_UID}, binfs::BinFilesytem, users::Passwd, e5fs::E5FSFilesystem}, machine::VirtualDeviceType, binaries::{EXIT_SUCCESS, PASSWD_PATH}};
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
  os.kernel.mount("/dev/sda", "/", eunix::fs::FilesystemType::e5fs).unwrap();


  os.kernel.mount("", "/bin", eunix::fs::FilesystemType::binfs).unwrap();

  // let e5fs = os
  //   .kernel
  //   .vfs
  //   .mount_points
  //   .get_mut("/")
  //   .unwrap()
  //   .driver
  //   .as_any()
  //   .downcast_mut::<E5FSFilesystem>()
  //   .unwrap();
  // e5fs.create_dir("/proc").unwrap();
  // e5fs.create_dir("/root").unwrap();
  // e5fs.create_dir("/dev").unwrap();
  // e5fs.create_dir("/sys").unwrap();
  // e5fs.create_dir("/bin").unwrap();
  // drop(e5fs);

  if let Err(errno) = os.kernel.update_uid_gid_maps() {
    println!("[{KERNEL_MESSAGE_HEADER_ERR}]: cannot update '{PASSWD_PATH}': {errno:?}");
  }

  // let eunix_inode = os.kernel.vfs.create_file("/mnt").unwrap();
  // let mnt_inode = os.kernel.vfs.create_dir("/mnt").unwrap();
  // assert_eq!(mnt_inode.number, 1, "mnt_inode should be 1");
  // let mnt_eblan_inode = os.kernel.vfs.create_dir("/mnt/eblan").unwrap();
  // assert_eq!(mnt_eblan_inode.number, 2, "mnt_eblan_inode should be 2");

  let binfs = os
    .kernel
    .vfs
    .mount_points
    .get_mut("/bin")
    .unwrap()
    .driver
    .as_any()
    .downcast_mut::<BinFilesytem>()
    .unwrap();
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
    (String::from("/mv"),           binaries::mv),        // [x]
    (String::from("/cp"),           binaries::cp),        // [x]
    (String::from("/write"),        binaries::write),     // [x]
    (String::from("/ed"),           binaries::ed),        // [x]
    (String::from("/chmod"),        binaries::chmod),     // [x]
    (String::from("/chown"),        binaries::chown),     // [x]
    (String::from("/uname"),        binaries::uname),     // [x]
    (String::from("/mount"),        binaries::mount),     // [x]
    (String::from("/lsblk"),        binaries::lsblk),     // [x]
    (String::from("/passwd"),       binaries::passwd),    // [x]
    (String::from("/id"),           binaries::id),        // [x]
    (String::from("/whoami"),       binaries::whoami),    // [x]
    (String::from("/su"),           binaries::su),        // [x]
    (String::from("/useradd"),      binaries::useradd),   // [x]
    (String::from("/usermod"),      binaries::usermod),   // [ ]
    (String::from("/userdel"),      binaries::userdel),   // [x]
    (String::from("/groupmod"),     binaries::groupmod),  // [ ]
    (String::from("/groupdel"),     binaries::groupdel),  // [ ]
  ]).expect("we know that we have enough inodes and there is no dublicates");

  let mut input_username = String::new();
  let mut input_password = String::new();

  println!("Eunix v1.0.0 (tty1)");
  println!();

  // match os.kernel.vfs.read_file(PASSWD_PATH, EVERYTHING) {
  //   Ok(bytes) => {
  //     loop {
  //       print!("eunix login: ");
  //       stdout().flush().unwrap();
  //       stdin().read_line(&mut input_username).unwrap();
  //       print!("Password: ");
  //       stdout().flush().unwrap();
  //       stdin().read_line(&mut input_password).unwrap();
  //       let input_username = input_username.trim();
  //       // let input_password = input_password.trim();
  //       let input_password = hex::encode(Sha256::digest(&input_password.as_bytes())); 
  //       let contents = String::from_utf8(bytes.clone()).unwrap();
  //       let passwds = Passwd::parse_passwds(&contents);
  //
  //       match passwds.iter().find(|&p| p.name == input_username) {
  //         Some(Passwd { password, uid, gid, .. }) => {
  //           if *password == input_password {
  //             os.kernel.current_uid = *uid;
  //             os.kernel.current_gid = *gid;
  //             os.kernel.update_vfs_current_uid_gid();
  //             break;
  //           }
  //         },
  //         None => {
  //         },
  //       }
  //       println!("Login incorrect");
  //       println!();
  //     }
  //   },
  //   Err(Errno::ENOENT(_)) => {
  //     println!("login: {PASSWD_PATH} does not exist, logging as root");
  //   },
  //   Err(errno) => {
  //     println!("login: unexpected error: {errno:?}");
  //   },
  // }

  fn caret_by_uid(uid: Id) -> String {
    if uid == ROOT_UID {
      String::from("#")
    } else {
      String::from("$")
    }
  }

  // Shell vars
  let ifs = ' ';
  let mut ps1 = format!("({: >3}) {} ", 0, caret_by_uid(os.kernel.current_uid));
  let mut pwd = String::from("/");
  let path = String::from("/usr/bin:/bin");

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
            ps1 = format!("({exit_code: >3}) {} ", caret_by_uid(os.kernel.current_uid));
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
