use std::fmt::Error;
use std::time::SystemTime;

use crate::eunix::kernel::Kernel;
use crate::{eunix::fs::Filesystem, machine::DeviceTable};

use super::fs::{AddressSize, VDirectoryEntry, VINode};
use super::kernel::Errno;

pub struct DirectoryEntry<'a> {
  inode_address: AddressSize,
  name: &'a str,
  next_dir_entry_offset: AddressSize,
}

pub type Directory<'a> = Vec<DirectoryEntry<'a>>;

pub struct INode {
  mode: u16,
  links_count: AddressSize,
  uid: u32,
  gid: u32,
  file_size: AddressSize,
  atime: u32,
  mtime: u32,
  ctime: u32,
}

pub struct Superblock {
  filesystem_type: [u8; 255],
  filesystem_size: AddressSize, // in blocks
  inode_table_size: AddressSize,
  free_inodes_count: AddressSize,
  free_blocks_count: AddressSize,
  inodes_count: AddressSize,
  blocks_count: AddressSize,
  block_size: AddressSize,
  free_inodes: [AddressSize; 16],
  free_blocks: [AddressSize; 16],
}

pub struct Block<'a> {
  is_free: bool,
  data: &'a [u8],
  next_block: AddressSize,
}

pub struct DeviceFilesystem<'a> {
  devices: &'a DeviceTable,
  inodes: Vec<INode>,
}

impl <'a> DeviceFilesystem <'a> {
  fn new(devices: &'a DeviceTable) -> Self {
    // Self {
    //   devices,
    //   inodes: devices.iter().map(|(&path, &device)| INode {
    //     // r = read
    //     // w = write
    //     // x = execute
    //     // m = mode (000 = file, 001 = directory, 010 = block, 011 = char, 100 - sys)
    //     // u = used
    //     // n - n/a
    //     //      nnn u mmm rwx rwx rwx
    //     mode: 0b000_1_000_111_000_000,
    //     links_count: 1,
    //     uid: 0,
    //     gid: 0,
    //     file_size: 0,
    //     atime: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis().try_into().unwrap(),
    //     mtime: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis().try_into().unwrap(),
    //     ctime: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis().try_into().unwrap(),
    //   }).collect()
    // }
    todo!();
  }
}

impl<'a> Filesystem for DeviceFilesystem<'a> {
  fn read_bytes(&self, pathname: &str, count: AddressSize) -> Result<&[u8], Errno> {
    Err(Errno::EPERM)
  }

  fn write_bytes(&mut self, pathname: &str) -> Result<(), Errno> {
    Err(Errno::EPERM)
  }

  fn read_dir(&self, pathname: &str) -> &[VDirectoryEntry] {
    let mut tty_devices_number: u32 = 0;
    let mut block_devices_number: u32 = 0;

    // self
    //   .devices
    //   .iter()
    //   .zip(1..)
    //   .map(|((&path, &device), letter_number)| {
    //     VDirectoryEntry {
    //       name: match device {
    //         crate::machine::VirtualDevice::BlockDevice => {
    //           block_devices_number += 1;
    //           String::from("sd").push(char::from_u32(64u32 + block_devices_number).unwrap())
    //         }
    //         crate::machine::VirtualDevice::TTYDevice => {
    //           tty_devices_number += 1;
    //           String::from("tty").push(tty_devices_number)
    //         }
    //       },
    //       num_inode: letter_number, // TODO: FIXME
    //     }
    //   })
    todo!();
  }

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&self, pathname: &str) -> VINode {
    todo!();
  }

  fn get_name(&self) -> String {
    "devfs".to_owned()
  }
}

// impl DeviceFilesystem {
//   fn mkfs(percent_inodes: u32, block_size: AddressSize) {}
// }

// vim:ts=2 sw=2
