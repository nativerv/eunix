use std::fmt;

use super::{fs::{Filesystem, AddressSize}, virtfs::VirtFsFilesystem};

#[derive(Debug, Clone)]
struct Binary(fn(Vec<String>) -> AddressSize);

impl fmt::Display for Binary {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(f, "{:?}", self)
  }
}

fn default_binary(_: Vec<String>) -> AddressSize {
  0
}

impl Default for Binary {
  fn default() -> Self {
    Self(default_binary)
  }
}

pub struct BinFilesytem {
  virtfs: VirtFsFilesystem<Binary>,
}

impl BinFilesytem {
  pub fn new() -> Self {
    Self { 
      virtfs: VirtFsFilesystem::new("binfs"),
    }
  }
}

impl Filesystem for BinFilesytem {
  fn create_file(&mut self, pathname: &str)
    -> Result<super::fs::VINode, super::kernel::Errno> {
    self.virtfs.create_file(pathname)
  }

  fn read_file(&mut self, pathname: &str, count: super::fs::AddressSize)
    -> Result<Vec<u8>, super::kernel::Errno> {
    self.virtfs.read_file(pathname, count)
  }

  fn write_file(&mut self, pathname: &str, data: &[u8])
    -> Result<super::fs::VINode, super::kernel::Errno> {
    self.virtfs.write_file(pathname, data)
  }

  fn read_dir(&mut self, pathname: &str)
    -> Result<super::fs::VDirectory, super::kernel::Errno> {
    self.virtfs.read_dir(pathname)
  }

  fn stat(&mut self, pathname: &str)
    -> Result<super::fs::FileStat, super::kernel::Errno> {
    self.virtfs.stat(pathname)
  }

  fn change_mode(&mut self, pathname: &str, mode: super::fs::FileMode)
    -> Result<(), super::kernel::Errno> {
    self.virtfs.change_mode(pathname, mode)
  }

  fn lookup_path(&mut self, pathname: &str)
    -> Result<super::fs::VINode, super::kernel::Errno> {
    self.virtfs.lookup_path(pathname)
  }

  fn name(&self) -> String {
    String::from("binfs")
  }

  fn as_any(&mut self) -> &mut dyn std::any::Any {
    self
  }
}

// vim:ts=2 sw=2
