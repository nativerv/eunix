use std::any::Any;
use std::collections::BTreeMap;
use std::time::SystemTime;

use crate::eunix::kernel::Kernel;
use crate::machine::VirtualDeviceType;
use crate::eunix::fs::Filesystem;
use crate::util::unixtime;

use super::fs::{AddressSize, VDirectoryEntry, VINode, VDirectory, VFS, FileMode, FileStat, FileModeType};
use super::kernel::{Errno, KernelDeviceTable, UnixtimeSize, Times};

pub struct DirectoryEntry<'a> {
  inode_address: AddressSize,
  name: &'a str,
  next_dir_entry_offset: AddressSize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct INode {
  mode: FileMode,
  links_count: AddressSize,
  uid: u16,
  gid: u16,
  file_size: AddressSize,
  atime: UnixtimeSize,
  mtime: UnixtimeSize,
  ctime: UnixtimeSize,
  btime: UnixtimeSize,
  number: AddressSize,
}
impl From<INode> for VINode {
  fn from(inode: INode) -> Self {
    Self {
      mode: inode.mode,
      links_count: inode.links_count,
      file_size: inode.file_size,
      uid: inode.uid,
      gid: inode.gid,
      atime: inode.atime,
      ctime: inode.ctime,
      mtime: inode.mtime,
      btime: inode.btime,
      number: inode.number,
    }
  }
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

pub struct DeviceFilesystem {
  device_table: KernelDeviceTable,
  inodes: Vec<INode>,
}

impl DeviceFilesystem {
  pub fn new(device_table: &KernelDeviceTable) -> Self {
    let inodes = vec![INode {
      mode: FileMode::new(0b0_000_001_111_101_101),
      links_count: 2,
      file_size: 0,
      uid: 0,
      gid: 0,
      atime: unixtime(),
      mtime: unixtime(),
      ctime: unixtime(), 
      btime: unixtime(), 
      number: 0,
    }];
    let rest_inodes = device_table
      .devices
      .iter()
      .enumerate()
      .map(|(device_number, (_path, (dev_type, _1)))| INode {
        //    free?
        ///   | unused
        ///   | |   filetype
        ///   | |   |   user
        ///   | |   |   |   group
        ///   | |   |   |   |   others
        ///   | |   |   |   |   |
        ///   f xxx ttt rwx rwx rwx
        /// 0b0_000_000_110_000_000
        /// Where:
        /// filetype:
        ///   000 - file   100 - char
        ///   001 - dir    101 - unused
        ///   010 - sys    110 - unused
        ///   011 - block  111 - unused
        mode: FileMode::new(0b0_000_011_110_000_000).with_file_type(
          match dev_type {
            VirtualDeviceType::BlockDevice => FileModeType::Block,
            VirtualDeviceType::TTYDevice => FileModeType::Char,
          } as u8
        ),
        links_count: 1,
        file_size: 0,
        uid: 0,
        gid: 0,
        atime: unixtime(),
        mtime: unixtime(),
        ctime: unixtime(), 
        btime: unixtime(), 
        number: device_number as AddressSize + 1,
      }).collect::<Vec<INode>>();

    let inodes = inodes
      .into_iter()
      .chain(rest_inodes.into_iter())
      .collect();

    Self {
      device_table: device_table.clone(),
      inodes,
    }
  }

  /// Returns: Map of `name -> realpath`
  /// Like:
  /// "sda" -> "/home/user/disk.enxvd"
  pub fn device_names(&self) -> BTreeMap<String, String> {
    let mut tty_devices_count = 0;
    let mut block_devices_count = 0;

    self.device_table.devices
      .iter()
      .enumerate()
      .map(|(_device_number, (realpath, (device_type, _)))| {
        let name = match device_type {
          VirtualDeviceType::BlockDevice => {
            block_devices_count += 1;
            format!("sd{}", char::from_u32(96u32 + block_devices_count).unwrap())
          }
          VirtualDeviceType::TTYDevice => {
            tty_devices_count += 1;
            format!("tty{}", tty_devices_count)
          }
        };
        (name.to_owned(), realpath.to_owned())
      })
      .collect()
  }

  pub(crate) fn device_by_pathname(&self, pathname: &str) -> Result<String, Errno> {
    let (_everything_else, final_component) = VFS::split_path(pathname)?;
    let device_names = self.device_names();
    let realpath = device_names.get(&final_component).ok_or(Errno::ENOENT(String::from("no device corresponds to that name")))?;

    Ok(realpath.to_owned())
  }
}

impl Filesystem for DeviceFilesystem {
  fn create_file(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
    Err(Errno::EPERM(String::from("operation not permitted")))
  }

  fn create_dir(&mut self, pathname: &str)
    -> Result<VINode, Errno> {
        todo!()
    }

  fn read_file(&mut self, pathname: &str, count: AddressSize) -> Result<Vec<u8>, Errno> {
    Err(Errno::EPERM(String::from("devfs read_bytes: permission denied")))
  }

  fn write_file(&mut self, pathname: &str, data: &[u8]) -> Result<VINode, Errno> {
    Err(Errno::EPERM(String::from("devfs write_bytes: permission denied")))
  }

  fn read_dir(&self, pathname: &str) -> Result<VDirectory, Errno> {
    // TODO: FIXME: remove /. when .. and . is implemented 
    if pathname != "/" && pathname != "/." && pathname != "/.." { // OLD
    // if pathname != "/" {
      return Err(Errno::ENOENT(String::from("no such file or directory")))
    }

    Ok(
      VDirectory {
        entries: self
          .device_names()
          .iter()
          .zip(1..)
          .map(|((name, _), device_number)| {
            (name.to_owned(), VDirectoryEntry::new(device_number as AddressSize, name))
          })
          .collect()
      }
    )
  }

  fn stat(&self, pathname: &str)
    -> Result<super::fs::FileStat, Errno> {
    let VINode {
      mode,
      file_size,
      links_count,
      uid,
      gid,
      number,
      atime,
      mtime,
      ctime,
      btime,
      ..
    } = self.lookup_path(pathname)?; 

    Ok(FileStat {
      mode,
      size: file_size,
      inode_number: number,
      links_count,
      uid,
      gid,
      block_size: 0, // TODO: FIXME: magic number
      atime,
      mtime,
      ctime, 
      btime, 
    })
  }

  fn change_mode(&mut self, pathname: &str, mode: super::fs::FileMode)
    -> Result<(), Errno> {
    Err(Errno::EPERM(String::from("operation not permitted")))
  }

  fn change_times(&mut self, pathname: &str, times: Times)
    -> Result<(), Errno> {
    todo!()
  }

  // Поиск файла в файловой системе. Возвращает INode файла.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&self, pathname: &str) -> Result<VINode, Errno> {
    let (_, final_component) = VFS::split_path(pathname)?;
    let dir = self.read_dir("/")?; // TODO: FIXME: magic string

    let inode_number = if final_component == "/." || final_component == "/.." || final_component == "/" {
      0
    } else {
      dir
        .entries
        .get(&final_component).ok_or(Errno::ENOENT(String::from("no such file or directory 2")))?.inode_number
    };

    self.inodes
      .get(inode_number as usize)
      .map(|&inode| inode.into())
      .ok_or(Errno::EIO(String::from("devfs::lookup_path: can't find inode from dir")))
  }

fn name(&self) -> String {
    String::from("devfs")
  }

fn as_any(&mut self) -> &mut dyn Any {
    self
  }
}

// impl DeviceFilesystem {
//   fn mkfs(percent_inodes: u32, block_size: AddressSize) {}
// }

// vim:ts=2 sw=2
