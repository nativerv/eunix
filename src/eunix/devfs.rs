use std::io::Error;
use std::time::SystemTime;

use crate::eunix::kernel::Kernel;
use crate::{eunix::fs::Filesystem, machine::DeviceTable};

use super::fs::{AddressSize, VDirectoryEntry, VINode};

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
    Self {
      devices,
      inodes: devices.iter().map(|(&name, &device)| INode {
        // r = read
        // w = write
        // x = execute
        // m = mode (00 = file, 01 = directory, 10 = device, 11 = system)
        // u = used
        //          u mm rwx rwx rwx
        mode: 0b00001_00_111_000_000,
        links_count: 1,
        uid: 0,
        gid: 0,
        file_size: 0,
        atime: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis(),
        mtime: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis(),
        ctime: SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_millis(),
      })
    }
  }
}

impl<'a> Filesystem for DeviceFilesystem<'a> {
  fn read_bytes(&self, pathname: &str, count: AddressSize) -> Result<&[u8], Error> {}

  fn write_bytes(&mut self, pathname: &str) -> Result<(), Error> {}

  fn read_dir(&self, pathname: &str) -> &[VDirectoryEntry] {
    let mut tty_devices_number: u32 = 0;
    let mut block_devices_number: u32 = 0;

    self
      .devices
      .iter()
      .zip(1..)
      .map(|((&name, &device), letter_number)| {
        VDirectoryEntry {
          name: match device {
            crate::machine::VirtualDevice::BlockDevice => {
              block_devices_number += 1;
              "sd" + ((64 + block_devices_number) as char)
            }
            crate::machine::VirtualDevice::TTYDevice => {
              tty_devices_number += 1;
              "tty" + tty_devices_number
            }
          },
          num_inode: letter_number, // TODO: FIXME
        }
      })
  }

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&self, pathname: &str) -> VINode {}

  fn get_name(&self) -> String {
    "devfs".to_owned()
  }
}

impl DeviceFilesystem {
  fn mkfs(percent_inodes: u32, block_size: AddressSize) {}
}

// vim:ts=2 sw=2
