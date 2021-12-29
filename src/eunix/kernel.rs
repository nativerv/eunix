use crate::eunix::devfs::DeviceFilesystem;
use crate::eunix::fs::{FileDescription, FileDescriptor, VFS, OpenMode, MountedFilesystem};
use crate::*;
use crate::machine::{MachineDeviceTable, VirtualDeviceType};
use std::any::Any;
use std::collections::BTreeMap;

use super::fs::{AddressSize, OpenFlags, VDirectoryEntry, Filesystem, FilesystemType, VDirectory};

#[derive(Debug, PartialEq, Eq)]
pub enum Errno {
  /// Permission denied
  EACCES(&'static str),
  /// Operation not permitted
  EPERM(&'static str),
  /// Is a directory
  EISDIR(&'static str),
  /// Not a directory
  ENOTDIR(&'static str),
  /// Name too long
  ENAMETOOLONG(&'static str),
  /// Not implemented
  ENOSYS(&'static str),
  /// No such entity
  ENOENT(&'static str),
  /// I/O Error
  EIO(&'static str),
  /// Invalid argument
  EINVAL(&'static str),
  /// Illegal byte sequence
  EILSEQ(&'static str),
  /// No such process
  ESRCH(&'static str),
}

#[derive(Debug)]
pub struct Process {
  file_descriptors: BTreeMap<FileDescriptor, FileDescription>,
  uid: i32,
  binary: String,
}

impl Process {}

#[derive(Debug, Clone)]
pub struct KernelDeviceTable {
  /// `realpath -> (dev_type, mounted_pathname)` 
  pub devices: BTreeMap<String, (VirtualDeviceType, Option<String>)>
}
impl From<MachineDeviceTable> for KernelDeviceTable {
  fn from(mach_dev_table: MachineDeviceTable) -> Self {
    Self {
      devices: mach_dev_table.devices
        .iter()
        .map(|(realpath, dev_type)| (realpath.to_owned(), (dev_type.to_owned(), Option::<String>::None)))
        .collect(),
    }
  }
}

#[derive(Debug)]
pub struct Kernel {
  pub vfs: VFS,
  pub processes: Vec<Process>,
  pub current_process_id: u32,
  pub device_table: KernelDeviceTable,
  // registered_filesystems: BTreeMap<>,
}

impl Kernel {
  pub fn new(devices: &MachineDeviceTable) -> Self {
    Self {
      vfs: VFS {
        mount_points: BTreeMap::new(),
        open_files: BTreeMap::new(),
      },
      processes: Vec::new(),
      current_process_id: 1,
      device_table: devices.clone().into(),
    }
  }
  pub fn devices(&self) -> &KernelDeviceTable {
    &self.device_table
  }
  pub fn current_process_id(&self) -> u32 {
    self.current_process_id
  }
  pub fn vfs(&self) -> &VFS {
    &self.vfs
  }
  pub fn processes(&self) -> &Vec<Process> {
    &self.processes
  }
}

impl Kernel {
  pub fn open(&mut self, pathname: &str, flags: OpenFlags) -> Result<FileDescriptor, Errno> {
    todo!();
  }
  pub fn read(&self, file_descriptor: FileDescriptor, count: AddressSize) -> Result<Vec<u8>, Errno> {
    todo!();
  }
  pub fn write(&mut self, file_descriptor: FileDescriptor, buffer: Vec<u8>) -> Result<AddressSize, Errno> {
    todo!();
  }
  pub fn chmod(&mut self, file_descriptor: FileDescriptor, new_perms: Vec<u8>) -> Result<(), Errno> {
    todo!();
  }
  pub fn getdents(&mut self, file_descriptor: FileDescriptor) -> Result<VDirectory, Errno> {
    let process = self.processes.get(self.current_process_id() as usize).ok_or(Errno::ESRCH("cannot get current process"))?;
    let FileDescription {
      inode: _inode,
      flags,
      pathname,
    } = process.file_descriptors.get(&file_descriptor).ok_or(Errno::ENOENT("no such file descriptor"))?;

    // Guard for OpenMode
    match flags.mode() {
      OpenMode::Write => return Err(Errno::EACCES("getdents: permission denied")),
      OpenMode::ReadWrite | OpenMode::Read => (),
    }

    self.vfs.read_dir(pathname)
  }
  pub fn mount(&mut self, source: &str, target: &str, fs_type: FilesystemType) -> Result<(), Errno> {
    if let Some(_) = self.vfs.mount_points.get(target) {
      return Err(Errno::EINVAL("mount point already taken"))
    }

    let mounted_fs = match fs_type {
      FilesystemType::e5fs => {
        // Find device, represented by `source` pathname in VFS
        // (we store device's VFS pathname in tuple alongside device type)
        // (because i haven't found a way to do this directly with devfs)
        // (see `KernelDeviceTable`)
        // let (realpath, (_vdev_type, _vdev_path)) = self.devices().devices
        //   .iter()
        //   .find_map(|(realpath, (vdev_type, vdev_path))| {
        //     let vdev_path = vdev_path.clone()?;
        //     if vdev_path == source {
        //       Some((realpath.to_owned(), (*vdev_type, Some(vdev_path))))
        //     } else {
        //       None
        //     }
        //   }).ok_or(Errno::ENOENT("no such device"))?;

        let (mount_point, internal_path) = self.vfs.match_mount_point(source)?;
        let mounted_fs = self.vfs.mount_points.get_mut(&mount_point).expect("VFS::lookup_path: we know that mount_point exist");  

        let realpath = if mounted_fs.r#type == FilesystemType::devfs {
          let devfs = mounted_fs.driver
            .as_any()
            .downcast_ref::<DeviceFilesystem>()
            .expect("we know that mounted_fs.driver === instanceof DeviceFilesystem");

          devfs.device_by_path(&internal_path)?
        } else {
          return Err(Errno::EINVAL("source is not a device"));
        };

        // Instantiate new e5fs around device that we've found
        let e5fs = eunix::e5fs::E5FSFilesystem::from(realpath.as_str())?;

        MountedFilesystem {
          r#type: FilesystemType::e5fs,
          driver: Box::new(e5fs),
        }
      },
      FilesystemType::devfs => {
        let devfs = eunix::devfs::DeviceFilesystem::new(self.devices());

        MountedFilesystem {
          r#type: FilesystemType::devfs,
          driver: Box::new(devfs),
        }
      },
    };

    // Finally, insert constructed mounted_fs
    self.vfs.mount_points.insert(target.to_owned(), mounted_fs);
    if fs_type == FilesystemType::devfs {
      // let (_, device_name) = VFS::split_path(source)?;
      // let fs = self.vfs.mount_points
      //   .get(target)
      //   .expect(&format!("we know that '{}' mount point exist", target));
      // let devfs: Box<DeviceFilesystem> = unsafe { 
      //   std::mem::transmute::<Box<dyn Any>, Box<DeviceFilesystem>>(fs.driver) 
      // };
      // let device_names = Kernel::device_names(&mut self.device_table);
      // let realpath = device_names.get(device_name.as_str()).unwrap();
      // let device = self.device_table.devices.get_mut(realpath).unwrap();
      // *device = (device.0, Some(target.to_owned()));
      // self.devices.devices.insert(realpath, value);
    }

    Ok(())
  }
  pub fn umount(&mut self, target: &str) -> Result<(), Errno> {
    self.vfs.mount_points.remove(target).ok_or(Errno::ENOENT("no such mount point"))?;

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

}

// vim:ts=2 sw=2
