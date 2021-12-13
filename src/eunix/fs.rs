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

#[allow(dead_code)]
pub const NOLINKS: u32 = u32::MAX;

#[derive(Debug, PartialEq, Eq)]
pub struct FileMode(pub u16);

impl Default for FileMode {
  fn default() -> Self {
      Self(0b0000000_110_000_000)
  }
}

// impl std::ops::Add for FileMode {
//   fn add(self, rhs: Self) -> Self::Output {
//       Self(self.0 + rhs)
//   }
// }

impl FileMode {
  pub fn new(raw: u16) -> Self {
    Self(raw)
  }
  pub fn zero() -> Self {
    Self(0b0000000_000_000_000)
  }
}

impl std::ops::Add for FileMode {
  type Output = Self;

  fn add(self, rhs: Self) -> Self::Output {
    Self(self.0 + rhs.0)
  }
}

pub type FileDescriptor = AddressSize;

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

#[derive(Debug)]
pub struct VDirectoryEntry<'a> {
  num_inode: AddressSize,
  name: &'a str,
}

#[derive(Debug)]
pub struct VINode {
  mode: u16,
  links_count: AddressSize,
  uid: u32,
  gid: u32,
  file_size: AddressSize,
  atime: u32,
  mtime: u32,
  ctime: u32,
}

pub trait Filesystem {
  // Получить count байт из файловой
  // системы по указанному
  // pathname_from_fs_root,
  // либо ошибку если pathname_from_fs_root
  // не существует
  fn read_bytes( &self, pathname: &str, count: AddressSize)
    -> Result<&[u8], Errno>;

  fn write_bytes(&mut self, pathname: &str)
    -> Result<(), Errno>;

  fn read_dir(&self, pathname: &str)
    -> &[VDirectoryEntry];


  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&self, pathname: &str)
    -> VINode;

  fn get_name(&self)
    -> String;

  // fn new()
  //   -> Self;
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


// vim:ts=2 sw=2
