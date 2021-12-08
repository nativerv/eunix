use std::{collections::BTreeMap, io::Error};
use core::fmt::Debug;

pub type AddressSize = u64;
pub type FileMode = u16;
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
    -> Result<&[u8], Error>;

  fn write_bytes(&mut self, pathname: &str)
    -> Result<(), Error>;

  fn read_dir(&self, pathname: &str)
    -> &[VDirectoryEntry];


  // Поиск файла в файловой системе. Возвращает INode фала.
  // Для VFS сначала матчинг на маунт-поинты и вызов lookup_path("/mount/point") у конкретной файловой системы;
  // Для конкретных реализаций (e5fs) поиск сразу от рута файловой системы
  fn lookup_path(&self, pathname: &str)
    -> VINode;

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


// vim:ts=2 sw=2
