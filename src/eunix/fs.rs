use std::collections::BTreeMap;
use core::fmt::Debug;

use super::kernel::Errno;

pub type AddressSize = u32;
pub type Id = u16;

/// _По тонкому льду ((нет))_
///  
/// Use max address as indicator of no next block
/// Can be invalid in theory if we use exactly 2047 gigs of blocks,
/// after which the whole fs will not work anymore so who caresi guessb.
#[allow(dead_code)]
pub const NO_ADDRESS: AddressSize = AddressSize::MAX;

#[allow(dead_code)]
pub const NOBODY: Id = Id::MAX;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileMode(pub u16);

impl Default for FileMode {
  fn default() -> Self {
      Self(0b0_000_000_110_000_000)
  }
}

impl FileMode {
  #[allow(dead_code)]
  pub fn new(raw: u16) -> Self {
    Self(raw)
  }

  #[allow(dead_code)]
  pub fn zero() -> Self {
    Self(0b0000000_000_000_000)
  }

  pub fn free(&self) -> u8 {
    let mut current = format!("{:016b}", self.0);

    u8::from_str_radix(&current[0..1], 2).expect(&format!("can't parse in free: {}", &current))
  }
  
  pub fn r#type(&self) -> u8 {
    let mut current = format!("{:016b}", self.0);

    u8::from_str_radix(&current[4..7], 2).expect(&format!("can't parse in type: {}", &current))
  }

  pub fn user(&self) -> u8 {
    let mut current = format!("{:016b}", self.0);

    u8::from_str_radix(&current[7..10], 2).expect(&format!("can't parse in user: {}", &current))
  }

  pub fn group(&self) -> u8 {
    let mut current = format!("{:016b}", self.0);

    u8::from_str_radix(&current[10..13], 2).expect(&format!("can't parse in group: {}", &current))
  }

  pub fn others(&self) -> u8 {
    let mut current = format!("{:016b}", self.0);
    
    u8::from_str_radix(&current[13..16], 2).expect(&format!("can't parse in others: {}", &current))
  }

  pub fn with_free(&self, mask: u8) -> Self {
    let mut current = format!("{:016b}", self.0);
    let mask = format!("{:01b}", mask);

    current.replace_range(0..1, &mask);
    Self(u16::from_str_radix(&current, 2).expect(&format!("can't parse in free: {}", &current)))
  }
  
  pub fn with_type(&self, mask: u8) -> Self {
    let mut current = format!("{:016b}", self.0);
    let mask = format!("{:03b}", mask);

    current.replace_range(4..7, &mask);
    Self(u16::from_str_radix(&current, 2).expect(&format!("can't parse in type: {}", &current)))
  }

  pub fn with_user(&self, mask: u8) -> Self {
    let mut current = format!("{:016b}", self.0);
    let mask = format!("{:03b}", mask);

    current.replace_range(7..10, &mask);
    Self(u16::from_str_radix(&current, 2).expect(&format!("can't parse in user: {}", &current)))
  }

  pub fn with_group(&self, mask: u8) -> Self {
    let mut current = format!("{:016b}", self.0);
    let mask = format!("{:03b}", mask);

    current.replace_range(10..13, &mask);
    Self(u16::from_str_radix(&current, 2).expect(&format!("can't parse in group: {}", &current)))
  }

  pub fn with_others(&self, mask: u8) -> Self {
    let mut current = format!("{:016b}", self.0);
    let mask = format!("{:03b}", mask);
    
    current.replace_range(13..16, &mask);
    Self(u16::from_str_radix(&current, 2).expect(&format!("can't parse in others: {}", &current)))
  }

  pub fn get_raw(&self) -> u16 {
    self.0
  }

  /// gets the bit at position `n`. Bits are numbered from 0 (least significant) to 31 (most significant).
  fn get_bit_at(input: u32, n: u8) -> bool {
    if n < 32 {
      input & (1 << n) != 0
    } else {
      false
    }
  }
}

impl std::ops::Add for FileMode {
  type Output = Self;

  fn add(self, rhs: Self) -> Self::Output {
    Self(self.0 + rhs.0)
  }
}

pub type FileDescriptor = AddressSize;

pub struct FileStat {
  mode: FileMode,
  size: AddressSize,
  inode_number: AddressSize,
  links_count: AddressSize,
  uid: u16,
  gid: u16,
  block_size: AddressSize,
}

#[derive(Debug)]
pub enum OpenMode {
  Read,
  Write,
  ReadWrite,
}

#[derive(Debug)]
pub struct OpenFlags {
  mode: OpenMode,
  create: bool,
  append: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct VDirectory {
  entries: Vec<VDirectoryEntry>,
}

#[derive(Debug)]
pub struct VDirectoryEntry {
  inode_number: AddressSize,
  name: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct VINode {
  pub mode: FileMode,
  pub links_count: AddressSize,
  pub uid: Id,
  pub gid: Id,
  pub file_size: AddressSize,
  pub atime: u32,
  pub mtime: u32,
  pub ctime: u32,
  pub number: AddressSize,
}

pub trait Filesystem {
  // Получить count байт из файловой
  // системы по указанному
  // pathname_from_fs_root,
  // либо ошибку если pathname_from_fs_root
  // не существует
  fn create_file(&self)
    -> Result<VINode, Errno>;

  fn read_file(&self, inode_number: AddressSize, count: AddressSize)
    -> Result<Vec<u8>, Errno>;

  fn write_file(&mut self, inode_number: AddressSize, data: &[u8])
    -> Result<(), Errno>;

  fn read_dir(&self, inode_number: AddressSize)
    -> Result<VDirectory, Errno>;

  fn stat(&mut self, inode_number: AddressSize)
    -> Result<FileStat, Errno>;

  fn change_mode(&mut self, inode_number: AddressSize, mode: FileMode)
    -> Result<(), Errno>;

  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&mut self, pathname: &str)
    -> Result<VINode, Errno>;

  fn get_name(&self)
    -> String;
}

impl Debug for dyn Filesystem {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
      write!(f, "Filesystem{{{}}}", self.get_name())
  }
}

#[derive(Debug)]
pub struct FileDescription {
  inode: VINode,
  flags: OpenFlags,
}

#[derive(Debug)]
pub struct VFS<'a> {
  pub mount_points: BTreeMap<&'a str, Box<dyn Filesystem>>,
  pub open_files: BTreeMap<&'a str, FileDescription>,
}

pub enum RegisteredFilesystem {
  devfs,
  // procfs(ProcessFilesystem),
  // sysfs(SysFilesystem),
  e5fs,
  // tmpfs(MemFilesystem),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn file_mode_works() {
    let expected: u16 = 0b1_000_011_101_110_001;

    let filemode = FileMode::zero()
      .with_free(0b1)
      .with_type(0b011)
      .with_user(0b101)
      .with_group(0b110)
      .with_others(0b001);

    assert_eq!(filemode.get_raw(), expected);
  }
}

// vim:ts=2 sw=2
