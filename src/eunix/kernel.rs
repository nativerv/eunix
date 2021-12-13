use crate::eunix::fs::{FileDescription, FileDescriptor, VFS};
use crate::machine::{DeviceTable, VirtualDevice};
use std::collections::BTreeMap;

use super::fs::{AddressSize, OpenFlags, VDirectoryEntry, Filesystem, RegisteredFilesystem};

#[derive(Debug)]
pub enum Errno {
  EACCES,
  EPERM,
  EISDIR,
  ENOTDIR,
  ENAMETOOLONG,
  ENOSYS,
  ENOENT,
  EIO,
  EINVAL,
}

#[derive(Debug)]
pub struct Process<'a> {
  file_descriptors: BTreeMap<FileDescriptor, FileDescription>,
  uid: i32,
  binary: &'a str,
}

impl Process<'_> {}

#[derive(Debug)]
pub struct Kernel<'a> {
  vfs: VFS<'a>,
  processes: Vec<Process<'a>>,
  current_process: u32,
  block_devices: DeviceTable,
  // registered_filesystems: BTreeMap<>,
}

impl<'a> Kernel<'a> {
  pub fn new(devices: &'a DeviceTable) -> Self {
    Self {
      vfs: VFS {
        mount_points: BTreeMap::new(),
        open_files: BTreeMap::new(),
      },
      processes: Vec::new(),
      current_process: 1,
      block_devices: devices
        .into_iter()
        .filter(|(_, device_type)| **device_type == VirtualDevice::BlockDevice)
        .map(|(path, device_type)| ((*path).clone(), *device_type))
        .collect(),
    }
  }
  fn get_block_devices(&self) -> &DeviceTable {
    &self.block_devices
  }
  fn get_current_process(&self) -> u32 {
    self.current_process
  }
}

impl <'a> Kernel<'a> {
  pub fn open(&mut self, pathname: &str, flags: OpenFlags) -> Result<FileDescriptor, Errno> {
    todo!();
  }
  pub fn read(&self, file_descriptor: FileDescriptor, count: AddressSize) -> Result<&'a [u8], Errno> {
    todo!();
  }
  pub fn write(&mut self, file_descriptor: FileDescriptor, buffer: Vec<u8>) -> Result<AddressSize, Errno> {
    todo!();
  }
  pub fn chmod(&mut self, file_descriptor: FileDescriptor, new_perms: Vec<u8>) -> Result<(), Errno> {
    todo!();
  }
  pub fn getdents(&self, file_descriptor: FileDescriptor) -> Result<&'a [VDirectoryEntry<'a>], Errno> {
    // let process: Process = self.processes.get((self.get_current_process())).unwrap();
    //
    // let &file_description = process.file_descriptors.get(&file_descriptor).unwrap();

    todo!();
  }
  pub fn mount(&mut self, source: &str, target: &str, filesystem: RegisteredFilesystem) -> Result<(), Errno> {
    // self.vfs.mount_points.append((target, match filesystem {
    //   RegisteredFilesystem::e5fs => {
    //     todo!();
    //     // crate::eunix::e5fs::E5FSFilesystem::new()
    //   },
    //   RegisteredFilesystem::devfs => {
    //     crate::eunix::devfs::DeviceFilesystem::new(self.get_block_devices())
    //   },
    // }));

    todo!();
  }
  pub fn umount(target: &str) -> Result<(), Errno> {
    todo!();
  }
}

// vim:ts=2 sw=2
