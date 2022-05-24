// use std::{fmt, collections::BTreeMap};
//
// use super::{fs::{Filesystem, AddressSize}, kernel::Args};
//
// #[derive(Debug, Clone)]
// struct Binary(fn(Args) -> AddressSize);
//
// impl fmt::Display for Binary {
//   fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//       write!(f, "{:?}", self)
//   }
// }
//
// fn default_binary(_: Args) -> AddressSize {
//   0
// }
//
// impl Default for Binary {
//   fn default() -> Self {
//     Self(default_binary)
//   }
// }
//
// pub struct BinFilesytem {
//   paths: BTreeMap<String, Binary>
// }
//
// impl Filesystem for BinFilesytem {
//   fn create_file(&mut self, pathname: &str)
//   -> Result<super::fs::VINode, super::kernel::Errno> {
//       todo!()
//   }
//
//   fn create_dir(&mut self, pathname: &str)
//   -> Result<super::fs::VINode, super::kernel::Errno> {
//       todo!()
//   }
//
//   fn read_file(&mut self, pathname: &str, count: AddressSize)
//   -> Result<Vec<u8>, super::kernel::Errno> {
//       todo!()
//   }
//
//   fn write_file(&mut self, pathname: &str, data: &[u8])
//   -> Result<super::fs::VINode, super::kernel::Errno> {
//       todo!()
//   }
//
//   fn read_dir(&mut self, pathname: &str)
//   -> Result<super::fs::VDirectory, super::kernel::Errno> {
//       todo!()
//   }
//
//   fn stat(&mut self, pathname: &str)
//   -> Result<super::fs::FileStat, super::kernel::Errno> {
//       todo!()
//   }
//
//   fn change_mode(&mut self, pathname: &str, mode: super::fs::FileMode)
//   -> Result<(), super::kernel::Errno> {
//       todo!()
//   }
//
//   fn lookup_path(&mut self, pathname: &str)
//   -> Result<super::fs::VINode, super::kernel::Errno> {
//       todo!()
//   }
//
//   fn name(&self) -> String {
//       todo!()
//   }
//
//   fn as_any(&mut self) -> &mut dyn std::any::Any {
//       todo!()
//   }
// }

use std::{fmt, rc::Rc, borrow::Borrow};

use super::{fs::{Filesystem, AddressSize}, virtfs::{VirtFsFilesystem, Payload}, kernel::{Args, Kernel, Errno}};

type BinaryFn = fn(Args, &mut Kernel) -> AddressSize;

#[derive(Clone)]
pub struct Binary(pub BinaryFn);

impl fmt::Debug for Binary {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      // let fun = self.0 as ;
      let fun: fn(_, &'static mut _) -> _ = self.0;
      write!(f, "{:?}", fun)
  }
}

impl fmt::Display for Binary {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(f, "{:?}", self)
  }
}


fn default_binary(_: Args, _: &mut Kernel) -> AddressSize {
  0
}

impl Default for Binary {
  fn default() -> Self {
    Self(default_binary)
  }
}

pub struct BinFilesytem {
  pub virtfs: VirtFsFilesystem<Binary>,
}

impl BinFilesytem {
  pub fn new() -> Self {
    Self {
      virtfs: VirtFsFilesystem::new("binfs", 4),
    }
  }
  pub fn write_binary(&mut self, pathname: &str, binary_fn: BinaryFn)
    -> Result<super::fs::VINode, super::kernel::Errno> {
      let vinode = self.lookup_path(pathname)?;
      self.virtfs.write_payload(&Payload::File(Binary(binary_fn)), vinode.number)?;

      Ok(vinode)
  }
  // pub fn exec_binary(&mut self, pathname: &str, kernel: &mut Kernel)
  //   -> Result<AddressSize, super::kernel::Errno> {
  //     let vinode = self.lookup_path(pathname)?;
  //
  //     let binary = match self.virtfs.read_payload(vinode.number) {
  //         Ok(Payload::File(binary)) => binary.0(),
  //         Ok(Payload::Directory(_)) => return Err(Errno::EISDIR(format!("binfs: is a directory: {pathname}"))),
  //         Err(errno) => return Err(errno),
  //     }
  //
  //     Ok(vinode)
  //   }
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

  fn create_dir(&mut self, pathname: &str)
    -> Result<super::fs::VINode, super::kernel::Errno> {
    self.virtfs.create_dir(pathname)
  }
}

// vim:ts=2 sw=2
