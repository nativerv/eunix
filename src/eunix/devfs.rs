use std::any::Any;
use std::collections::BTreeMap;
use std::time::SystemTime;

use crate::eunix::kernel::Kernel;
use crate::machine::VirtualDeviceType;
use crate::eunix::fs::Filesystem;
use crate::util::unixtime;

use super::fs::{AddressSize, VDirectoryEntry, VINode, VDirectory, VFS, FileMode, FileStat, FileModeType};
use super::kernel::{Errno, KernelDeviceTable};

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
  atime: u32,
  mtime: u32,
  ctime: u32,
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

pub struct Block<'a> {
  is_free: bool,
  data: &'a [u8],
  next_block: AddressSize,
}

pub struct DeviceFilesystem {
  device_table: KernelDeviceTable,
  inodes: Vec<INode>,
}

impl DeviceFilesystem {
  pub fn new(devices: &KernelDeviceTable) -> Self {
    let mut root_inode = vec![INode::default()];
    root_inode.get_mut(0).unwrap().mode = root_inode
      .get(0)
      .unwrap()
      .mode
      .with_type(FileModeType::Dir as u8);
    let rest_inodes = devices.devices
      .iter()
      .enumerate()
      .map(|(device_number, (_path, _device))| INode {
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
        mode: FileMode::new(0b0_000_000_111_000_000),
        links_count: 1,
        file_size: 0,
        uid: 0,
        gid: 0,
        atime: unixtime(),
        mtime: unixtime(),
        ctime: unixtime(), 
        number: device_number as AddressSize + 1,
      }).collect::<Vec<INode>>();

    let inodes = root_inode
      .into_iter()
      .chain(rest_inodes.into_iter())
      .collect();

    Self {
      device_table: devices.clone(),
      inodes,
    }
  }
  
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

  pub(crate) fn device_by_path(&self, pathname: &str) -> Result<String, Errno> {
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

  fn read_dir(&mut self, pathname: &str) -> Result<VDirectory, Errno> {
    let mut tty_devices_count = 0;
    let mut block_devices_count = 0;

    let (everything_else, _) = VFS::split_path(pathname)?;

    // TODO: FIXME: remove /. when .. and . is implemented 
    if pathname != "/" && pathname != "/." {
      return Err(Errno::ENOENT(String::from("no such file or directory")))
    }

    Ok(
      VDirectory {
        entries: self.device_names()
          .iter()
          .enumerate()
          .map(|(device_number, (_realpath, name))| {
            (name.to_owned(), VDirectoryEntry::new(device_number as AddressSize, name))
          })
          .collect()
      }
    )
  }

  fn stat(&mut self, pathname: &str)
    -> Result<super::fs::FileStat, Errno> {
    let VINode {
      mode,
      file_size,
      links_count,
      uid,
      gid,
      number,
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
    })
  }

  fn change_mode(&mut self, pathname: &str, mode: super::fs::FileMode)
    -> Result<(), Errno> {
    Err(Errno::EPERM(String::from("operation not permitted")))
  }

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&mut self, pathname: &str) -> Result<VINode, Errno> {
    let (everything_else, final_component) = VFS::split_path(pathname)?;
    let dir = self.read_dir("/")?; // TODO: FIXME: magic string

    let inode_number = if final_component == "." {
      0
    } else {
      dir.entries.get(&final_component).ok_or(Errno::ENOENT(String::from("no such file or directory 2")))?.inode_number
    };
    
    self.inodes
      .iter()
      .find(|inode| inode.number == inode_number)
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
